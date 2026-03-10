"use client"

import { useMemo, useState } from "react"
import {
  X,
  Server,
  ShieldCheck,
  FileCode,
  Volume2,
  Globe,
  Info,
  Wrench,
  Mic,
  Plus,
  RefreshCw,
  SquarePen,
  Trash2,
} from "lucide-react"

interface SettingsPanelProps {
  open: boolean
  onClose: () => void
}

type SpeechMode = "manual" | "auto" | "realtime"
type McpTransport = "stdio" | "sse" | "stream"

interface MockMcpServer {
  id: string
  alias: string
  transport: McpTransport
  endpoint: string
  enabled: boolean
  toolCount: number
}

const INITIAL_MCP_SERVERS: MockMcpServer[] = [
  {
    id: "calendar-service",
    alias: "calendar",
    transport: "sse",
    endpoint: "https://mcp.example.com/calendar/sse",
    enabled: true,
    toolCount: 5,
  },
  {
    id: "notes-stdio",
    alias: "note",
    transport: "stdio",
    endpoint: "uvx mcp-notes",
    enabled: true,
    toolCount: 2,
  },
  {
    id: "task-stream",
    alias: "task",
    transport: "stream",
    endpoint: "https://mcp.example.com/task/stream",
    enabled: false,
    toolCount: 0,
  },
]

const SPEECH_MODE_OPTIONS: Record<SpeechMode, { title: string; desc: string }> = {
  manual: {
    title: "手动触发",
    desc: "manual: 由设备端按键控制开始/停止监听，适合精确控制采集时机。",
  },
  auto: {
    title: "唤醒词触发",
    desc: "auto: 通过唤醒词触发并支持打断播放，适合低功耗待机场景。",
  },
  realtime: {
    title: "自由对话",
    desc: "realtime: 全双工对话模式，检测到语音后可实时打断 AI 说话。",
  },
}

