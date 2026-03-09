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

# 十三、技术选型（定稿）

## 选型结论

采用 **Rust + GPUI** 作为固定实现方案。

- 核心引擎：Rust（低延迟音频处理、稳定的并发模型、内存安全）
- 桌面端：GPUI（Rust 原生桌面 UI，渲染开销低，不依赖 WebView）
- 工程组织：Cargo Workspace（`host-core` / `host-platform` / `host-app-gpui`）
- 通信层：Tokio + tokio-tungstenite + rustls
- 音频层：cpal（跨平台抽象）+ 平台适配（WASAPI/CoreAudio/PulseAudio）
- 音频处理：rubato（重采样）、webrtc-vad（打断检测）
- 可观测性：tracing + tracing-subscriber
- 打包发布：cargo-bundle（后续补齐各平台签名/安装器流程）

## 备选方案对比（简版）

- Tauri 2 + Web 前端：开发门槛低、生态成熟，但引入 WebView/前端栈后运行时链路更复杂
- Electron + Node.js：开发快，但实时音频链路依赖原生插件，延迟和稳定性风险更高
- Go + Wails：部署方便，但桌面音频生态和跨平台设备控制能力不如 Rust 方案成熟
- Rust + GPUI：UI 组件需要更多自建，但在性能、稳定性、跨平台可控性方面最匹配本产品

## 平台实现策略

- Windows：WASAPI Loopback 采集 + VB-Cable 输出
- macOS：CoreAudio 采集/播放 + BlackHole 虚拟设备
- Linux：PulseAudio/PipeWire Monitor 采集 + 虚拟设备输出

## MVP实施边界

- 先实现主链路：采集 -> 20ms 分帧 -> WebSocket 双向流 -> TTS 播放到虚拟麦克风
- 首批优先支持 macOS + Windows，Linux 在第二阶段补齐
- AEC 先落地基础实时方案（AEC3，16kHz/mono，10ms 分块），后续再补齐更细粒度延迟估计与自动调参

## 协议实现备注

- 当前实现统一按 **20ms / 16kHz / mono / PCM16 = 320 samples / 640 bytes** 作为采集分帧口径
- 传输层统一使用 WebSocket Binary + Opus 编解码，并在联调时以服务端协议版本为准

------------------------------------------------------------------------

# 十四、工程脚手架落地状态（2026-03-05）

- 已初始化 Cargo Workspace：`crates/host-core`、`crates/host-platform`、`apps/host-app-gpui`
- 已提供 GPUI 最小可运行壳：`cargo run -p host-app-gpui`
- 已在 `host-platform` 落地 WS 主链路基础实现：hello 握手、`listen` 控制、双向音频帧收发
- 已在 `host-app-gpui` 落地消息可视化：WS 文本消息以聊天形式展示（含工具调用类文本事件）
- 已在 `host-app-gpui` 支持文本输入（Enter 发送 `listen detect`）与 `cpal` 麦克风采集连续上行（20ms 分帧 Opus）
- 已在 `host-app-gpui` 支持下行 Opus 解码并本地扬声器播放
- 已在 `host-app-gpui` 支持输入/输出设备列表选择与 `BlackHole` 输出快捷切换（用于虚拟麦克风链路）
- 已在 `host-app-gpui` 支持将输入源切换为输出回采（loopback）设备，用于采集会议软件下行声音
- 已在 `host-app-gpui` 支持将所选输入源与输出源音频镜像到系统默认扬声器，便于本机监听
- `host-app-gpui` 对下行音频二进制仅做统计，不转文本展示
- 当前虚拟麦克风输出仍为下一阶段
- 已在 `host-app-gpui` 接入 AEC3 实时回声消除：下行 Opus 解码样本作为 render 参考，麦克风上行在 10ms 帧上做 capture 消除，并基于采集/播放回调延迟动态更新 stream delay（可用 `HOST_ENABLE_AEC=0` 关闭）
- 设置面板新增 AEC 开关与实时指标展示（应用流延迟、采集/播放回调延迟、播放缓冲延迟、ERL/ERLE）
