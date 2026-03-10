const modules = [
  {
    name: "host-core",
    path: "crates/host-core",
    desc: "核心协议与领域模型层，不依赖具体平台和 UI",
    responsibilities: [
      "`hello` / `listen` / `mcp` 文本协议模型",
      "JSON-RPC 请求/响应结构定义",
      "16kHz/20ms/mono 音频帧常量",
      "Gateway 状态与基础枚举类型",
      "协议序列化测试",
    ],
    color: "border-primary/30 bg-primary/5",
    tagColor: "bg-primary/15 text-primary",
  },
  {
    name: "host-platform",
    path: "crates/host-platform",
    desc: "WebSocket 传输适配层，承载连接与事件流",
    responsibilities: [
      "URL/header 组装（Authorization/device-id/client-id）",
      "hello 握手等待与超时控制",
      "文本/二进制/pong 事件分发",
      "listen/mcp/jsonrpc 发送封装",
      "WS 主链路集成测试",
    ],
    color: "border-chart-2/30 bg-chart-2/5",
    tagColor: "bg-chart-2/15 text-chart-2",
  },
  {
    name: "host-app-gpui",
    path: "apps/host-app-gpui",
    desc: "桌面应用层，串联 UI、音频运行时与 MCP 网桥",
    responsibilities: [
      "GPUI 窗口、侧栏、聊天面板、设置面板",
      "cpal 采集/播放 + Opus 编解码 + AEC3 指标",
      "MCP server 管理页与本地探测",
      "MCP JSON-RPC 路由（initialize/tools/list/tools/call）",
      "本地 settings.json 持久化与脱敏展示",
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
    "crates/host-core",       # 协议与模型
    "crates/host-platform",   # WS 适配层
    "apps/host-app-gpui",     # GPUI 桌面应用
]

[workspace.dependencies]
tokio              = "1.44"
tokio-tungstenite  = "0.26"
rustls             = "0.23"
cpal               = "0.15"
opus               = "0.3"
aec3               = "0.1"
gpui               = "0.2"
gpui-component     = "0.5"
rmcp               = "1.1"
serde/serde_json   = "1.0"`}</code>
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
          <div className="flex flex-col items-center gap-2">
            <span className="rounded-md bg-chart-4/15 px-2 py-1 font-mono text-chart-4">
              host-app-gpui
            </span>
            <div className="flex items-center gap-2">
              <span>{"-->"}</span>
              <span className="rounded-md bg-chart-2/15 px-2 py-1 font-mono text-chart-2">
                host-platform
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span>{"-->"}</span>
              <span className="rounded-md bg-primary/15 px-2 py-1 font-mono text-primary">
                host-core
              </span>
            </div>
          </div>
          <p className="mt-1 text-muted-foreground/60">
            host-app-gpui 同时依赖 host-platform 与 host-core
          </p>
        </div>
      </div>
    </section>
  )
}
