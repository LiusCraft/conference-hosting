const phases = [
  {
    phase: "MVP",
    label: "最小可行产品",
    status: "进行中",
    statusColor: "bg-primary text-primary-foreground",
    items: [
      { label: "Cargo Workspace + GPUI 桌面壳", done: true },
      { label: "WebSocket 双向通信（hello / listen / 音频帧）", done: true },
      { label: "GPUI 聊天消息可视化界面", done: true },
      { label: "cpal 麦克风采集 + Opus 编码上行", done: true },
      { label: "下行 Opus 解码 + 扬声器播放", done: true },
      { label: "输入/输出设备列表选择", done: true },
      { label: "MCP 管理页 + tools 桥接闭环", done: true },
      { label: "Loopback 回采与音频镜像", done: true },
      { label: "AEC3 实时回声消除 + 指标面板", done: true },
      { label: "跨平台实机联调（Windows/Linux）", done: false },
    ],
  },
  {
    phase: "V1.0",
    label: "稳定发布版",
    status: "计划中",
    statusColor: "bg-chart-2/20 text-chart-2",
    items: [
      { label: "虚拟麦克风一键自动编排", done: false },
      { label: "网关自动重连与健康检查", done: false },
      { label: "VAD 智能打断 + 静音检测", done: false },
      { label: "MCP 调用审计与指标看板", done: false },
      { label: "macOS + Windows 应用签名分发", done: false },
      { label: "系统托盘常驻 + 快捷键控制", done: false },
      { label: "音频质量监控仪表板", done: false },
    ],
  },
  {
    phase: "V2.0",
    label: "能力扩展版",
    status: "规划中",
    statusColor: "bg-secondary text-muted-foreground",
    items: [
      { label: "Linux PulseAudio/PipeWire 支持", done: false },
      { label: "多角色 AI（主持人 / 客服 / 秘书）", done: false },
      { label: "实时会议翻译", done: false },
      { label: "AI 自动参会（日历集成）", done: false },
      { label: "插件系统支持自定义 AI 后端", done: false },
      { label: "团队协作与会议管理后台", done: false },
    ],
  },
]

export function RoadmapSection() {
  return (
    <section id="roadmap" className="px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            产品路线图
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            从 MVP 到全功能平台
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            路线图已按当前代码状态刷新：先完成跨平台联调与发布质量，再扩展会议智能能力。
          </p>
        </div>

        <div className="grid gap-8 md:grid-cols-3">
          {phases.map((phase) => (
            <div
              key={phase.phase}
              className="rounded-xl border border-border bg-card/30 p-6"
            >
              <div className="mb-4 flex items-center justify-between">
                <div>
                  <span className="font-mono text-2xl font-bold text-foreground">
                    {phase.phase}
                  </span>
                  <p className="mt-0.5 text-xs text-muted-foreground">
                    {phase.label}
                  </p>
                </div>
                <span
                  className={`rounded-full px-3 py-1 text-[10px] font-medium ${phase.statusColor}`}
                >
                  {phase.status}
                </span>
              </div>
              <div className="flex flex-col gap-2.5">
                {phase.items.map((item) => (
                  <div key={item.label} className="flex items-start gap-2.5">
                    <div
                      className={`mt-1 flex h-4 w-4 shrink-0 items-center justify-center rounded-sm border ${
                        item.done
                          ? "border-primary bg-primary"
                          : "border-border bg-transparent"
                      }`}
                    >
                      {item.done && (
                        <svg
                          width="10"
                          height="10"
                          viewBox="0 0 10 10"
                          className="text-primary-foreground"
                        >
                          <path
                            d="M2 5l2 2 4-4"
                            stroke="currentColor"
                            strokeWidth="1.5"
                            fill="none"
                            strokeLinecap="round"
                            strokeLinejoin="round"
                          />
                        </svg>
                      )}
                    </div>
                    <span
                      className={`text-xs leading-relaxed ${
                        item.done
                          ? "text-foreground"
                          : "text-muted-foreground"
                      }`}
                    >
                      {item.label}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
