# power-switch

中文桌面模型与技能管理工具。将模型应用到 WorkBuddy、Claude Code、Codex CLI 与桌面端，支持 macOS、Windows 和 Linux。

![power-switch 图标](assets/power-switch-master.png)

## 开发与构建

需要 Node.js 22+、pnpm 10、Rust 1.92+，以及 [Tauri 2 平台依赖](https://v2.tauri.app/start/prerequisites/)。macOS 需要 Xcode Command Line Tools；Windows 需要 MSVC 与 WebView2；Linux 需要 WebKitGTK 4.1。

```sh
pnpm install --frozen-lockfile
pnpm dev                  # 原生桌面开发
just check                # TS、格式、Clippy、Vitest、Cargo test
just build                # 当前平台安装包
just icons                # 从母版重新生成平台图标
```

没有安装 just 时，可直接运行 Justfile 中的 pnpm / Cargo 命令。`pnpm dev:web` 提供使用虚构数据的浏览器演示，不写入真实配置。生产安装包在 `src-tauri/target/release/bundle`；指定 Rust target 构建时，位于 `src-tauri/target/<target>/release/bundle`。

## 下载与发布

从 [GitHub Releases](https://github.com/zbmain/power-switch/releases) 下载应用。macOS 提供同时支持 Intel 和 Apple Silicon 的通用 DMG/ZIP；Windows x64/ARM64 提供 MSI/ZIP；Linux x64/ARM64 提供 AppImage、DEB 和 RPM。安装包附带 `SHA256SUMS` 校验清单。

首版未使用开发者证书签名或 Apple 公证，系统可能提示未知发布者。Windows ZIP 需要已安装 WebView2，仍使用用户应用数据目录，不是数据随身携带的便携模式。Linux 目前尚不支持 New API 系统凭证持久化，其他平台差异见 [New API 文档](docs/new-api.md)。

向 `master` 提交代码或发起 PR 会运行 CI。推送 `v*` 版本标签后，GitHub Actions 校验版本、运行测试、构建全部平台，再统一创建预发布；正式版由维护者验证后手动提升。完整操作见 [发布说明](docs/releasing.md)。应用暂不提供内置自动更新。

## 使用流程

1. 添加模型，依次填写平台名称、协议、API 地址和必填的 API Key，再从平台接口返回的清单中选择模型 ID。
2. 按服务商真实能力填写高级设置。新模型默认勾选工具调用、图像输入；已保存的关闭选项保持不变。Codex 必须填写上下文窗口；推理档位默认留空。
3. 点击“测试模型”，向当前接口发送文本 test，收到有效文本回复后才能点击“保存模型”。编辑已有模型或复制模型也必须重新测试；修改任何表单字段后，之前的通过结果失效。
4. 点击“应用到 Agent”，选择兼容的软件，核对目标文件和脱敏前后内容。
5. 点击“确认覆盖并备份”后才写入。若预览期间文件发生变化，需重新预览。
6. WorkBuddy 默认勾选“写入后打开 WorkBuddy 新建任务页”：配置写入成功后通过 `workbuddy://home` 打开新任务，请在模型列表中手动选择刚写入的模型；WorkBuddy 会记住新任务的模型选择。取消勾选则只更新模型列表。WorkBuddy 5.6.2 的 `workbuddy://switch-model?modelId=...` 已确认被接收，但不会实际切换模型，因此应用不会再把协议唤起误报为自动选中。已有任务不会切换。Codex 桌面端完全退出后重新打开并新建会话。Claude Code 新建会话读取配置。

模型卡片在“编辑”“复制模型”之后提供“测试模型”按钮。手动测试通过时名称后显示绿圈，失败时显示红圈，卡片下方显示原因或耗时。成功提示统一为“测试通过，耗时0.12 秒”。测试结果提示默认显示 5 秒，也可点击行末 × 提前关闭；关闭提示不会清除圆圈或重新锁定已通过测试的保存按钮。未主动测试的模型没有圆圈。测试结果仅保留在当前应用会话，关闭后清除；从文件或链接导入的已有模型不自动发送请求。测试不修改 Agent 配置。

测试按所选协议发送一次请求，最长等待 30 秒，输出上限 256 Token，响应不超过 1 MiB，不跟随重定向、不自动重试。测试验证文本调用，不能证明图像、工具调用或完整上下文能力；部分推理模型可能在回复正文前耗尽本次输出额度，此时保持未通过并提示原因。浏览器演示不进行真实测试，也不模拟测试通过。

“配置已写入”只表示文件回读校验成功，不等于 API 调用已验证。退出 power-switch 不影响已写入配置。备份页可预览并恢复任意记录；恢复前也会备份当前状态。记录右侧的删除按钮需二次确认，只删除该备份，不修改当前 Agent 配置；备份删除后无法恢复。

| 协议                    | 支持的 Agent       | 地址示例                     |
| ----------------------- | ------------------ | ---------------------------- |
| OpenAI Chat Completions | WorkBuddy          | `https://api.example.com/v1` |
| OpenAI Responses        | Codex CLI / 桌面端 | `https://api.example.com/v1` |
| Anthropic Messages      | Claude Code        | `https://api.example.com`    |

也接受标准完整接口地址，保存时自动规范化。不提供协议转换代理、账号轮换、云同步。显式 Codex profile、命令行参数和组织管理策略可能覆盖用户级配置。

## 路径与数据

设置中的“选择文件”“选择目录”会打开系统原生选择器，也可继续手动输入完整路径。WorkBuddy、Claude Code 选择目录时自动补齐配置文件名；Codex 选择配置目录。选择后需点击“保存设置”，不会立即写入 Agent 配置。

通过操作系统目录 API 获取实际路径；设置页显示绝对路径并允许覆盖。Windows 使用当前用户目录，例如 `C:\Users\用户名`，不依赖字面量 `~`。

| 对象           | 默认位置                                                          | 覆盖方式                                |
| -------------- | ----------------------------------------------------------------- | --------------------------------------- |
| WorkBuddy      | 用户目录 / `.workbuddy/models.json`                               | `WORKBUDDY_DATA_DIR` 或设置中的文件路径 |
| Claude Code    | 用户目录 / `.claude/settings.json`                                | 设置中的文件路径                        |
| Codex          | 用户目录 / `.codex/config.toml`                                   | `CODEX_HOME` 或设置中的目录             |
| 本应用 macOS   | `~/Library/Application Support/power-switch`                      | 系统应用数据目录                        |
| 本应用 Windows | `%LOCALAPPDATA%\power-switch`                                     | 系统应用数据目录                        |
| 本应用 Linux   | `$XDG_DATA_HOME/power-switch`，默认 `~/.local/share/power-switch` | XDG 系统目录                            |

模型库与密钥存储在本地 `models.json`，备份位于 `backups/*.json`。**这些文件含明文凭据，不应公开分享或提交版本库。** Unix 目录权限为 700，文件为 600；Windows 使用用户应用目录继承的 ACL。应用不输出原始分享链接或密钥日志。仓库忽略 `*.env`、`.venv*`、构建产物和验收产物。

WorkBuddy 保留数组或 `{ "models": [...] }` 结构、其他条目和未知字段；Claude Code 只合并模型相关 env；Codex 使用独立 provider 与 `power-switch-models.json`，保留 `auth.json`、其他 provider 和 TOML 注释。已有 Codex 目录会合并至本应用管理的目录，原文件不改写。

所有写入均先预检、备份，使用同目录临时文件替换，并回读校验。多文件中途失败会尝试回滚；发生外部编辑导致无法安全回滚时，会保留冲突文件并明确报告。异常退出可能留下 `pending` 备份记录，可在备份页检查恢复。

## 技能管理

当前版本的“技能”入口暂时禁用，显示“待开放”，以下能力保留在代码中，待完善后开放。侧边栏顺序为“模型库”“模型配置备份”“技能”“设置”。

技能页按全局、Claude Code、Codex、WorkBuddy 展示技能清单，右侧查看名称、描述、真实目录、软链接目标和 SKILL.md 原文。页面打开期间每分钟自动刷新；点击“刷新”后重新计时一分钟。切换到其他页面时停止扫描，返回技能页立即重新读取。

默认目录分别为用户目录下的 .agents/skills、.claude/skills、.codex/skills、.workbuddy/skills。自定义 Agent 配置位置后，技能目录跟随其配置所在目录；Codex、WorkBuddy 的环境变量覆盖也生效。当前实际路径显示在各分页顶部。

- **安装**：仅能安装到全局。可选择本地技能目录、SKILL.md 文件或含多个技能的目录，也支持无需登录的 HTTPS Git 仓库及 GitHub tree 子目录链接。Git 来源需要本机安装 Git。预览中勾选要安装的技能，再确认；遇到同名目录时不覆盖。一次最多安装 50 个，来源快照限 100 MiB、10000 个文件。安装来源中的软链接会被拒绝。
- **连接到 Agent**：选中全局技能，勾选目标 Agent，预览目录后确认建立软链接。修改全局内容会被所有链接它的 Agent 使用；已有同名目标不会被覆盖。Windows 创建真实软链接需要系统允许该权限（如开启开发者模式），失败时会明确提示。
- **删除**：预览后再次确认。删除 Agent 下的软链接只移除链接，保留全局内容。删除全局技能会同时列出并移除引用它的 Agent 链接。文件在确认前后发生变化时，需要重新预览。
- **技能回收目录**：删除内容移入对应技能目录下的 .power-switch-trash/操作ID，保留原目录名后缀。需要恢复时，可从此处手动移回原路径，先恢复全局源目录再恢复 Agent 链接，避免覆盖后来安装的内容。回收目录不出现在技能清单中。

技能文档以只读文本展示，power-switch 不执行其中的指令、脚本或依赖安装。

## 浏览器链接导入

安装后支持：

```text
power-switch://model/import?v=1&data=<Base64URL(JSON)>
```

JSON 示例（最多 50 条，总链接不超过 64 KiB）：

```json
{
  "models": [
    {
      "name": "我的模型",
      "protocol": "openai-chat",
      "baseUrl": "https://api.example.com/v1",
      "modelId": "my-model",
      "supportsToolCall": true,
      "supportsImages": false,
      "contextWindow": 128000,
      "reasoningLevels": []
    }
  ]
}
```

协议枚举为 `openai-chat`、`openai-responses`、`anthropic-messages`。`apiKey` 可选；链接不接受文件路径或自动应用参数。按协议、规范化地址和模型 ID 去重，默认跳过重复项，预览中可主动勾选更新。缺失密钥的更新不会清空已保存密钥。

生成测试链接：

```js
const payload = {
  models: [
    {
      name: "示例",
      protocol: "openai-chat",
      baseUrl: "https://api.example.com/v1",
      modelId: "example",
    },
  ],
};
console.log(
  "power-switch://model/import?v=1&data=" +
    Buffer.from(JSON.stringify(payload)).toString("base64url"),
);
```

网页可使用普通 `<a href="power-switch://...">` 打开。应用仅展示导入预览，确认后保存模型，应用到 Agent 需要另行确认。分享默认不含密钥；主动包含密钥后，应当将整个链接视为凭据。

## 代码结构与验收

- `src/`：React 界面、Tauri 接口、前端测试。
- `src-tauri/src/`：模型、路径、适配器、文件事务、导入及桌面命令。
- `src-tauri/tests/core.rs`：文件合并、恢复、外部修改检测、路径、导入测试。
- `src-tauri/examples/acceptance.rs`：显式启用的本机验收工具，**WorkBuddy 模式会修改真实配置**，正常开发测试不调用。
- `assets/`、`src-tauri/icons/`：生成图标母版与各平台图标。
- [首版验收记录](docs/acceptance.md)、[本轮功能增强验收](docs/enhancements.md)、[图标说明](assets/README.md)。

同一工作区的 New API 接入功能由另一个任务实现，说明见 [New API 文档](docs/new-api.md)。

独立实现，参考 [cc-switch](https://github.com/farion1231/cc-switch) 与本地 workbuddy-switch 的配置设计；参见 [第三方声明](THIRD_PARTY_NOTICES.md)。配置依据：[WorkBuddy 模型文档](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model)、[Codex 接入](https://www.kimi.com/code/docs/third-party-tools/codex.html)、[Claude Code 接入](https://www.kimi.com/code/docs/third-party-tools/claude-code.html)、[Tauri Deep Linking](https://v2.tauri.app/plugin/deep-linking/)。
