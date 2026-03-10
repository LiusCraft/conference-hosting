# AI会议托管工具产品设计方案

## 一、产品概述

AI会议托管工具是一款用于自动参与在线会议并进行语音交互的音频桥接工具。系统能够实时采集电脑会议音频，通过
WebSocket 协议发送至 AI
语音平台进行处理，并接收返回的语音数据，通过虚拟麦克风播放，使会议中的其他参与者能够听到
AI 自动生成的发言。

该工具的核心作用是充当会议系统与 AI 语音平台之间的音频网关（Audio
Gateway）。

核心能力： - 实时会议音频采集 - 音频流 WebSocket 传输 - AI语音分析处理 -
语音生成播放 - 虚拟麦克风输出

支持会议软件： - Zoom - Microsoft Teams - 腾讯会议 - Google Meet -
其他支持麦克风输入的软件

------------------------------------------------------------------------

# 二、系统整体架构

在线会议软件\
↓\
会议托管客户端\
↓\
WebSocket连接\
↓\
AI语音平台（ASR + LLM + TTS）\
↓\
返回语音\
↓\
虚拟麦克风播放\
↓\
会议成员听到AI发言

------------------------------------------------------------------------

# 三、系统核心流程

会议音频\
↓\
音频采集\
↓\
音频帧切分\
↓\
WebSocket发送\
↓\
AI平台处理\
↓\
语音生成\
↓\
返回音频\
↓\
客户端播放\
↓\
虚拟麦克风输出\
↓\
会议参与者听到AI回答

------------------------------------------------------------------------

# 四、WebSocket 接入设计

WebSocket 地址

wss://xrobo-io.qiniuapi.com/v1/ws/

连接Header

Authorization: Bearer `<token>`{=html}\
Device-Id: `<device_id>`{=html}\
Client-Id: `<uuid>`{=html}\
Protocol-Version: 1

------------------------------------------------------------------------

# 五、会话初始化

客户端连接成功后需要发送 hello 消息。

示例

{ "type": "hello", "device_id": "<device_id>", "device_name": "<device_name>",
"device_mac": "<device_mac>", "token": "<token>", "features": { "notify":
{ "intent_trace": true } } }

推荐音频参数

PCM\
16kHz\
Mono\
16bit\
20ms frame

------------------------------------------------------------------------

# 六、音频流传输

音频格式

采集侧 PCM 16bit\
16000Hz\
Mono

帧大小

20ms

对应大小

320 samples（PCM16 Mono）\
640 bytes（PCM16 Mono）

发送频率

50帧/秒

发送方式

上行：PCM 分帧后编码为 Opus，再通过 WebSocket Binary Frame 发送\
下行：服务端返回 Opus Binary Packet，客户端解码后播放

------------------------------------------------------------------------

# 七、AI平台返回数据

## 文本识别结果

{ "type": "stt", "text": "你好，请问有什么可以帮助的" }

## TTS音频数据

Opus Binary Audio Stream

客户端处理流程

接收 Opus 音频\
↓\
缓存\
↓\
Opus 解码\
↓\
音频播放器\
↓\
虚拟麦克风

------------------------------------------------------------------------

# 八、客户端模块设计

## 1 音频采集模块

Windows\
WASAPI Loopback

Mac\
CoreAudio + BlackHole

Linux\
PulseAudio Monitor

采集数据

PCM 16bit\
16000Hz\
Mono

------------------------------------------------------------------------

## 2 WebSocket通信模块

负责：

建立连接\
发送hello\
推送音频帧\
接收事件\
接收TTS音频

------------------------------------------------------------------------

## 3 音频播放模块

WS Binary Data\
↓\
Audio Buffer\
↓\
Audio Player\
↓\
Virtual Mic Output

------------------------------------------------------------------------

## 4 虚拟麦克风模块

Windows

VB-Cable\
Virtual Audio Cable

Mac

BlackHole\
Loopback

------------------------------------------------------------------------

# 九、关键技术问题

## 回声问题

解决方案

AEC回声消除\
或播放时暂停采集

## 打断机制

VAD（语音活动检测）

检测到人声 → 停止TTS播放

## 延迟控制

目标延迟 \< 1 秒

延迟组成

音频帧 20ms\
网络 50ms\
ASR 200ms\
AI处理 300ms\
TTS 200ms

总延迟约 700ms

------------------------------------------------------------------------

# 十、产品定位

该工具本质上是

Voice Gateway

架构

会议系统\
⇅\
音频桥接工具\
⇅\
AI语音平台

------------------------------------------------------------------------

# 十一、未来扩展

自动会议纪要\
AI自动参会\
实时会议翻译\
多角色AI（主持人 / 客服 / 销售 / 秘书）

