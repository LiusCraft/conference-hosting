"use client"

import { X, Server, ShieldCheck, FileCode, Volume2, Globe, Info, Wrench } from "lucide-react"

interface SettingsPanelProps {
  open: boolean
  onClose: () => void
}

function SettingRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-center justify-between py-2 border-b border-border/50 last:border-b-0">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className={`text-xs text-foreground ${mono ? "font-mono" : ""}`}>{value}</span>
    </div>
  )
}

function SettingSection({ title, icon, children }: { title: string; icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2 text-xs font-medium text-foreground">
        {icon}
        {title}
      </div>
      <div className="flex flex-col bg-secondary/30 rounded-lg px-3 py-1">
        {children}
      </div>
    </div>
  )
}

export function SettingsPanel({ open, onClose }: SettingsPanelProps) {
  if (!open) return null

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
      <div className="flex flex-col w-[480px] max-h-[80%] bg-card border border-border rounded-xl shadow-2xl shadow-black/40 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-border">
          <h2 className="text-sm font-semibold text-foreground">设置</h2>
          <button
            onClick={onClose}
            className="flex items-center justify-center w-6 h-6 rounded-md hover:bg-secondary transition-colors cursor-pointer"
          >
            <X className="w-4 h-4 text-muted-foreground" />
          </button>
        </div>

        {/* Content */}
        <div className="flex flex-col gap-5 p-5 overflow-y-auto">
          <SettingSection title="WebSocket 服务器" icon={<Server className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="地址" value="wss://xrobo-io.qiniuapi.com/v1/ws/" mono />
            <SettingRow label="协议版本" value="1" mono />
            <SettingRow label="传输方式" value="WebSocket Binary Frame" />
          </SettingSection>

          <SettingSection title="认证信息" icon={<ShieldCheck className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="Authorization" value="Bearer ****...a3f2" mono />
            <SettingRow label="Device-Id" value="host-macbook-001" mono />
            <SettingRow label="Client-Id" value="2fb2a4e8...7b3f" mono />
          </SettingSection>

          <SettingSection title="音频参数" icon={<Volume2 className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="格式" value="PCM 16bit" />
            <SettingRow label="采样率" value="16000 Hz" mono />
            <SettingRow label="声道" value="Mono" />
            <SettingRow label="帧时长" value="20ms" mono />
            <SettingRow label="帧大小" value="320 samples / 640 bytes" mono />
            <SettingRow label="发送频率" value="50 帧/秒" mono />
            <SettingRow label="编解码" value="Opus (上行/下行)" />
            <SettingRow label="AEC" value="AEC3 已启用" />
            <SettingRow label="Listen Mode" value="realtime" mono />
          </SettingSection>

          <SettingSection title="平台适配" icon={<Globe className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="操作系统" value="macOS" />
            <SettingRow label="音频采集" value="CoreAudio (cpal) + loopback" />
            <SettingRow label="输出路由" value="BlackHole 2ch" />
            <SettingRow label="回声消除" value="AEC3 + 动态 stream delay" />
          </SettingSection>

          <SettingSection title="MCP 网桥" icon={<Wrench className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="Transport" value="stdio / sse / stream" mono />
            <SettingRow label="JSON-RPC" value="initialize / tools/list / tools/call" mono />
            <SettingRow label="工具命名" value="{alias}.{tool}" mono />
            <SettingRow label="刷新策略" value="单个或全量刷新" />
          </SettingSection>

          <SettingSection title="工程信息" icon={<FileCode className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="引擎" value="Rust + GPUI" />
            <SettingRow label="运行时" value="Tokio async runtime" />
            <SettingRow label="通信" value="tokio-tungstenite + rustls" />
            <SettingRow label="音频层" value="cpal + opus + aec3" />
          </SettingSection>

          <SettingSection title="关于" icon={<Info className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="版本" value="0.1.0-alpha" mono />
            <SettingRow label="构建" value="Cargo Workspace" />
            <SettingRow label="Crates" value="host-core / host-platform / host-app-gpui" mono />
          </SettingSection>
        </div>
      </div>
    </div>
  )
}
