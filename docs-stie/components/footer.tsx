import Link from "next/link"

const techStack = [
  { label: "语言", value: "Rust" },
  { label: "UI 框架", value: "GPUI" },
  { label: "组件库", value: "gpui-component" },
  { label: "异步运行时", value: "Tokio" },
  { label: "音频抽象", value: "cpal" },
  { label: "编解码", value: "Opus" },
  { label: "回声消除", value: "aec3" },
  { label: "WebSocket", value: "tokio-tungstenite" },
  { label: "MCP SDK", value: "rmcp" },
]

export function Footer() {
  return (
    <footer className="border-t border-border px-6 py-16">
      <div className="mx-auto max-w-7xl">
        <div className="grid gap-10 md:grid-cols-3">
          <div>
            <div className="flex items-center gap-3">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" className="text-primary-foreground">
                  <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                </svg>
              </div>
              <span className="text-lg font-semibold text-foreground">
                AI Meeting Host
              </span>
            </div>
            <p className="mt-4 max-w-xs text-sm leading-relaxed text-muted-foreground">
              基于 Rust + GPUI 的 AI 会议语音网关，当前已实现音频主链路、AEC3 与 MCP 工具桥接。
            </p>
          </div>

          <div>
            <h4 className="mb-4 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              技术栈
            </h4>
            <div className="grid grid-cols-2 gap-2">
              {techStack.map((item) => (
                <div key={item.label} className="flex items-center gap-2 text-xs">
                  <span className="text-muted-foreground">{item.label}:</span>
                  <span className="font-mono text-foreground">{item.value}</span>
                </div>
              ))}
            </div>
          </div>

          <div>
            <h4 className="mb-4 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              快速导航
            </h4>
            <div className="flex flex-col gap-2">
              {[
                { label: "系统架构", href: "#architecture" },
                { label: "协议兼容", href: "#protocol" },
                { label: "核心功能", href: "#features" },
                { label: "跨平台策略", href: "#platform" },
                { label: "性能优化", href: "#performance" },
                { label: "模块化设计", href: "#modularity" },
                { label: "产品路线图", href: "#roadmap" },
              ].map((link) => (
                <a
                  key={link.href}
                  href={link.href}
                  className="text-xs text-muted-foreground transition-colors hover:text-foreground"
                >
                  {link.label}
                </a>
              ))}
              <Link
                href="/mockup"
                className="text-xs text-primary transition-colors hover:text-primary/80"
              >
                桌面应用 UI 原型
              </Link>
              <a
                href="https://github.com/LiusCraft/conference-hosting"
                target="_blank"
                rel="noreferrer"
                className="text-xs text-primary transition-colors hover:text-primary/80"
              >
                GitHub 仓库
              </a>
              <a
                href="https://linx.qiniu.com/"
                target="_blank"
                rel="noreferrer"
                className="text-xs text-primary transition-colors hover:text-primary/80"
              >
                获取在线智能体接入
              </a>
            </div>
          </div>
        </div>

        <div className="mt-12 border-t border-border pt-6 text-center text-xs text-muted-foreground">
          AI Meeting Host 当前实现文档站 | Rust + GPUI + MCP Bridge | 2026
        </div>
      </div>
    </footer>
  )
}
