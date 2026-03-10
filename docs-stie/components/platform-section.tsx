"use client"

import { useState } from "react"
import { Monitor, Apple, Terminal, Check, Clock, Minus } from "lucide-react"

type PlatformId = "macos" | "windows" | "linux"

const platforms: {
  id: PlatformId
  icon: typeof Apple
  name: string
  status: string
  statusColor: string
  audioCapture: string
  audioOutput: string
  virtualMic: string
  uiEngine: string
  priority: string
}[] = [
  {
    id: "macos",
    icon: Apple,
    name: "macOS",
    status: "已验证",
    statusColor: "text-primary",
    audioCapture: "CoreAudio (cpal) + loopback 输入选择",
    audioOutput: "默认输出或 BlackHole 等虚拟设备",
    virtualMic: "通过输出设备路由到 BlackHole/Loopback",
    uiEngine: "GPUI Metal 后端渲染",
    priority: "MVP 阶段",
  },
  {
    id: "windows",
    icon: Monitor,
    name: "Windows",
    status: "已适配",
    statusColor: "text-chart-2",
    audioCapture: "WASAPI (cpal) + 输出设备回采模式",
    audioOutput: "默认输出或虚拟音频设备",
    virtualMic: "可路由到 VB-Cable / Virtual Audio Cable",
    uiEngine: "GPUI DirectX/Vulkan 后端",
    priority: "联调阶段",
  },
  {
    id: "linux",
    icon: Terminal,
    name: "Linux",
    status: "代码就绪",
    statusColor: "text-chart-5",
    audioCapture: "ALSA/PulseAudio/PipeWire (cpal 抽象)",
    audioOutput: "默认 Sink 或虚拟设备",
    virtualMic: "可通过虚拟设备路由（待实机完善）",
    uiEngine: "GPUI Vulkan 后端",
    priority: "联调阶段",
  },
]

const compatMatrix = [
  { feature: "音频设备枚举/选择", macos: "done", windows: "done", linux: "done" },
  { feature: "Opus 编解码", macos: "done", windows: "done", linux: "done" },
  { feature: "WebSocket 通信", macos: "done", windows: "done", linux: "done" },
  { feature: "AEC3 运行时开关", macos: "done", windows: "done", linux: "done" },
  { feature: "MCP 管理页 + 持久化", macos: "done", windows: "done", linux: "done" },
  { feature: "MCP tools 桥接", macos: "done", windows: "done", linux: "done" },
  { feature: "Loopback 回采", macos: "done", windows: "partial", linux: "partial" },
  { feature: "音频镜像监听", macos: "done", windows: "partial", linux: "partial" },
  { feature: "虚拟麦克风一键编排", macos: "partial", windows: "planned", linux: "planned" },
  { feature: "应用签名分发", macos: "partial", windows: "planned", linux: "planned" },
]

function StatusIcon({ status }: { status: string }) {
  if (status === "done")
    return <Check size={14} className="text-primary" />
  if (status === "partial")
    return <Clock size={14} className="text-chart-2" />
  return <Minus size={14} className="text-muted-foreground/40" />
}

export function PlatformSection() {
  const [activePlatform, setActivePlatform] = useState<PlatformId>("macos")
  const current = platforms.find((p) => p.id === activePlatform)!

  return (
    <section id="platform" className="px-6 py-24 md:py-32">
      <div className="mx-auto max-w-7xl">
        <div className="mb-16 max-w-2xl">
          <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
            跨平台策略
          </p>
          <h2 className="text-3xl font-bold tracking-tight text-foreground md:text-4xl">
            一套核心，多平台适配
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            当前代码以 cpal + Opus + AEC3 + rmcp 为跨平台公共主干，
            平台差异主要集中在设备路由和虚拟音频生态联调。
          </p>
        </div>

        <div className="grid gap-8 lg:grid-cols-5">
          {/* Platform selector */}
          <div className="flex flex-col gap-3 lg:col-span-2">
            {platforms.map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => setActivePlatform(p.id)}
                className={`flex items-start gap-4 rounded-xl border p-5 text-left transition-all ${
                  activePlatform === p.id
                    ? "border-primary/40 bg-primary/5"
                    : "border-border bg-card/30 hover:border-border hover:bg-card/50"
                }`}
              >
                <div
                  className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-lg ${
                    activePlatform === p.id
                      ? "bg-primary/15"
                      : "bg-secondary"
                  }`}
                >
                  <p.icon
                    size={20}
                    className={
                      activePlatform === p.id
                        ? "text-primary"
                        : "text-muted-foreground"
                    }
                  />
                </div>
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-semibold text-foreground">
                      {p.name}
                    </span>
                    <span className={`text-xs font-medium ${p.statusColor}`}>
                      {p.status}
                    </span>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    音频采集: {p.audioCapture}
                  </p>
                </div>
              </button>
            ))}
          </div>

          {/* Platform detail */}
          <div className="flex flex-col gap-6 lg:col-span-3">
            <div className="rounded-xl border border-border bg-card/50 p-6">
              <h3 className="mb-5 flex items-center gap-2 text-base font-semibold text-foreground">
                <current.icon size={18} className="text-primary" />
                {current.name} 平台实现详情
              </h3>
              <div className="grid gap-4 sm:grid-cols-2">
                {[
                  { label: "音频采集", value: current.audioCapture },
                  { label: "音频输出", value: current.audioOutput },
                  { label: "虚拟麦克风", value: current.virtualMic },
                  { label: "UI 渲染引擎", value: current.uiEngine },
                ].map((item) => (
                  <div
                    key={item.label}
                    className="rounded-lg border border-border bg-secondary/30 p-4"
                  >
                    <p className="text-xs text-muted-foreground">{item.label}</p>
                    <p className="mt-1 font-mono text-xs font-medium text-foreground">
                      {item.value}
                    </p>
                  </div>
                ))}
              </div>
            </div>

            {/* Compat matrix */}
            <div className="rounded-xl border border-border bg-card/50 p-6">
              <h3 className="mb-4 text-sm font-semibold text-foreground">
                功能兼容矩阵
              </h3>
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-border">
                      <th className="pb-3 text-left font-medium text-muted-foreground">
                        功能模块
                      </th>
                      <th className="pb-3 text-center font-medium text-muted-foreground">
                        macOS
                      </th>
                      <th className="pb-3 text-center font-medium text-muted-foreground">
                        Windows
                      </th>
                      <th className="pb-3 text-center font-medium text-muted-foreground">
                        Linux
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {compatMatrix.map((row) => (
                      <tr
                        key={row.feature}
                        className="border-b border-border/50 last:border-0"
                      >
                        <td className="py-3 font-mono text-foreground">
                          {row.feature}
                        </td>
                        <td className="py-3 text-center">
                          <span className="inline-flex items-center justify-center">
                            <StatusIcon status={row.macos} />
                          </span>
                        </td>
                        <td className="py-3 text-center">
                          <span className="inline-flex items-center justify-center">
                            <StatusIcon status={row.windows} />
                          </span>
                        </td>
                        <td className="py-3 text-center">
                          <span className="inline-flex items-center justify-center">
                            <StatusIcon status={row.linux} />
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="mt-4 flex flex-wrap gap-4 text-[10px] text-muted-foreground">
                <span className="flex items-center gap-1">
                  <Check size={10} className="text-primary" /> 已实现
                </span>
                <span className="flex items-center gap-1">
                  <Clock size={10} className="text-chart-2" /> 进行中
                </span>
                <span className="flex items-center gap-1">
                  <Minus size={10} className="text-muted-foreground/40" /> 计划中
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