const MCP_ENDPOINT_PLACEHOLDER: Record<McpTransport, string> = {
  stdio: "uvx mcp-notes",
  sse: "https://mcp.example.com/sse",
  stream: "https://mcp.example.com/stream",
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
  const [aecEnabled, setAecEnabled] = useState(true)
  const [speechMode, setSpeechMode] = useState<SpeechMode>("realtime")
  const [mcpServers, setMcpServers] = useState<MockMcpServer[]>(INITIAL_MCP_SERVERS)
  const [showMcpEditor, setShowMcpEditor] = useState(false)
  const [editingServerId, setEditingServerId] = useState<string | null>(null)
  const [mcpAlias, setMcpAlias] = useState("")
  const [mcpTransport, setMcpTransport] = useState<McpTransport>("stdio")
  const [mcpEndpoint, setMcpEndpoint] = useState("")
  const [mcpNotice, setMcpNotice] = useState<string | null>(null)
  const [mcpError, setMcpError] = useState<string | null>(null)

  const speechModeDetail = useMemo(() => SPEECH_MODE_OPTIONS[speechMode], [speechMode])
  const enabledMcpCount = useMemo(() => mcpServers.filter((server) => server.enabled).length, [mcpServers])
  const toolCount = useMemo(
    () => mcpServers.reduce((count, server) => count + (server.enabled ? server.toolCount : 0), 0),
    [mcpServers]
  )

  const resetMcpEditor = () => {
    setShowMcpEditor(false)
    setEditingServerId(null)
    setMcpAlias("")
    setMcpTransport("stdio")
    setMcpEndpoint("")
    setMcpError(null)
  }

  const beginAddMcpServer = () => {
    setShowMcpEditor(true)
    setEditingServerId(null)
    setMcpAlias("")
    setMcpTransport("stdio")
    setMcpEndpoint("")
    setMcpError(null)
    setMcpNotice("已切换到新增模式")
  }

  const beginEditMcpServer = (server: MockMcpServer) => {
    setShowMcpEditor(true)
    setEditingServerId(server.id)
    setMcpAlias(server.alias)
    setMcpTransport(server.transport)
    setMcpEndpoint(server.endpoint)
    setMcpError(null)
    setMcpNotice(`正在编辑 ${server.alias}`)
  }

  const saveMcpServer = () => {
    const alias = mcpAlias.trim()
    const endpoint = mcpEndpoint.trim()
    if (!alias || !endpoint) {
      setMcpError("alias 和 endpoint 不能为空")
      setMcpNotice(null)
      return
    }

    if (editingServerId) {
      setMcpServers((servers) =>
        servers.map((server) =>
          server.id === editingServerId
            ? {
                ...server,
                alias,
                transport: mcpTransport,
                endpoint,
              }
            : server
        )
      )
      setMcpNotice(`已更新 ${alias}`)
    } else {
      setMcpServers((servers) => [
        ...servers,
        {
          id: `${alias.toLowerCase().replace(/\s+/g, "-")}-${Date.now().toString(36)}`,
          alias,
          transport: mcpTransport,
          endpoint,
          enabled: true,
          toolCount: 0,
        },
      ])
      setMcpNotice(`已添加 ${alias}`)
    }

    setMcpError(null)
    resetMcpEditor()
  }

  const toggleMcpServerEnabled = (serverId: string) => {
    let changedServer: MockMcpServer | null = null
    setMcpServers((servers) =>
      servers.map((server) => {
        if (server.id !== serverId) {
          return server
        }

        const nextServer = {
          ...server,
          enabled: !server.enabled,
        }
        changedServer = nextServer
        return nextServer
      })
    )

    if (changedServer) {
      setMcpNotice(`${changedServer.alias} 已${changedServer.enabled ? "启用" : "禁用"}`)
      setMcpError(null)
    }
  }

  const deleteMcpServer = (serverId: string) => {
    const target = mcpServers.find((server) => server.id === serverId)
    if (!target) {
      return
    }

    setMcpServers((servers) => servers.filter((server) => server.id !== serverId))
    setMcpNotice(`已删除 ${target.alias}`)
    setMcpError(null)
  }

  const refreshMcpTools = (serverId?: string) => {
    if (serverId) {
      let refreshedAlias = ""
      setMcpServers((servers) =>
        servers.map((server) => {
          if (server.id !== serverId) {
            return server
          }

          refreshedAlias = server.alias
          return {
            ...server,
            toolCount: server.enabled ? ((server.toolCount + 1) % 8) + 1 : 0,
          }
        })
      )
      if (refreshedAlias) {
        setMcpNotice(`已刷新 ${refreshedAlias} 的 tools`)
      }
      setMcpError(null)
      return
    }

    setMcpServers((servers) =>
      servers.map((server) => ({
        ...server,
        toolCount: server.enabled ? ((server.toolCount + 2) % 8) + 1 : 0,
      }))
    )
    setMcpNotice("MCP tools 已完成全量刷新")
    setMcpError(null)
  }

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
          <SettingSection title="MCP Servers" icon={<Wrench className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="Server 数量" value={`${mcpServers.length} 个（启用 ${enabledMcpCount} 个）`} />
            <SettingRow label="已发现工具" value={`${toolCount} 个`} mono />

            <div className="flex flex-col gap-2 py-2 border-t border-border/50">
              {mcpServers.length === 0 ? (
                <div className="px-2 py-1 text-xs text-muted-foreground">尚未添加 MCP server，可通过下方表单创建</div>
              ) : (
                mcpServers.map((server) => (
                  <div key={server.id} className="rounded-md border border-border/60 bg-background/60 p-2.5">
                    <div className="flex items-center justify-between gap-2">
                      <div className="flex items-center gap-1.5 min-w-0">
                        <span className="text-xs font-medium text-foreground truncate">{server.alias}</span>
                        <span className="px-1.5 py-0.5 rounded bg-secondary text-[10px] font-mono text-muted-foreground uppercase">
                          {server.transport}
                        </span>
                        <span
                          className={`px-1.5 py-0.5 rounded text-[10px] font-mono uppercase ${
                            server.enabled
                              ? "bg-primary/15 text-primary"
                              : "bg-muted text-muted-foreground"
                          }`}
                        >
                          {server.enabled ? "enabled" : "disabled"}
                        </span>
                      </div>
                      <span className="text-[10px] font-mono text-muted-foreground">
                        {server.enabled ? `${server.toolCount} tools` : "0 tools"}
                      </span>
                    </div>

                    <p className="mt-1 text-[10px] font-mono text-muted-foreground/80 truncate">{server.endpoint}</p>

                    <div className="mt-2 flex items-center gap-1.5">
                      <button
                        onClick={() => beginEditMcpServer(server)}
                        className="flex items-center gap-1 px-2 py-1 rounded border border-border text-[10px] text-foreground hover:bg-secondary transition-colors cursor-pointer"
                      >
                        <SquarePen className="w-3 h-3" />
                        编辑
                      </button>
                      <button
                        onClick={() => toggleMcpServerEnabled(server.id)}
                        className="px-2 py-1 rounded border border-border text-[10px] text-foreground hover:bg-secondary transition-colors cursor-pointer"
                      >
                        {server.enabled ? "禁用" : "启用"}
                      </button>
                      <button
                        onClick={() => refreshMcpTools(server.id)}
                        className="flex items-center gap-1 px-2 py-1 rounded border border-border text-[10px] text-foreground hover:bg-secondary transition-colors cursor-pointer"
                      >
                        <RefreshCw className="w-3 h-3" />
                        刷新工具
                      </button>
                      <button
                        onClick={() => deleteMcpServer(server.id)}
                        className="flex items-center gap-1 px-2 py-1 rounded border border-destructive/40 text-[10px] text-destructive hover:bg-destructive/10 transition-colors cursor-pointer"
                      >
                        <Trash2 className="w-3 h-3" />
                        删除
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>

            <div className="flex items-center gap-2 py-2 border-t border-border/50">
              <button
                onClick={beginAddMcpServer}
                className="flex items-center gap-1 px-2.5 py-1.5 rounded border border-border text-xs text-foreground hover:bg-secondary transition-colors cursor-pointer"
              >
                <Plus className="w-3.5 h-3.5" />
                新增 server
              </button>
              <button
                onClick={() => refreshMcpTools()}
                className="flex items-center gap-1 px-2.5 py-1.5 rounded border border-border text-xs text-foreground hover:bg-secondary transition-colors cursor-pointer"
              >
                <RefreshCw className="w-3.5 h-3.5" />
                立即刷新工具
              </button>
            </div>

            {showMcpEditor && (
              <div className="flex flex-col gap-2 mt-1 p-3 rounded-md border border-border/60 bg-background/70">
                <div className="text-xs font-medium text-foreground">{editingServerId ? "编辑 MCP Server" : "新增 MCP Server"}</div>

                <input
                  value={mcpAlias}
                  onChange={(event) => setMcpAlias(event.target.value)}
                  placeholder="Alias"
                  className="h-8 px-2.5 rounded border border-border bg-background text-xs text-foreground placeholder:text-muted-foreground/70 outline-none focus:border-primary/70"
                />

                <div className="grid grid-cols-[88px_1fr] gap-2">
                  <select
                    value={mcpTransport}
                    onChange={(event) => setMcpTransport(event.target.value as McpTransport)}
                    className="h-8 px-2 rounded border border-border bg-background text-xs text-foreground outline-none focus:border-primary/70"
                  >
                    <option value="stdio">stdio</option>
                    <option value="sse">sse</option>
                    <option value="stream">stream</option>
                  </select>
                  <input
                    value={mcpEndpoint}
                    onChange={(event) => setMcpEndpoint(event.target.value)}
                    placeholder={MCP_ENDPOINT_PLACEHOLDER[mcpTransport]}
                    className="h-8 px-2.5 rounded border border-border bg-background text-xs text-foreground placeholder:text-muted-foreground/70 outline-none focus:border-primary/70"
                  />
                </div>

                <div className="flex items-center justify-end gap-2 pt-1">
                  <button
                    onClick={resetMcpEditor}
                    className="px-2.5 py-1 rounded border border-border text-xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                  >
                    重置
                  </button>
                  <button
                    onClick={saveMcpServer}
                    className="px-2.5 py-1 rounded border border-primary/40 bg-primary/15 text-xs text-primary hover:bg-primary/20 transition-colors cursor-pointer"
                  >
                    {editingServerId ? "保存修改" : "添加 server"}
                  </button>
                </div>
              </div>
            )}

            {mcpNotice && <p className="py-1 text-[11px] text-primary">{mcpNotice}</p>}
            {mcpError && <p className="py-1 text-[11px] text-destructive">{mcpError}</p>}
          </SettingSection>

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
            <SettingRow label="AEC" value={aecEnabled ? "启用（AEC3）" : "关闭"} />

            <div className="flex items-start justify-between gap-3 py-2 border-t border-border/50">
              <div className="flex flex-col gap-0.5">
                <span className="text-xs text-foreground">实时回声消除（AEC）</span>
                <span className="text-[11px] text-muted-foreground leading-relaxed">
                  使用 AEC3 处理麦克风上行，并根据采集/播放延迟动态调整 stream delay。
                </span>
              </div>
              <button
                onClick={() => setAecEnabled((enabled) => !enabled)}
                className={`shrink-0 px-2.5 py-1 rounded-md border text-xs transition-colors cursor-pointer ${
                  aecEnabled
                    ? "border-primary/40 bg-primary/15 text-primary"
                    : "border-border bg-secondary text-muted-foreground"
                }`}
              >
                {aecEnabled ? "已启用" : "已关闭"}
              </button>
            </div>
          </SettingSection>

          <SettingSection title="语音监听模式" icon={<Mic className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="当前模式" value={`${speechModeDetail.title} (${speechMode})`} mono />

            <div className="flex items-center gap-1 py-2 border-t border-border/50">
              {(["manual", "auto", "realtime"] as SpeechMode[]).map((mode) => (
                <button
                  key={mode}
                  onClick={() => setSpeechMode(mode)}
                  className={`px-2.5 py-1 rounded-md border text-[11px] font-mono uppercase transition-colors cursor-pointer ${
                    speechMode === mode
                      ? "border-primary/40 bg-primary/15 text-primary"
                      : "border-border bg-background text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {mode}
                </button>
              ))}
            </div>

            <p className="pb-2 text-[11px] leading-relaxed text-muted-foreground">{speechModeDetail.desc}</p>
          </SettingSection>

          <SettingSection title="平台适配" icon={<Globe className="w-3.5 h-3.5 text-primary" />}>
            <SettingRow label="操作系统" value="macOS" />
            <SettingRow label="输入设备" value="BlackHole 2ch (Loopback)" />
            <SettingRow label="输出设备" value="BlackHole 2ch" />
            <SettingRow label="回声消除" value={aecEnabled ? "AEC3 + 动态 stream delay" : "已关闭"} />
            <SettingRow label="语音模式" value={speechMode} mono />
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
