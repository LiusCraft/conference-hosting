import { Gauge, Layers, Paintbrush, Bolt, MemoryStick, Cpu } from "lucide-react"

const strategies = [
  {
    icon: Paintbrush,
    title: "GPUI 原生 GPU 渲染",
    points: [
      "所有 UI 元素通过 Metal (macOS) / DirectX / Vulkan 直接绘制到 GPU",
      "不依赖 WebView 或 DOM，渲染路径极短，帧率稳定在 60fps+",
      "文本布局、阴影、动画全部在 GPU Shader 中完成",
    ],
  },
  {
    icon: Bolt,
    title: "Tokio 异步运行时",
    points: [
      "WebSocket 收发、音频处理、UI 更新运行在独立异步任务中",
      "零阻塞 I/O 模型，音频帧不会因网络抖动而丢帧",
      "利用 Rust async/await 自然表达并发，编译器保障无数据竞争",
    ],
  },
  {
    icon: MemoryStick,
    title: "零拷贝音频管线",
    points: [
      "音频缓冲区在采集、编码、发送之间通过引用传递，避免冗余复制",
      "Ring Buffer 环形缓冲设计，最小化内存分配和 GC 压力（Rust 无 GC）",
      "20ms 分帧按固定大小预分配，运行时零 allocation",
    ],
  },
  {
    icon: Gauge,
    title: "低延迟优先调度",
    points: [
      "音频回调线程设置为实时优先级，确保采集/播放不被其他任务抢占",
      "WebSocket 心跳与音频帧发送解耦，互不阻塞",
      "TTS 下行流式解码，边接收边播放，不等待完整音频",
    ],
  },
  {
    icon: Layers,
    title: "分层缓冲策略",
    points: [
      "上行：20ms 帧立即编码推送，无额外缓冲",
      "下行：自适应 jitter buffer 吸收网络抖动",
      "播放：双缓冲交替写入/播放，消除音频断裂",
    ],
  },
  {
    icon: Cpu,
    title: "编译期优化",
    points: [
      "Rust 编译器 LLVM 后端深度优化，Release 构建自动内联关键路径",
      "泛型单态化确保音频处理函数无虚函数开销",
      "cargo-bundle 生成平台原生安装包，无运行时解释层",
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
            GPU 渲染 + Rust 并发
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            充分利用 Rust 的零成本抽象与 GPUI 的 GPU 直接渲染能力，
            在保证内存安全的前提下达到接近系统级的性能表现。
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
