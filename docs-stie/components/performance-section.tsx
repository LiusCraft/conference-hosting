import { Gauge, Layers, Paintbrush, Bolt, MemoryStick, Cpu } from "lucide-react"

const strategies = [
  {
    icon: Paintbrush,
    title: "GPUI 原生 GPU 渲染",
    points: [
      "桌面端 UI 使用 GPUI 渲染，不依赖 WebView",
      "音频高频事件在 UI 侧做节流刷新，降低重绘压力",
      "连接状态、RTT、AEC 指标与消息面板可并行更新",
    ],
  },
  {
    icon: Bolt,
    title: "Tokio 异步运行时",
    points: [
      "网关运行在独立 worker 线程中，内部持有 Tokio runtime",
      "命令通道与事件通道解耦 UI 与网络/音频任务",
      "WebSocket 文本、二进制、pong 事件分别处理，互不阻塞",
    ],
  },
  {
    icon: MemoryStick,
    title: "固定帧音频管线",
    points: [
      "上行统一 20ms/16kHz/mono 口径，便于链路稳定与调试",
      "Opus 编码使用固定码率与复杂度，降低抖动",
      "下行解码后按输出采样率重建播放缓冲并限长保护",
    ],
  },
  {
    icon: Gauge,
    title: "AEC3 动态延迟校准",
    points: [
      "采集与播放回调延迟、播放缓冲延迟持续采样",
      "按平滑策略更新 stream delay，减少回声残留与抖动",
      "支持运行时开关与共享路由强制开启策略",
    ],
  },
  {
    icon: Layers,
    title: "可降级的容错策略",
    points: [
      "上行帧队列满时触发丢包告警，避免阻塞主循环",
      "hello 握手、MCP 连接、MCP 调用均有超时边界",
      "单个 MCP server 异常时返回可用子集，不中断主链路",
    ],
  },
  {
    icon: Cpu,
    title: "运行态可观测",
    points: [
      "状态栏展示会话时长、上下行速率、RTT 与响应延迟",
      "设置面板展示 AEC stream delay/ERL/ERLE 等关键指标",
      "敏感字段在展示前统一脱敏，提升调试与安全平衡",
    ],
  },
]

export function PerformanceSection() {
  return (
    <section id="performance" className="px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            性能优化
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            实时性与稳定性并重
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            当前实现重点放在链路稳定、延迟可观测和故障隔离，
            所有结论均以已落地代码路径为依据。
          </p>
        </div>

        <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
          {strategies.map((item) => (
            <div
              key={item.title}
              className="group rounded-xl border border-border bg-card/30 p-6 transition-all hover:border-primary/20 hover:bg-card/60"
            >
              <div className="mb-4 flex items-center gap-3">
                <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/10">
                  <item.icon size={18} className="text-primary" />
                </div>
                <h3 className="text-sm font-semibold text-foreground">
                  {item.title}
                </h3>
              </div>
              <ul className="flex flex-col gap-2.5">
                {item.points.map((point, idx) => (
                  <li
                    key={idx}
                    className="flex gap-2 text-xs leading-relaxed text-muted-foreground"
                  >
                    <span className="mt-1 h-1 w-1 shrink-0 rounded-full bg-primary/60" />
                    {point}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
