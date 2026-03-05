# conference-hosting

Rust + GPUI 的 AI 会议托管桌面端工程骨架。

## 目录结构

- `crates/host-core`：核心领域状态与模型
- `crates/host-platform`：平台能力适配层（占位）
- `apps/host-app-gpui`：GPUI 桌面应用最小可运行壳

## 快速开始

```bash
cargo fetch
cargo run -p host-app-gpui
```

## 常用命令

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
