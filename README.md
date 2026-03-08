# conference-hosting

Rust + GPUI 的 AI 会议托管桌面端工程骨架。

## 目录结构

- `crates/host-core`：核心领域状态与模型
- `crates/host-platform`：平台能力适配层（包含 WS hello + 双向音频主链路客户端）
- `apps/host-app-gpui`：GPUI 桌面应用最小可运行壳

## 快速开始

```bash
cargo fetch
cargo run -p host-app-gpui
```

## 应用图标与打包

- 图标源文件：`apps/host-app-gpui/assets/svg/app-taskbar-logo.svg`
- 已生成图标：`apps/host-app-gpui/assets/icons/app-taskbar-logo.icns`、`apps/host-app-gpui/assets/icons/app-taskbar-logo.ico`

重新导出图标（SVG -> PNG/ICO/ICNS）：

```bash
bash apps/host-app-gpui/scripts/build_app_icons.sh
```

生成 macOS `.app` 包（Dock 显示自定义图标）：

```bash
bash apps/host-app-gpui/scripts/package_macos_app.sh
```

输出路径：`apps/host-app-gpui/dist/AI Meeting Host.app`

Windows 下执行 `cargo build -p host-app-gpui --release` 时，会通过 `build.rs`
自动把 `assets/icons/app-taskbar-logo.ico` 嵌入到 exe 资源中，用于任务栏图标。

可选环境变量（用于 GPUI 联调连接参数覆盖）：

- `HOST_WS_URL`（默认值见 `apps/host-app-gpui/src/main.rs` 的 `DEFAULT_WS_URL`）
- `HOST_DEVICE_ID` / `HOST_DEVICE_MAC`
- `HOST_DEVICE_NAME`
- `HOST_CLIENT_ID`
- `HOST_TOKEN`
- `HOST_INPUT_DEVICE`（可选，输入设备名或关键字）
- `HOST_OUTPUT_DEVICE`（可选，输出设备名或关键字）

## 常用命令

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## WS 主链路（当前进展）

- `host-core` 已抽象 `hello` / `listen` 协议消息结构
- `host-platform` 提供 `WsGatewayClient`，支持：
  - 建连后自动发送 `hello` 并等待 `session_id`
  - 发送 `listen start/stop/detect` 控制消息
  - 上行发送音频二进制帧
  - 下行接收文本事件与音频二进制帧
- `host-app-gpui` 已接入上述链路，并以聊天流形式展示 WS 文本消息（含 tool/mcp/intent_trace 等文本事件）
- `host-app-gpui` 支持键盘输入文本（Enter 发送）并下发为 `listen detect`
- `host-app-gpui` 已接入 `cpal` 麦克风采集，并按 20ms 分帧编码为 Opus 连续上行（16k mono）
- `host-app-gpui` 已接入下行 Opus 解码与本地扬声器播放
- `host-app-gpui` 支持输入/输出设备列表面板选择，并提供一键切换 `BlackHole` 输出
- `host-app-gpui` 的输入源列表支持选择“输出回采（loopback）”设备，便于采集会议软件下行声音
- `host-app-gpui` 会将选中输入源与选中输出源的音频镜像到当前系统扬声器，便于本机监听
- 音频二进制帧仅计数，不在聊天面板输出文本
