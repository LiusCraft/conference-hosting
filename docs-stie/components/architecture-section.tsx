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
    id: "capture",
    label: "音频采集层",
    desc: "CoreAudio (macOS) / WASAPI (Win) / PulseAudio (Linux) 通过 cpal 抽象",
    color: "bg-primary/10 border-primary/30 text-primary",
    dotColor: "bg-primary",
  },
  {
    id: "process",
    label: "音频处理引擎",
    desc: "20ms 分帧 / PCM 16kHz 16bit Mono / Opus 编码 / rubato 重采样",
    color: "bg-chart-3/15 border-chart-3/30 text-chart-3",
    dotColor: "bg-chart-3",
  },
  {
    id: "ws",
    label: "WebSocket 通信层",
    desc: "tokio-tungstenite + rustls 双向流，50 帧/秒上行",
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
    id: "output",
    label: "虚拟麦克风输出",
    desc: "BlackHole (macOS) / VB-Cable (Win) 向会议回放 AI 语音",
    color: "bg-chart-2/15 border-chart-2/30 text-chart-2",
    dotColor: "bg-chart-2",
  },
]

const dataFlowSteps = [
  { step: "01", label: "音频采集", detail: "从系统音频设备获取会议下行音频 PCM 流" },
  { step: "02", label: "帧切分", detail: "以 20ms 为单位切分，每帧 320 samples / 640 bytes" },
  { step: "03", label: "编码上行", detail: "Opus 编码后通过 WebSocket Binary Frame 发送" },
  { step: "04", label: "AI 处理", detail: "服务端 ASR 转文本 -> LLM 推理 -> TTS 语音合成" },
  { step: "05", label: "下行解码", detail: "接收 Opus 音频流，本地解码为 PCM" },
  { step: "06", label: "播放输出", detail: "经虚拟麦克风输出到会议软件，参会者听到 AI 回答" },
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
            端到端音频网关架构
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            从会议音频采集到 AI 语音回放，六层架构确保低延迟、高可靠的实时语音处理链路。
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

            {/* Latency breakdown */}
            <div className="mt-10 rounded-xl border border-border bg-card/50 p-6">
              <h4 className="mb-4 text-sm font-semibold text-foreground">
                延迟分解（目标 {'<'} 1s）
              </h4>
              <div className="flex flex-col gap-3">
                {[
                  { label: "音频帧", ms: 20, pct: 3 },
                  { label: "网络传输", ms: 50, pct: 7 },
                  { label: "ASR 识别", ms: 200, pct: 28 },
                  { label: "AI 推理", ms: 300, pct: 43 },
                  { label: "TTS 合成", ms: 200, pct: 28 },
                ].map((item) => (
                  <div key={item.label}>
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-muted-foreground">{item.label}</span>
                      <span className="font-mono text-foreground">{item.ms}ms</span>
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
                    总延迟
                  </span>
                  <span className="font-mono text-sm font-bold text-primary">
                    ~770ms
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
