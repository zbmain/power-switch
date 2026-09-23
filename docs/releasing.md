# 发布说明

## 分支与版本策略

本项目参考 [CC Switch CI](https://github.com/farion1231/cc-switch/blob/main/.github/workflows/ci.yml) 与 [标签发布流程](https://github.com/farion1231/cc-switch/blob/main/.github/workflows/release.yml)，采用以下策略：

- `master` 是默认开发和发布基线；功能在短期分支开发，经 PR 检查后合并，不维护长期 `release` 分支。
- 推送 `master`、面向 `master` 的 PR、手动运行 CI 均执行质量检查，不创建 Release。
- 推送 `v<SemVer>` 标签触发 Release，例如 `v0.1.0`、`v0.2.0-rc.1`。标签提交必须属于远程 `master` 历史。
- 每个新版本都先创建 **Pre-release**，包括没有 `rc` 后缀的标签。验证完成后，由维护者手动提升正式版。
- 已发布正式版不可由流水线覆盖。不移动已发布标签，修复应发布新的补丁版本。

## 首次配置

仓库需要允许 GitHub Actions 运行。普通检查仅有 `contents: read`，发布任务单独使用 `contents: write` 和自动提供的 `GITHUB_TOKEN`，不需要配置个人访问令牌、Apple 证书或 Tauri 更新签名私钥。

维护者本机需要 Git SSH 推送权限。查看私有仓库 Actions、修改默认分支及管理 Release 时，还需 GitHub CLI 登录：

```sh
gh auth login --hostname github.com --git-protocol ssh --web
gh auth status
gh repo edit zbmain/power-switch --default-branch master
```

更改默认分支前需确保 `master` 已推送。保持仓库现有可见性；私有仓库的下载链接仅对有权限的用户开放。GitHub 托管 runner 的可用额度遵循仓库所属账号计划。

CI 使用 Node.js 22、pnpm 10.18.3、Rust 1.92.0。前端依赖采用 `--frozen-lockfile`，Rust 检查、测试和发布构建采用 `--locked`。

## 准备一个版本

在已同步的 `master` 上建立版本准备分支：

```sh
git switch master
git pull --ff-only origin master
git switch -c codex/release-v0.1.1
pnpm install --frozen-lockfile
```

以 `0.1.1` 为例，编辑 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 中的项目版本，三处必须完全一致；不要修改依赖版本来代替项目版本。更新 Cargo 锁文件中的本项目记录：

```sh
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
pnpm release:check -- v0.1.1
just check
git diff --check
git diff -- src-tauri/Cargo.lock
```

审阅锁文件，只接受与本次改动相关的变化。`release:check` 会比较上述三处以及 `Cargo.lock` 的本项目版本，并拒绝不合法标签。缺少 just 时，执行 Justfile 中对应的 pnpm/Cargo 命令。

将版本修改及必要发布说明提交，推送分支并发起 PR：

```sh
git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git diff --cached --check
git diff --cached
git commit -m "chore: prepare v0.1.1"
git push -u origin codex/release-v0.1.1
gh pr create --base master --title "Prepare v0.1.1" --body "Synchronize the application version for the next prerelease."
```

任何层级的 `.env`、`.venv`、`venv`、密钥和本地模型数据都不能暂存或提交。

## 打标签并自动发布

PR 合并且 `master` CI 成功后：

```sh
git switch master
git pull --ff-only origin master
pnpm release:check -- v0.1.1
git status --short
git tag -a v0.1.1 -m "power-switch v0.1.1"
git push origin v0.1.1
```

首次发布使用项目已有版本 `0.1.0`，将上面标签替换为 `v0.1.0`，无需先增加版本号。打标签前必须确认工作区没有未提交的发布改动。

查看运行状态：

```sh
gh run list --repo zbmain/power-switch --workflow release.yml
gh run watch RUN_ID --repo zbmain/power-switch --exit-status
gh release view v0.1.1 --repo zbmain/power-switch
```

Release 流程依次执行标签和版本检查、可复用 CI、五个平台构建、资产完整性检查、草稿上传与校验，最后才公开为预发布。CI 测试数据使用隔离目录，不运行会修改真实 WorkBuddy 等配置的手动验收工具。

### 补齐已经手动创建的 Release

如果先在 GitHub 页面发布 Release，页面只会自动附带源码 ZIP/TAR，不会编译桌面应用。对于已经存在、且没有人工上传资产的 `v0.1.0` 正式版，在 `master` 包含补建工作流后，进入 **Actions → Release → Run workflow**，选择 `master`，填写 `tag = v0.1.0`，勾选 `repair_existing_release`。也可在已登录 GitHub CLI 后运行：

```sh
gh workflow run release.yml --repo zbmain/power-switch --ref master -f tag=v0.1.0 -f repair_existing_release=true
gh run list --repo zbmain/power-switch --workflow release.yml --limit 5
```

手动运行会从指定标签检出应用源码并执行完整测试与五平台构建。只有全部产物齐全才开始上传。补建模式仅接受没有上传资产的现有正式版，源码 ZIP/TAR 不算上传资产；它不会移动标签或改变正式版状态。如果某个平台失败，请先修复构建问题，再重试；如已有部分资产上传，需人工核对后处理，不会自动覆盖正式版文件。后续版本应先推送标签，让工作流自动创建带安装包的预发布，不需在网页中提前创建 Release。

## 安装包与校验

| 系统                        | Runner           | Rust target               | 文件后缀                           |
| --------------------------- | ---------------- | ------------------------- | ---------------------------------- |
| macOS Intel + Apple Silicon | macos-14         | universal-apple-darwin    | macos-universal.dmg / .zip         |
| Windows x64                 | windows-2022     | x86_64-pc-windows-msvc    | windows-x64.msi / .zip             |
| Windows ARM64               | windows-11-arm   | aarch64-pc-windows-msvc   | windows-arm64.msi / .zip           |
| Linux x64                   | ubuntu-22.04     | x86_64-unknown-linux-gnu  | linux-x64.AppImage / .deb / .rpm   |
| Linux ARM64                 | ubuntu-22.04-arm | aarch64-unknown-linux-gnu | linux-arm64.AppImage / .deb / .rpm |

完整名称示例：`power-switch-v0.1.0-macos-universal.dmg`。每次发布必须包含 12 个非空安装产物和 1 个 `SHA256SUMS`。上传完成后，发布程序还会比较 GitHub 服务端返回的 SHA-256，校验失败不会公开草稿。

下载全部产物后，在 macOS/Linux 校验：

```sh
gh release download v0.1.0 --repo zbmain/power-switch --dir release-download
cd release-download
shasum -a 256 -c SHA256SUMS
```

Windows 可执行 `Get-FileHash .\power-switch-v0.1.0-windows-x64.msi -Algorithm SHA256`，将结果与 `SHA256SUMS` 同名记录比较。

- macOS DMG 和 ZIP 中的应用均为通用二进制，构建时通过 `lipo` 校验 Intel 与 ARM64 架构。首版没有 Apple 开发者签名和公证；可能只有构建工具所需的临时签名。确认来源与校验值后，可在系统“隐私与安全性”查看允许打开的选项，不要关闭系统的全局安全检查。
- Windows MSI 可引导安装 WebView2 并完成安装注册。ZIP 只包含应用可执行文件，需预装 WebView2；数据仍保存在用户应用目录，不保证注册 `power-switch://` 导入协议。系统可能显示未知发布者提示。
- Linux AppImage 使用前需 `chmod +x`，部分发行版需要 FUSE；DEB/RPM 由系统包管理器安装。当前 New API 登录的系统凭证持久化仅实现 macOS/Windows，Linux 的此功能限制不因打包而改变。
- 没有应用内自动更新、`latest.json` 或 Tauri 更新签名资产。升级通过 Releases 手动下载完成。

## 重跑、正式发布与故障处理

- 单个平台构建失败：检查日志修复后再发布，发布任务依赖整个构建矩阵成功，不会公开缺包版本。
- 网络导致上传失败：保留草稿及构建 artifacts，从 Actions 重新运行失败的发布任务，继续上传并校验。构建 artifacts 保留 14 天。
- 同标签运行会串行处理，不取消正在上传的任务。已公开预发布仅允许同提交、同校验值的幂等验证；重新编译产生不同二进制时不会覆盖，需发布新版本。
- 首次发布完成后，在对应真实机器验证安装、启动、图标、导入协议和主要功能。CI 成功只证明自动化检查和构建成功，不能替代人工安装验收。
- 验证通过后，在 GitHub Release 编辑页取消“预发布”标记并设为 Latest；或执行以下命令：

```sh
gh release edit v0.1.0 --repo zbmain/power-switch --prerelease=false --latest
```

- 正式版的同标签重跑会被明确拒绝。发现问题应发布 `v0.1.1` 等补丁版本，不删除或移动原版本标签。
- Actions 报 `Resource not accessible by integration` 时，检查组织策略是否允许工作流发布 Release，以及发布任务的 `contents: write` 是否保留。
- 本地 Rust 测试需要临时回环端口供模拟服务器使用；受限沙箱禁止监听时应在允许回环端口的环境运行测试，不应删除或跳过这些测试。
