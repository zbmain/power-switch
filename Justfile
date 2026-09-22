set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

default:
    @just --list

# 启动 Tauri 桌面开发环境。
dev:
    pnpm dev

# 启动标明“不写入文件”的浏览器演示。
web:
    pnpm dev:web

# 统一执行静态检查、格式检查与测试。
check:
    pnpm typecheck
    pnpm format:check
    cargo fmt --manifest-path src-tauri/Cargo.toml --check
    cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    just test

# 所有自动化测试均使用隔离目录。
test:
    pnpm test
    pnpm test:release
    cargo test --locked --manifest-path src-tauri/Cargo.toml

# 格式化项目代码。
format:
    pnpm format
    cargo fmt --manifest-path src-tauri/Cargo.toml

# 使用当前平台构建测试安装包。
build:
    pnpm build

# 从生成的图标母版导出平台图标。
icons:
    pnpm tauri icon assets/power-switch-master.png --output src-tauri/icons

# 仅测试 Rust 核心，不启动桌面窗口。
core-test:
    cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features

# 校验即将发布的标签与四份项目版本信息。
release-check tag:
    pnpm release:check -- {{quote(tag)}}

# 使用已安装的 Codex 解析隔离配置，CODEX_BIN 可覆盖可执行文件路径。
codex-check:
    node scripts/verify-codex.mjs
