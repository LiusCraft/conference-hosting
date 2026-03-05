const modules = [
  {
    name: "host-core",
    path: "crates/host-core",
    desc: "纯 Rust 核心业务逻辑库，不依赖任何平台或 UI 框架",
    responsibilities: [
      "WebSocket 协议抽象（hello 握手、listen 控制、帧收发）",
      "音频帧切分与队列管理",
      "Opus 编解码封装",
      "VAD 语音活动检测集成",
      "会话状态机管理",
    ],
    color: "border-primary/30 bg-primary/5",
    tagColor: "bg-primary/15 text-primary",
  },
  {
    name: "host-platform",
    path: "crates/host-platform",
    desc: "平台适配层，隔离不同操作系统的音频设备和虚拟麦克风差异",
    responsibilities: [
      "cpal 音频设备枚举与配置",
      "平台特定的 Loopback 回采实现",
      "虚拟麦克风设备发现与输出",
      "音频镜像（监听）管线",
      "条件编译 #[cfg(target_os)] 平台分支",
    ],
    color: "border-chart-2/30 bg-chart-2/5",
    tagColor: "bg-chart-2/15 text-chart-2",
  },
  {
    name: "host-app-gpui",
    path: "apps/host-app-gpui",
    desc: "GPUI 桌面应用层，负责 UI 渲染与用户交互",
    responsibilities: [
      "GPUI 窗口与组件树",
      "聊天式消息展示界面",
      "输入/输出设备选择 UI",
      "BlackHole 快捷切换",
      "音频状态实时指示",
    ],
    color: "border-chart-4/30 bg-chart-4/5",
    tagColor: "bg-chart-4/15 text-chart-4",
  },
]

export function ModularitySection() {
  return (
    <section id="modularity" className="px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            模块化设计
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            Cargo Workspace 三层架构
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            核心逻辑、平台适配、UI 应用三层清晰分离，每层可独立编译测试，
            便于未来扩展新平台或替换 UI 框架。
          </p>
        </div>

        {/* Workspace visualization */}
        <div className="mb-12 rounded-xl border border-border bg-card/30 p-6">
          <div className="mb-4 flex items-center gap-2">
            <div className="h-3 w-3 rounded-full bg-destructive/60" />
            <div className="h-3 w-3 rounded-full bg-chart-2/60" />
            <div className="h-3 w-3 rounded-full bg-primary/60" />
            <span className="ml-3 font-mono text-xs text-muted-foreground">
              Cargo.toml — workspace
            </span>
          </div>
          <pre className="overflow-x-auto font-mono text-xs leading-6 text-muted-foreground">
            <code>{`[workspace]
members = [
    "crates/host-core",       # 核心业务逻辑
    "crates/host-platform",   # 平台适配层
    "apps/host-app-gpui",     # GPUI 桌面应用
]

[workspace.dependencies]
tokio       = { version = "1", features = ["full"] }
tungstenite = "0.24"
cpal        = "0.15"
opus        = "0.3"
rubato      = "0.14"
webrtc-vad  = "0.4"
tracing     = "0.1"`}</code>
          </pre>
        </div>

        {/* Module cards */}
        <div className="grid gap-6 md:grid-cols-3">
          {modules.map((mod) => (
            <div
              key={mod.name}
              className={`rounded-xl border p-6 ${mod.color}`}
            >
              <div className="mb-3 flex items-center gap-2">
                <span
                  className={`rounded-md px-2 py-0.5 font-mono text-xs font-medium ${mod.tagColor}`}
                >
                  {mod.name}
                </span>
              </div>
              <p className="mb-1 font-mono text-[10px] text-muted-foreground">
                {mod.path}
              </p>
              <p className="mb-4 text-sm leading-relaxed text-foreground/80">
                {mod.desc}
              </p>
              <ul className="flex flex-col gap-2">
                {mod.responsibilities.map((r) => (
                  <li
                    key={r}
                    className="flex items-start gap-2 text-xs text-muted-foreground"
                  >
                    <span className="mt-1.5 h-1 w-1 shrink-0 rounded-full bg-foreground/20" />
                    {r}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        {/* Dependency flow */}
        <div className="mt-10 flex flex-col items-center gap-2 text-xs text-muted-foreground">
          <p className="font-medium text-foreground">依赖方向</p>
          <div className="flex items-center gap-3">
            <span className="rounded-md bg-chart-4/15 px-2 py-1 font-mono text-chart-4">
              host-app-gpui
            </span>
            <span>{"-->"}</span>
            <span className="rounded-md bg-chart-2/15 px-2 py-1 font-mono text-chart-2">
              host-platform
            </span>
            <span>{"-->"}</span>
            <span className="rounded-md bg-primary/15 px-2 py-1 font-mono text-primary">
              host-core
            </span>
          </div>
          <p className="mt-1 text-muted-foreground/60">
            上层依赖下层，下层不感知上层实现
          </p>
        </div>
      </div>
    </section>
  )
}
