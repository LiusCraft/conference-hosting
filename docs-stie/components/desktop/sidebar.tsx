"use client"

import { useState } from "react"
import {
  Wifi,
  WifiOff,
  Mic,
  MicOff,
  Volume2,
  VolumeX,
  ChevronDown,
  Settings,
  Radio,
  Activity,
  MonitorSpeaker,
  Headphones,
  Cable,
} from "lucide-react"

const INPUT_DEVICES = [
  { id: "blackhole-loopback", name: "BlackHole 2ch (Loopback)", type: "virtual" as const },
  { id: "macbook-mic", name: "MacBook Pro Microphone", type: "builtin" as const },
  { id: "external-mic", name: "Blue Yeti USB", type: "external" as const },
]

const OUTPUT_DEVICES = [
  { id: "blackhole-out", name: "BlackHole 2ch", type: "virtual" as const },
  { id: "macbook-speaker", name: "MacBook Pro Speakers", type: "builtin" as const },
  { id: "airpods", name: "AirPods Pro", type: "external" as const },
]

interface AudioLevelMeterProps {
  level: number
  label: string
  active: boolean
}

function AudioLevelMeter({ level, label, active }: AudioLevelMeterProps) {
  const bars = 20
  const activeBars = Math.round((level / 100) * bars)

  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider">{label}</span>
      <div className="flex items-center gap-[2px] h-3">
        {Array.from({ length: bars }).map((_, i) => {
          const isActive = i < activeBars && active
          let color = "bg-primary/80"
          if (i >= bars * 0.7) color = "bg-accent/80"
          if (i >= bars * 0.9) color = "bg-destructive/80"

          return (
            <div
              key={i}
              className={`w-1.5 h-full rounded-[1px] transition-colors duration-75 ${
                isActive ? color : "bg-border/40"
              }`}
            />
          )
        })}
      </div>
    </div>
  )
}

interface DeviceSelectorProps {
  label: string
  icon: React.ReactNode
  devices: typeof INPUT_DEVICES
  selected: string
  onSelect: (id: string) => void
}

