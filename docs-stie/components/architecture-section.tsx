"use client"

import { useState } from "react"

const layers = [
  {
    id: "meeting",
    label: "在线会议软件",
    desc: "Zoom / Teams / 腾讯会议 / Google Meet",
    color: "bg-chart-2/15 border-chart-2/30 text-chart-2",
    dotColor: "bg-chart-2",
  },
  {
    id: "ui",
    label: "GPUI 桌面壳",
    desc: "连接控制、设备选择、聊天事件流、设置面板、MCP 管理页面",
    color: "bg-primary/10 border-primary/30 text-primary",
    dotColor: "bg-primary",
  },
  {
    id: "runtime",
    label: "网关运行时",
    desc: "cpal 采集/播放 + Opus 编解码 + AEC3 回声消除 + 命令/事件通道",
    color: "bg-chart-3/15 border-chart-3/30 text-chart-3",
    dotColor: "bg-chart-3",
  },
  {
    id: "ws",
    label: "host-platform WS 适配层",
    desc: "hello 握手、header 注入、超时控制、文本/二进制事件拆分",
    color: "bg-chart-5/15 border-chart-5/30 text-chart-5",
    dotColor: "bg-chart-5",
  },
  {
    id: "ai",
    label: "AI 语音平台",
    desc: "ASR 语音识别 + LLM 大模型推理 + TTS 语音合成",
    color: "bg-chart-4/15 border-chart-4/30 text-chart-4",
    dotColor: "bg-chart-4",
  },
  {
    id: "mcp",
    label: "MCP 上游能力层",
    desc: "stdio / sse / stream 接入，工具以 <alias>.<tool> 命名空间聚合",
    color: "bg-chart-2/15 border-chart-2/30 text-chart-2",
    dotColor: "bg-chart-2",
  },
]

const dataFlowSteps = [
  { step: "01", label: "连接握手", detail: "发送 hello（含 notify/mcp features），等待 session_id" },
  { step: "02", label: "采集上行", detail: "cpal 回调采集输入设备，10ms AEC 处理后聚合为 20ms 音频帧" },
  { step: "03", label: "编码发送", detail: "20ms PCM16 mono 编码为 Opus，通过 WebSocket Binary 连续上送" },
  { step: "04", label: "平台处理", detail: "平台执行 ASR/LLM/TTS，同时下发 stt/tts/notify/mcp 文本事件" },
  { step: "05", label: "下行播放", detail: "客户端解码 Opus 包并播放到选定输出设备，可镜像到系统扬声器" },
  { step: "06", label: "MCP 回包", detail: "处理 initialize/tools/list/tools/call 并按路由返回 JSON-RPC 响应" },
]

export function ArchitectureSection() {
  const [activeLayer, setActiveLayer] = useState<string | null>(null)

  return (
    <section id="architecture" className="relative px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        {/* Section header */}
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            系统架构
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            代码实现映射架构
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            以 host-core / host-platform / host-app-gpui 为主干，串联音频主链路与 MCP 工具链路。
          </p>
        </div>

        <div className="grid gap-16 lg:grid-cols-2">
          {/* Architecture layers */}
          <div className="flex flex-col gap-3">
            {layers.map((layer, index) => (
              <div key={layer.id}>
                <button
                  type="button"
                  className={`group w-full rounded-xl border p-5 text-left transition-all ${layer.color} ${
                    activeLayer === layer.id
                      ? "scale-[1.02] shadow-lg"
                      : "hover:scale-[1.01]"
                  }`}
                  onClick={() =>
                    setActiveLayer(activeLayer === layer.id ? null : layer.id)
                  }
                  onMouseEnter={() => setActiveLayer(layer.id)}
                  onMouseLeave={() => setActiveLayer(null)}
                >
                  <div className="flex items-center gap-3">
                    <span className={`h-2.5 w-2.5 rounded-full ${layer.dotColor}`} />
                    <span className="text-sm font-semibold">{layer.label}</span>
                  </div>
                  <p className="mt-2 pl-5.5 text-xs text-muted-foreground">
                    {layer.desc}
                  </p>
                </button>
                {index < layers.length - 1 && (
                  <div className="flex justify-center py-1">
                    <div className="flex flex-col items-center gap-0.5">
                      <div className="h-2 w-px bg-border" />
                      <svg
                        width="8"
                        height="6"
                        viewBox="0 0 8 6"
                        fill="none"
                        className="text-muted-foreground/50"
                      >
                        <path d="M4 6L0 0h8L4 6z" fill="currentColor" />
                      </svg>
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>

          {/* Data flow timeline */}
          <div>
            <h3 className="mb-8 text-lg font-semibold text-foreground">
              数据流转时序
            </h3>
            <div className="relative">
              {/* Timeline line */}
              <div className="absolute left-5 top-0 bottom-0 w-px bg-border" />
              <div className="flex flex-col gap-6">
                {dataFlowSteps.map((item) => (
                  <div key={item.step} className="group relative flex gap-5 pl-0">
                    {/* Timeline dot */}
                    <div className="relative z-10 flex h-10 w-10 shrink-0 items-center justify-center rounded-full border border-border bg-card text-xs font-bold text-primary transition-colors group-hover:border-primary group-hover:bg-primary/10">
                      {item.step}
                    </div>
                    <div className="pb-2 pt-1">
                      <p className="text-sm font-semibold text-foreground">
                        {item.label}
                      </p>
                      <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
                        {item.detail}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Runtime defaults */}
            <div className="mt-10 rounded-xl border border-border bg-card/50 p-6">
              <h4 className="mb-4 text-sm font-semibold text-foreground">
                运行时关键参数（当前默认）
              </h4>
              <div className="flex flex-col gap-3">
                {[
                  { label: "音频帧时长", value: "20ms", pct: 10 },
                  { label: "WS ping 间隔", value: "2s", pct: 22 },
                  { label: "WS ping 超时", value: "6s", pct: 35 },
                  { label: "hello 超时", value: "5s", pct: 30 },
                  { label: "MCP connect_timeout", value: "3000ms", pct: 18 },
                  { label: "MCP request_timeout", value: "8000ms", pct: 45 },
                ].map((item) => (
                  <div key={item.label}>
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-muted-foreground">{item.label}</span>
                      <span className="font-mono text-foreground">{item.value}</span>
                    </div>
                    <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-secondary">
                      <div
                        className="h-full rounded-full bg-primary transition-all duration-500"
                        style={{ width: `${item.pct}%` }}
                      />
                    </div>
                  </div>
                ))}
                <div className="mt-2 flex items-center justify-between border-t border-border pt-3">
                  <span className="text-xs font-medium text-muted-foreground">
                    备注
                  </span>
                  <span className="font-mono text-sm font-bold text-primary">
                    实时时延以 RTT/AEC 指标面板为准
                  </span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
