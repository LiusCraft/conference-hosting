import {
  Mic,
  Radio,
  Volume2,
  Headphones,
  MessageSquare,
  ShieldCheck,
  Activity,
  Ear,
} from "lucide-react"

const features = [
  {
    icon: Mic,
    title: "实时音频采集",
    desc: "通过 cpal 跨平台抽象层采集系统音频，支持 Loopback 回采与麦克风输入双路并行。",
    tags: ["cpal", "CoreAudio", "WASAPI"],
  },
  {
    icon: Radio,
    title: "WebSocket 双向流",
    desc: "基于 tokio-tungstenite + rustls 建立加密双向音频通道，50 帧/秒连续推送。",
    tags: ["Tokio", "TLS", "异步"],
  },
  {
    icon: Volume2,
    title: "虚拟麦克风输出",
    desc: "将 AI 生成语音路由到 BlackHole / VB-Cable，会议参与者无感接收 AI 发言。",
    tags: ["BlackHole", "VB-Cable"],
  },
  {
    icon: MessageSquare,
    title: "消息可视化",
    desc: "WS 文本事件（STT 结果、工具调用）以聊天界面实时展示，便于调试与监控。",
    tags: ["GPUI", "实时渲染"],
  },
  {
    icon: Ear,
    title: "VAD 智能打断",
    desc: "集成 webrtc-vad 检测人声活动，当参会者发言时自动中断 AI 播放，实现自然对话。",
    tags: ["webrtc-vad", "中断机制"],
  },
  {
    icon: Activity,
    title: "音频处理流水线",
    desc: "20ms PCM 分帧、Opus 编解码、rubato 重采样，构建完整的实时音频处理管线。",
    tags: ["Opus", "rubato", "PCM"],
  },
  {
    icon: Headphones,
    title: "本机监听镜像",
    desc: "输入/输出音频可镜像到系统默认扬声器，方便本机实时监听会议与 AI 的交互。",
    tags: ["音频镜像", "调试"],
  },
  {
    icon: ShieldCheck,
    title: "Rust 安全保障",
    desc: "所有权系统杜绝数据竞争，零成本抽象保障性能，unsafe 仅用于必要的 FFI 边界。",
    tags: ["内存安全", "并发安全"],
  },
]

export function FeaturesSection() {
  return (
    <section id="features" className="px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            核心功能
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            完整的音频网关能力矩阵
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            从音频采集到 AI 语音输出，每个模块都针对实时性与稳定性做了深度优化。
          </p>
        </div>

        <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-4">
          {features.map((feature) => (
            <div
              key={feature.title}
              className="group rounded-xl border border-border bg-card/50 p-6 transition-all hover:border-primary/30 hover:bg-card"
            >
              <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
                <feature.icon size={20} className="text-primary" />
              </div>
              <h3 className="mb-2 text-sm font-semibold text-foreground">
                {feature.title}
              </h3>
              <p className="mb-4 text-xs leading-relaxed text-muted-foreground">
                {feature.desc}
              </p>
              <div className="flex flex-wrap gap-1.5">
                {feature.tags.map((tag) => (
                  <span
                    key={tag}
                    className="rounded-md bg-secondary px-2 py-0.5 font-mono text-[10px] text-muted-foreground"
                  >
                    {tag}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
