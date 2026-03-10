import {
  Mic,
  Radio,
  Volume2,
  Headphones,
  MessageSquare,
  ShieldCheck,
  Activity,
  Wrench,
} from "lucide-react"

const features = [
  {
    icon: Mic,
    title: "实时音频采集",
    desc: "基于 cpal 采集输入设备，支持将输出设备作为 loopback 输入源，覆盖会议下行回采场景。",
    tags: ["cpal", "CoreAudio", "WASAPI"],
  },
  {
    icon: Radio,
    title: "握手与网络探测",
    desc: "实现 hello 超时校验、session_id 确认与 ping/pong RTT 采样，连接状态可在 UI 持续观测。",
    tags: ["tokio-tungstenite", "rustls", "RTT"],
  },
  {
    icon: Volume2,
    title: "Opus 双向音频链路",
    desc: "上行 20ms 帧 Opus 编码发送，下行 Opus 包实时解码播放到所选输出设备。",
    tags: ["Opus", "20ms", "Binary Frame"],
  },
  {
    icon: MessageSquare,
    title: "消息可视化",
    desc: "聊天面板实时展示 STT/LLM/TTS/MCP/intent_trace 事件，并统计响应延迟。",
    tags: ["GPUI", "实时渲染"],
  },
  {
    icon: ShieldCheck,
    title: "AEC3 回声消除",
    desc: "支持运行时开关、共享路由强制开启与实时指标输出（stream delay/ERL/ERLE）。",
    tags: ["aec3", "10ms", "动态延迟"],
  },
  {
    icon: Activity,
    title: "听写模式控制",
    desc: "支持 manual/auto/realtime 三种 listen mode，可在连接态动态下发到网关线程。",
    tags: ["listen", "manual", "realtime"],
  },
  {
    icon: Wrench,
    title: "MCP 工具桥接",
    desc: "支持 stdio/sse/stream 三类上游，完成 initialize/tools/list/tools/call 闭环。",
    tags: ["rmcp", "JSON-RPC", "tool routing"],
  },
  {
    icon: Headphones,
    title: "配置持久化与脱敏",
    desc: "WS 参数、UI 偏好、MCP 列表本地保存；token/authorization 等敏感字段展示自动脱敏。",
    tags: ["settings.json", "serde", "redaction"],
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
            与当前 Rust 代码一致的能力清单
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            覆盖连接、音频、AEC、MCP、配置与可观测性，反映当前可运行版本的真实落地状态。
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