------------------------------------------------------------------------

# 十二、总结

核心能力

Meeting Audio\
⇅\
Voice Gateway\
⇅\
AI Platform\
⇅\
Virtual Microphone

实现AI自动参与会议、自动回答、会议辅助和AI会议机器人。

------------------------------------------------------------------------

# 十三、技术选型（当前实现对齐）

## 选型结论

采用 **Rust + GPUI** 作为固定实现方案，且已进入可运行实现阶段。

- 核心引擎：Rust（低延迟音频链路、并发安全、可控内存占用）
- 桌面端：GPUI + gpui-component（原生渲染，不依赖 WebView）
- 工程组织：Cargo Workspace（`host-core` / `host-platform` / `host-app-gpui`）
- 通信层：Tokio + tokio-tungstenite + rustls（含 hello 超时、ping/pong RTT）
- 音频层：cpal（设备枚举/采集/播放）+ opus（上行编码/下行解码）
- 回声消除：aec3（10ms capture/render 帧，动态 stream delay）
- MCP 聚合：rmcp（`stdio` + streamable HTTP）
- 配置存储：serde + 本地 JSON（默认 `~/.conference-hosting/host-app-gpui/settings.json`）

## 备选方案对比（简版）

- Tauri 2 + Web 前端：开发门槛低、生态成熟，但引入 WebView/前端栈后运行时链路更复杂
- Electron + Node.js：开发快，但实时音频链路依赖原生插件，延迟和稳定性风险更高
- Go + Wails：部署方便，但桌面音频生态和跨平台设备控制能力不如 Rust 方案成熟
- Rust + GPUI：UI 组件需要更多自建，但在性能、稳定性、跨平台可控性方面最匹配本产品

## 平台实现策略

- Windows：WASAPI 设备链路 + VB-Cable/Virtual Audio Cable 路由
- macOS：CoreAudio 设备链路 + BlackHole/Loopback 路由
- Linux：PulseAudio/PipeWire 设备链路（功能已按抽象接入，联调与发布在后续阶段完善）

## MVP 实施边界（更新）

- 已实现主链路：采集 -> 20ms 分帧 -> Opus -> WebSocket 双向流 -> Opus 解码播放
- 已实现会话辅助能力：设置面板、连接状态、RTT/AEC 指标、消息可视化
- 已实现 MCP 基础闭环：管理配置 + `initialize/tools/list/tools/call`
- 尚未完成的平台化项：一键虚拟麦克风自动路由、完整多平台签名分发流程

## 协议实现备注

- 当前实现统一按 **20ms / 16kHz / mono / PCM16 = 320 samples / 640 bytes** 作为采集分帧口径
- 传输层统一使用 WebSocket Binary + Opus 编解码，文本控制消息走 JSON
- hello 默认声明 `features.notify.intent_trace=true` 与 `features.mcp=true`

------------------------------------------------------------------------

# 十四、工程落地状态（2026-03-11）

- 已完成 Workspace 与应用壳：`crates/host-core`、`crates/host-platform`、`apps/host-app-gpui`
- `host-core` 已沉淀协议模型：`hello/listen/mcp`、JSON-RPC 封装、音频常量
- `host-platform` 已实现 WS 客户端：header 注入、hello 握手校验、超时控制、事件流拆分
- `host-app-gpui` 已实现连接工作线程：命令/事件双通道、自动 ping/pong RTT 采样
- 已实现上行音频链路：设备采集 -> 单声道混音 -> 16k 对齐 -> Opus 编码 -> WS Binary 发送
- 已实现下行音频链路：WS Binary 接收 -> Opus 解码 -> 目标输出设备播放
- 已支持输入/输出设备选择、输出回采（loopback）作为输入、输入/输出镜像到系统扬声器
- 已接入 AEC3：支持运行时开关、共享路由强制开启、实时指标（stream delay/callback delay/ERL/ERLE）
- 已实现聊天面板事件聚合：STT/LLM/TTS 归并展示、intent_trace 折叠、响应延迟统计
- 已实现 Listen Mode（manual/auto/realtime）并在连接态动态下发
- 已实现 MCP Server 管理：新增/编辑/删除/启停、单个/全量刷新、探测状态与 tools 展示
- 已实现 MCP 网桥：对平台响应 `initialize`、`tools/list`、`tools/call`，并按 `<alias>.<tool>` 路由到上游
- 已实现敏感字段脱敏展示：`token`/`authorization`/`*_token` 日志与面板输出自动掩码
- 已实现本地配置持久化（WS 参数、UI 偏好、MCP 列表）
- 当前待补齐：自动化虚拟麦克风编排、VAD 打断策略、跨平台发布签名与安装器流程
