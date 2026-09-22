# 模型、配置路径、备份和技能管理增强验收

日期：2026-09-22，环境：macOS Apple Silicon。

## 已实现

1. 新模型与省略能力字段的导入默认开启图像输入，显式关闭的已有模型不变；New API 新导入表单同样默认开启。
2. Agent 配置位置支持原生文件/目录选择，也保留手动输入。取消选择不改变草稿；保存设置后才生效。
3. 备份记录增加删除按钮和二次确认，只删除选中备份；旧恢复预览失效，实际配置文件保持不变。
4. 技能页提供四个作用域、搜索、列表和右侧文档详情，每分钟自动刷新、手动刷新重新计时。支持全局本地/Git 安装、到 Agent 的真实软链接，以及确认后移入回收目录。

## 自动化检查

执行 just check，全部通过：

- TypeScript、Prettier、rustfmt、Clippy（警告视为错误）。
- Vitest 28 项：包括界面默认值、路径选择/取消/保存、备份二次确认、技能安装确认、链接确认、删除确认、自动刷新与慢扫描互斥。
- Rust 48 项：包含 9 项技能文件操作集成测试、1 项 Git URL 解析测试，以及新增的备份删除、图像能力默认值测试。

技能集成测试使用隔离用户目录与暂存目录，覆盖本地批量安装、只安装选中项、保留预览快照、拒绝同名覆盖、真实 Unix 软链接、删除链接保留源文件、全局删除清理引用、坏链接和循环链接、目录覆盖、并发变化、路径重定向和后续目标失败时不留下部分链接。

Windows 与 Linux 的原生选择器、权限和软链接行为需在对应系统继续验证。本轮未修改真实 WorkBuddy、Claude Code 或 Codex 配置，也未删除用户已安装技能。

## 原生窗口与构建

原生 release 应用已启动，左侧显示“技能”入口。实际扫描全局 104 个、Claude Code 4 个、Codex 7 个、WorkBuddy 4 个技能；页面正确展示目录、元数据、原始文档和软链接标记。截图检查了列表和右侧详情的布局。

检查期间检测到用户正在操作应用窗口，已停止界面操作。原生文件选择器的实际点击验收待补；路径选择、取消与显式保存的前端逻辑已通过模拟选择器测试。

额外执行 skills_acceptance 示例：从 Anthropic 官方公开 skills 仓库下载 skill-creator，在临时用户目录中确认全局安装，为临时 WorkBuddy 建立真实软链接，确认全局删除后两处清单均为空。全过程通过，临时目录自动清理；不读取或执行下载技能中的指令。

可通过下列命令重复网络验收（需 Git 和外网）：

    cargo run --manifest-path src-tauri/Cargo.toml --no-default-features --example skills_acceptance -- https://github.com/anthropics/skills skills/skill-creator

Vite 生产构建、Tauri release 构建、macOS APP 和 DMG 均成功。更新后的测试安装包位于 src-tauri/target/release/bundle/dmg/power-switch_0.1.0_aarch64.dmg，应用位于 src-tauri/target/release/bundle/macos/power-switch.app。本轮仍为未正式签名、公证的测试包。
