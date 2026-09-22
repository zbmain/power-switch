# New API 接入

power-switch 内置 New API 接入模块。首版适配 `v1.0.0-rc.21` 的 Cookie 会话认证与自定义 OAuth，默认地址为 `https://new-api.banmahui.cn`。无需修改 New API 或 Keycloak 服务端。

## 使用

1. 在模型库点击 **从 New API 添加**。实例地址应为 HTTPS 根地址，不包含 `/v1`，且与 New API 的 `ServerAddress` 一致。
2. 点击 **钉钉 / Keycloak 登录**，在独立窗口完成授权。应用不会读取或保存钉钉密码。
3. 选择目标客户端、分组和模型。列表只显示用户可用且服务端声明支持对应协议的模型，默认优先选中 `auto`；没有兼容的 `auto` 时选择第一个可用模型。平台名称默认为 `winwin`，可自行修改，切换模型时保留。
4. 选择 Codex 时，填写上游实际的上下文窗口。图像、工具调用和推理档位需按上游能力设置，不能从模型别名推断。
5. 点击 **创建并添加**。应用自动创建或复用专属密钥、获取完整值，校验密钥访问并保存模型。密钥默认长期有效、沿用账户额度、仅允许所选模型。
6. 可主动显示或复制密钥。**测试连接** 会发送一次最多请求 64 个输出 Token 的模型调用，可能消耗账户额度；保存和模型列表校验不发送推理请求。测试仅确认所选协议收到有效响应，不证明工具调用、图像或上下文能力。
7. 返回模型库，模型名称展示为“平台名称 · 模型 ID”（例如 `winwin · auto`），接口调用仍使用原始模型 ID。使用原有 **应用到 Agent → 预览 → 确认覆盖并备份** 流程。

浏览器演示使用内存中的虚构账号和密钥，不请求真实实例，不修改本地 Agent 配置。ego-browser 是调研和网页测试工具，不是插件运行依赖。

## 协议和接口

| New API 元数据    | power-switch 协议    | 客户端      | API 基础地址      |
| ----------------- | -------------------- | ----------- | ----------------- |
| `openai`          | `openai-chat`        | WorkBuddy   | `https://实例/v1` |
| `openai-response` | `openai-responses`   | Codex       | `https://实例/v1` |
| `anthropic`       | `anthropic-messages` | Claude Code | `https://实例`    |

登录时，Rust 使用新的 Cookie 容器请求 `/api/oauth/state`，在隔离 WebView 中打开提供方授权地址，保留 New API 原始回调 `/oauth/{provider}`。原生导航处理器校验回调来源、路径及唯一 `state`，截获授权码并阻止网页再次兑换，由同一 Rust HTTP 会话请求 `/api/oauth/{provider}`。Client Secret 和 Keycloak Token 始终由 New API 服务端处理。

管理请求使用 New API 会话和 `New-Api-User`，不把模型调用密钥当作管理令牌，也不调用会覆盖现有系统访问令牌的 `/api/user/token`。

使用的管理接口：`/api/status`、`/api/user/self`、`/api/user/self/groups`、`/api/user/models?group=…`、`/api/pricing`、`POST /api/token/`、`GET /api/token/search`、`POST /api/token/{id}/key`。完整 API Key 通过专用接口取得，不使用令牌列表中的脱敏值。`GET /v1/models` 验证模型密钥访问。

`v1.0.0-rc.21` 的完整密钥接口返回原始的 48 位字母数字串，应用校验完整性后补齐一次 `sk-` 前缀。参考固定版本的 [令牌接口](https://github.com/QuantumNous/new-api/blob/v1.0.0-rc.21/controller/token.go)、[密钥生成](https://github.com/QuantumNous/new-api/blob/v1.0.0-rc.21/common/utils.go) 和 [OAuth 实现](https://github.com/QuantumNous/new-api/blob/v1.0.0-rc.21/controller/oauth.go)。

## 存储与恢复

- macOS 登录会话只保存在系统钥匙串的 `com.powerswitch.desktop.new-api` 服务内，按实例地址隔离；Windows 使用系统凭证存储。其他平台暂不支持持久登录，绝不降级为明文会话文件。
- 当前版本会话最长保留 30 天，启动恢复时核对身份，后续请求也由服务器鉴权；会话到期后重新登录。已创建的模型密钥独立有效。
- API Key 沿用 `models.json` 的本机私有存储。管理会话、授权码和完整远端响应不写日志、分享链接或错误提示。
- `new-api.json` 只保存账号 ID、实例、分组、模型、唯一令牌名称、令牌 ID 和稳定的本地模型 ID。Unix 文件权限为 `0600`。**请保留此文件**，它负责识别已有密钥并恢复未完成操作。
- 创建请求发出前写入 `submitted` 记录。崩溃、断网或响应丢失后先查询精确名称；不自动重发无法确认结果的创建请求。可先点击 **重新检查并继续**；只有用户核对服务端列表并勾选允许重新创建时，才启动新的创建操作。
- 重复导入复用密钥和本地模型 ID；同一账号、实例、分组、模型的多种协议共用一把密钥。用户主动更换失效密钥时，同步更新本机仍指向原接口的关联模型；已写入 Agent 的配置需重新预览应用。
- **断开连接** 仅移除本机管理会话，不删除服务端密钥。删除模型库记录也不撤销服务端密钥，需要撤销时在 New API 控制台操作。

## 测试和版本范围

自动化测试使用本机模拟 HTTP 服务、内存凭证库和临时目录，不访问生产账号。覆盖 OAuth state／回调／重放／取消／超时、账号隔离、密钥响应丢失和恢复、重复导入、协议筛选、失效密钥、私有存储及按需推理测试。

```sh
pnpm test
pnpm typecheck
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm tauri build --bundles app
```

OAuth 的原生窗口需要在 macOS 上完成一次真实钉钉授权验收：检查扫码显示、授权返回、模型读取、创建与复用、显示／复制密钥、一次实际模型测试及应用预览。若身份提供方禁止嵌入式窗口，应保留错误现场并另行适配系统浏览器回调；不会自动转为读取其他浏览器的 Cookie。

其他版本的 New API 可能采用 JWT/Refresh Token 认证。首版检测到不同版本会提示不兼容，不自动升级服务端、修改 Keycloak、执行 SQL 或套用不兼容接口。

## 本轮交付验证（2026-09-22）

- Rust 测试 35 项、Vitest 14 项通过；TypeScript、Clippy 和格式检查通过。
- macOS `.app` 构建通过，输出位于 `src-tauri/target/release/bundle/macos/power-switch.app`。
- ego-browser 完成浏览器演示的登录、协议筛选、Codex 上下文填写、模型导入和按需测试流程。浏览器截图接口超时，界面检查使用页面快照和 DOM 布局信息。
- 尚未完成原生窗口真实钉钉扫码、生产密钥创建与实际付费调用；自动化结果不能替代这部分联调验收。
