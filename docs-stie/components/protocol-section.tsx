import { Cable, Sparkles, ShieldCheck, ExternalLink } from "lucide-react"

const protocolCards = [
  {
    icon: Cable,
    title: "默认接入灵矽协议族",
    desc: "桌面端默认连接灵矽平台协议服务，主链路包含 hello/listen/二进制音频帧。",
    meta: "默认 WS: wss://xrobo-io.qiniuapi.com/v1/ws/",
  },
  {
    icon: Sparkles,
    title: "在 xiaozhi 协议上增强",
    desc: "保持核心消息结构一致，并扩展 features.mcp、intent_trace 等能力字段与事件语义。",
    meta: "增强能力: MCP + 可观测事件",
  },
  {
    icon: ShieldCheck,
    title: "兼容 xiaozhi-server",
    desc: "与 xiaozhi-server 开源版本兼容，可直接复用 hello/listen/音频帧主链路进行联调。",
    meta: "兼容范围: 握手 + 音频双向流",
  },
]

export function ProtocolSection() {
  return (
    <section id="protocol" className="px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            协议兼容
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            灵矽增强协议 + xiaozhi 兼容
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            当前软件默认对接灵矽 WebSocket 协议族，兼容 xiaozhi-server 开源协议，
            并支持通过环境变量覆盖目标地址。
          </p>
        </div>

        <div className="grid gap-5 md:grid-cols-3">
          {protocolCards.map((item) => (
            <div
              key={item.title}
              className="rounded-xl border border-border bg-card/50 p-6 transition-colors hover:border-primary/30"
            >
              <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
                <item.icon size={20} className="text-primary" />
              </div>
              <h3 className="text-sm font-semibold text-foreground">{item.title}</h3>
              <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
                {item.desc}
              </p>
              <p className="mt-4 font-mono text-[11px] text-primary">{item.meta}</p>
            </div>
          ))}
        </div>

        <div className="mt-8 rounded-xl border border-primary/30 bg-primary/5 p-6">
          <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
            <div>
              <p className="text-sm font-semibold text-foreground">快速获取在线智能体接入信息</p>
              <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                前往灵矽平台创建或选择在线智能体，获取可用于本软件的接入信息（如 WS 地址与鉴权参数）。
              </p>
            </div>
            <a
              href="https://linx.qiniu.com/"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
            >
              前往 linx.qiniu.com
              <ExternalLink size={14} />
            </a>
          </div>
        </div>
      </div>
    </section>
  )
}