function DeviceSelector({ label, icon, devices, selected, onSelect }: DeviceSelectorProps) {
  const [open, setOpen] = useState(false)
  const selectedDevice = devices.find((d) => d.id === selected)

  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
        {icon}
        {label}
      </span>
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center justify-between px-2.5 py-2 bg-background rounded-md border border-border text-xs text-foreground hover:border-primary/50 transition-colors cursor-pointer"
      >
        <div className="flex items-center gap-2 truncate">
          {selectedDevice?.type === "virtual" && <Cable className="w-3 h-3 text-primary shrink-0" />}
          {selectedDevice?.type === "builtin" && <MonitorSpeaker className="w-3 h-3 text-muted-foreground shrink-0" />}
          {selectedDevice?.type === "external" && <Headphones className="w-3 h-3 text-accent shrink-0" />}
          <span className="truncate">{selectedDevice?.name}</span>
        </div>
        <ChevronDown className={`w-3 h-3 text-muted-foreground shrink-0 ml-1 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="flex flex-col rounded-md border border-border bg-popover overflow-hidden">
          {devices.map((device) => (
            <button
              key={device.id}
              onClick={() => {
                onSelect(device.id)
                setOpen(false)
              }}
              className={`flex items-center gap-2 px-2.5 py-2 text-xs text-left transition-colors cursor-pointer ${
                device.id === selected
                  ? "bg-primary/10 text-primary"
                  : "text-foreground hover:bg-secondary"
              }`}
            >
              {device.type === "virtual" && <Cable className="w-3 h-3 text-primary shrink-0" />}
              {device.type === "builtin" && <MonitorSpeaker className="w-3 h-3 text-muted-foreground shrink-0" />}
              {device.type === "external" && <Headphones className="w-3 h-3 text-accent shrink-0" />}
              <span className="truncate">{device.name}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

interface SidebarProps {
  connected: boolean
  onToggleConnect: () => void
  onOpenSettings: () => void
  micActive: boolean
  onToggleMic: () => void
  speakerActive: boolean
  onToggleSpeaker: () => void
  inputLevel: number
  outputLevel: number
}

export function Sidebar({
  connected,
  onToggleConnect,
  onOpenSettings,
  micActive,
  onToggleMic,
  speakerActive,
  onToggleSpeaker,
  inputLevel,
  outputLevel,
}: SidebarProps) {
  const [inputDevice, setInputDevice] = useState("blackhole-loopback")
  const [outputDevice, setOutputDevice] = useState("blackhole-out")

  return (
    <div className="flex flex-col w-[260px] bg-sidebar border-r border-sidebar-border shrink-0 select-none">
      {/* Connection Status */}
      <div className="flex flex-col gap-3 p-4 border-b border-sidebar-border">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className={`w-2 h-2 rounded-full ${connected ? "bg-primary animate-pulse" : "bg-muted-foreground"}`} />
            <span className="text-xs font-medium text-foreground">
              {connected ? "WebSocket" : "WebSocket"}
            </span>
          </div>
          <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded ${
            connected ? "bg-primary/15 text-primary" : "bg-muted text-muted-foreground"
          }`}>
            {connected ? "CONNECTED" : "DISCONNECTED"}
          </span>
        </div>

        {connected && (
          <div className="flex items-center gap-4 text-[10px] font-mono text-muted-foreground">
            <div className="flex items-center gap-1">
              <Activity className="w-3 h-3 text-primary" />
              <span>RTT 48ms</span>
            </div>
            <div className="flex items-center gap-1">
              <Radio className="w-3 h-3 text-primary" />
              <span>50 fps</span>
            </div>
          </div>
        )}

        <button
          onClick={onToggleConnect}
          className={`flex items-center justify-center gap-2 w-full py-2 rounded-md text-xs font-medium transition-colors cursor-pointer ${
            connected
              ? "bg-destructive/15 text-destructive hover:bg-destructive/25 border border-destructive/30"
              : "bg-primary text-primary-foreground hover:bg-primary/90"
          }`}
        >
          {connected ? (
            <>
              <WifiOff className="w-3.5 h-3.5" />
              断开连接
            </>
          ) : (
            <>
              <Wifi className="w-3.5 h-3.5" />
              连接服务器
            </>
          )}
        </button>
      </div>

      {/* Audio Devices */}
      <div className="flex flex-col gap-4 p-4 border-b border-sidebar-border overflow-y-auto">
        <div className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">
          音频设备
        </div>

        <DeviceSelector
          label="输入源 (采集)"
          icon={<Mic className="w-3 h-3" />}
          devices={INPUT_DEVICES}
          selected={inputDevice}
          onSelect={setInputDevice}
        />

        <DeviceSelector
          label="输出源 (播放)"
          icon={<Volume2 className="w-3 h-3" />}
          devices={OUTPUT_DEVICES}
          selected={outputDevice}
          onSelect={setOutputDevice}
        />
      </div>

      {/* Audio Levels */}
      <div className="flex flex-col gap-3 p-4 border-b border-sidebar-border">
        <div className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">
          电平指示
        </div>
        <AudioLevelMeter level={inputLevel} label="INPUT" active={micActive && connected} />
        <AudioLevelMeter level={outputLevel} label="OUTPUT" active={speakerActive && connected} />
      </div>

      {/* Quick Toggles */}
      <div className="flex flex-col gap-2 p-4 border-b border-sidebar-border">
        <button
          onClick={onToggleMic}
          className={`flex items-center gap-2.5 px-3 py-2 rounded-md text-xs transition-colors cursor-pointer ${
            micActive
              ? "bg-primary/10 text-primary border border-primary/20"
              : "bg-secondary text-muted-foreground border border-transparent"
          }`}
        >
          {micActive ? <Mic className="w-3.5 h-3.5" /> : <MicOff className="w-3.5 h-3.5" />}
          {micActive ? "采集中" : "采集已暂停"}
        </button>

        <button
          onClick={onToggleSpeaker}
          className={`flex items-center gap-2.5 px-3 py-2 rounded-md text-xs transition-colors cursor-pointer ${
            speakerActive
              ? "bg-primary/10 text-primary border border-primary/20"
              : "bg-secondary text-muted-foreground border border-transparent"
          }`}
        >
          {speakerActive ? <Volume2 className="w-3.5 h-3.5" /> : <VolumeX className="w-3.5 h-3.5" />}
          {speakerActive ? "播放中" : "播放已暂停"}
        </button>
      </div>

      {/* Settings */}
      <div className="mt-auto p-4">
        <button
          onClick={onOpenSettings}
          className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
        >
          <Settings className="w-3.5 h-3.5" />
          设置
        </button>
      </div>
    </div>
  )
}
