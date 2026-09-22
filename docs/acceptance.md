# 首版验收记录

日期：2026-09-22。执行环境：macOS Apple Silicon、WorkBuddy 5.5.6、Codex 0.155.0-alpha.9.2。

## 自动化验证

| 项目                            | 结果                                                     |
| ------------------------------- | -------------------------------------------------------- |
| TypeScript 类型检查             | 通过                                                     |
| Vitest                          | 14 项通过：基础界面 8 项，另一个任务的 New API 界面 6 项 |
| Cargo test                      | 36 项通过：基础核心 18 项，New API 核心及整合测试 18 项  |
| Clippy `-D warnings`            | 通过                                                     |
| Prettier / rustfmt              | 通过                                                     |
| Vite 生产构建                   | 通过                                                     |
| macOS 原生 debug / release 应用 | 构建通过                                                 |
| macOS DMG                       | 构建通过：`power-switch_0.1.0_aarch64.dmg`，约 7.5 MiB   |

覆盖配置数组/对象结构、未知字段保留、协议限制、上下文要求、Codex 目录合并、原目录与 auth.json 保留、损坏文件拒绝、文件指纹、取消不写入、恢复不存在状态、多文件第二次写入失败回滚、权限与锁、链接编码/长度/版本/重复导入和密钥脱敏。测试均使用临时目录，不修改真实 Codex 或 Claude Code 配置。

New API 模拟 HTTP 测试需允许绑定本机回环端口；沙箱限制导致的 bind PermissionDenied 不属于测试逻辑失败，允许本机端口后完整通过。

额外回归：3 MiB 配置序列化后的备份超过 8 MiB 时，仍可完成备份状态更新与完整恢复。已通过真实文件测试。

## 原生桌面交互

已在真实 Tauri 窗口完成：

- 系统 URL 调度热启动导入，中文、空格、`&` 字符正确显示。
- 关闭应用后用链接冷启动，重复项默认不勾选更新。
- 模型兼容性限制：Chat 模型只允许 WorkBuddy。
- 设置隔离目标 `/private/tmp/power-switch-ui-acceptance/models.json`。
- 确认窗口展示绝对路径、脱敏内容、二次确认文案；确认后生成真实文件及备份。
- 通过备份页面确认恢复，文件回到原本不存在状态，同时生成恢复前备份。
- 浅色与深色视觉检查，恢复“跟随系统”和默认 Agent 路径。
- 清理虚构验收模型；保留无密钥的验收备份记录。

导入热/冷启动测试使用已注册的 `.app` 和系统 `open` 调度；Windows / Linux 桌面注册与浏览器外部应用提示仍需对应平台验证。

## Codex 真实解析

运行 `scripts/verify-codex.mjs`：创建临时 CODEX_HOME，通过本应用适配器生成 provider 与 `power-switch-models.json`，启动真实 `codex app-server --stdio --strict-config`，经 initialize 和 model/list 验证自定义模型存在且为默认模型。

结果：通过。上下文窗口 128000、无推理档位、文本输入的模板被真实 Codex 解析。测试地址为无服务的本机地址，凭据为明确的虚构字符串，未进行网络推理调用，未改动本机 Codex 配置。

Claude Code 已通过隔离目录的合并与保留测试；未发起真实模型调用。

## WorkBuddy 实机验收

通过 `src-tauri/examples/acceptance.rs` 的显式 `workbuddy` 模式调用与 Tauri 命令相同的 Engine。沿用本机已有 Chat 模型与密钥，只临时修改展示名称及规范化地址；备份先于写入。日志不包含凭据。

- 写入前有 2 个模型；写入后仍有 2 个。
- 在 WorkBuddy 新建任务后，模型选择器显示 `Power Switch 验收:auto`。
- 模型列表同时保留 `fast`，无需重启 WorkBuddy。
- 已选择验收模型并输入最小消息；发送前检测到用户正在操作窗口，已暂停界面动作并询问继续时机。
- 最小调用尚待用户允许继续控制窗口；不能将“模型可见”视为“调用已通过”。
- 已确认目标文件仍与本次写入指纹一致，再通过 Engine 恢复原始备份；恢复后 SHA-256 与验收前完全相同。恢复前另建安全备份，其他条目及原始字节均保留。

验收原始备份保存在仓库外 `/private/tmp/power-switch-workbuddy-acceptance/power-switch/backups/5ab36701-29d6-4c2a-9e73-418999dbc133.json`，仅当前用户可读。恢复入口：

```sh
cargo run --manifest-path src-tauri/Cargo.toml --no-default-features --example acceptance -- restore /private/tmp/power-switch-workbuddy-acceptance 5ab36701-29d6-4c2a-9e73-418999dbc133
```

## 平台与发布范围

macOS Apple Silicon 本机已运行；Intel、Windows x64、Linux x64 已提供构建矩阵，但尚未在对应系统执行，安装、文件权限、系统链接和真实 Agent 联调均标记待验证。正式签名、公证和公开发布不在本轮范围。

图标已检查 16、32、128、512 像素导出；16 像素主要依靠蓝橙开关轮廓识别，电源细节在较大尺寸清晰。母版、PNG、ICO、ICNS 均保留在源码中。
