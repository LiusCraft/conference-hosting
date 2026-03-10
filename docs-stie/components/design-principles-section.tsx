const principles = [
  {
    title: "GPU 优先渲染",
    desc: "GPUI 将所有 UI 元素直接提交至 GPU，绕过传统 CPU 光栅化路径。文本、阴影、动画均在 Shader 中计算，确保 60fps 流畅交互，即使在高密度信息展示场景下也无卡顿。",
  },
  {
    title: "最小化视觉噪声",
    desc: "音频网关工具需长时间后台运行，UI 采用低对比度深色主题，减少视觉干扰。关键状态指标（连接状态、音频电平、延迟数值）以高亮色突出，非核心信息保持克制。",
  },
  {
    title: "实时反馈优先",
    desc: "音频电平、WebSocket 状态、RTT、AEC 指标与 STT/TTS 消息需持续可见。界面通过事件节流保证高频更新下仍可读、可控。",
  },
  {
    title: "单窗口紧凑布局",
    desc: "桌面工具应尽量减少窗口数量。主界面包含设备选择、消息流、状态面板三大区域，通过 GPUI Flex 布局自适应窗口尺寸，支持拖拽调整区域比例。",
  },
  {
    title: "键盘与低干扰交互",
    desc: "支持 Enter 发送指令、连接与采集开关快速操作、设置面板集中编辑。避免多层弹窗与过度动画，保证调试过程稳定流畅。",
  },
  {
    title: "配置与状态同源",
    desc: "WS 参数、AEC 开关、listen mode、MCP servers 在同一设置面板编辑并落盘，确保 UI 展示、运行时行为与持久化配置一致。",
  },
]

export function DesignPrinciplesSection() {
  return (
    <section className="px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            UI 设计原则
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            为实时音频场景而设计
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            界面设计以信息密度与实时性为核心诉求，GPUI 原生渲染确保视觉一致性和极致响应速度。
          </p>
        </div>

        <div className="grid gap-px overflow-hidden rounded-xl border border-border bg-border md:grid-cols-2 lg:grid-cols-3">
          {principles.map((p) => (
            <div key={p.title} className="bg-card/60 p-6 backdrop-blur-sm">
              <h3 className="mb-3 text-sm font-semibold text-foreground">
                {p.title}
              </h3>
              <p className="text-xs leading-relaxed text-muted-foreground">
                {p.desc}
              </p>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
