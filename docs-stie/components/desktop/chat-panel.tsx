"use client"

import { useState, useRef, useEffect, useCallback } from "react"
import { Send, Bot, User, Wrench, AudioLines, ChevronDown } from "lucide-react"

type MessageType = "stt" | "ai" | "tool" | "system" | "user"

interface Message {
  id: string
  type: MessageType
  text: string
  timestamp: string
  audioDuration?: string
}

const MOCK_MESSAGES: Message[] = [
  { id: "1", type: "system", text: "WebSocket 连接已建立", timestamp: "14:23:01" },
  { id: "2", type: "system", text: "hello 握手成功 | PCM 16kHz Mono 16bit 20ms", timestamp: "14:23:01" },
  { id: "3", type: "system", text: "开始音频采集: BlackHole 2ch (Loopback)", timestamp: "14:23:02" },
  { id: "4", type: "stt", text: "大家好，今天我们来讨论一下第二季度的营销策略。", timestamp: "14:23:15" },
  { id: "5", type: "tool", text: "function_call: analyze_context({\"topic\": \"Q2 marketing strategy\", \"participants\": 4})", timestamp: "14:23:16" },
  { id: "6", type: "ai", text: "好的，我来做一些补充。根据第一季度的数据，我们在社交媒体渠道的转化率提升了 23%，建议第二季度加大在短视频平台的投入。", timestamp: "14:23:18", audioDuration: "8.2s" },
  { id: "7", type: "stt", text: "这个建议不错，你能详细说一下具体的预算分配吗？", timestamp: "14:23:35" },
  { id: "8", type: "ai", text: "建议将总预算的 40% 分配给短视频平台，包括抖音和视频号。25% 用于搜索引擎优化，20% 用于邮件营销的自动化升级，剩余 15% 作为 A/B 测试的灵活预算。", timestamp: "14:23:38", audioDuration: "12.5s" },
  { id: "9", type: "stt", text: "那关于 KPI 的设定呢？我们需要设定哪些关键指标？", timestamp: "14:24:02" },
  { id: "10", type: "tool", text: "function_call: get_kpi_template({\"domain\": \"digital_marketing\", \"quarter\": \"Q2\"})", timestamp: "14:24:03" },
  { id: "11", type: "ai", text: "核心 KPI 建议如下：第一，新增获客成本控制在 45 元以下；第二，短视频平台的自然流量增长目标 30%；第三，用户留存率从当前的 62% 提升到 70%；第四，ROI 整体目标不低于 3.5 倍。", timestamp: "14:24:06", audioDuration: "15.1s" },
  { id: "12", type: "stt", text: "好的，这些指标看起来可行。我们接下来讨论一下时间节点。", timestamp: "14:24:30" },
]

function MessageBubble({ message }: { message: Message }) {
  if (message.type === "system") {
    return (
      <div className="flex justify-center py-1">
        <span className="text-[10px] font-mono text-muted-foreground/60 px-3 py-1 rounded-full bg-secondary/30">
          {message.timestamp} | {message.text}
        </span>
      </div>
    )
  }

  if (message.type === "tool") {
    return (
      <div className="flex items-start gap-2 px-4 py-1.5">
        <div className="flex items-center justify-center w-5 h-5 rounded bg-accent/15 shrink-0 mt-0.5">
          <Wrench className="w-3 h-3 text-accent" />
        </div>
        <div className="flex flex-col gap-0.5">
          <code className="text-[11px] font-mono text-accent/80 leading-relaxed break-all">
            {message.text}
          </code>
          <span className="text-[9px] font-mono text-muted-foreground/50">{message.timestamp}</span>
        </div>
      </div>
    )
  }

  const isAI = message.type === "ai"
  const isSTT = message.type === "stt"

  return (
    <div className={`flex items-start gap-2.5 px-4 py-1.5 ${isAI ? "" : ""}`}>
      <div
        className={`flex items-center justify-center w-6 h-6 rounded-md shrink-0 mt-0.5 ${
          isAI ? "bg-primary/15" : isSTT ? "bg-secondary" : "bg-accent/15"
        }`}
      >
        {isAI ? (
          <Bot className="w-3.5 h-3.5 text-primary" />
        ) : (
          <User className="w-3.5 h-3.5 text-muted-foreground" />
        )}
      </div>
      <div className="flex flex-col gap-1 min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className={`text-[10px] font-mono uppercase tracking-wider ${
            isAI ? "text-primary" : isSTT ? "text-muted-foreground" : "text-accent"
          }`}>
            {isAI ? "AI" : isSTT ? "STT" : "USER"}
          </span>
          <span className="text-[9px] font-mono text-muted-foreground/50">{message.timestamp}</span>
          {message.audioDuration && (
            <span className="flex items-center gap-0.5 text-[9px] font-mono text-primary/60">
              <AudioLines className="w-2.5 h-2.5" />
              {message.audioDuration}
            </span>
          )}
        </div>
        <p className={`text-[13px] leading-relaxed ${isAI ? "text-foreground" : "text-secondary-foreground"}`}>
          {message.text}
        </p>
      </div>
    </div>
  )
}

interface ChatPanelProps {
  connected: boolean
}

export function ChatPanel({ connected }: ChatPanelProps) {
  const [messages, setMessages] = useState<Message[]>(MOCK_MESSAGES)
  const [input, setInput] = useState("")
  const [isAutoScroll, setIsAutoScroll] = useState(true)
  const scrollRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)

  const scrollToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [])

  useEffect(() => {
    if (isAutoScroll) {
      scrollToBottom()
    }
  }, [messages, isAutoScroll, scrollToBottom])

  const handleScroll = () => {
    if (!scrollRef.current) return
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current
    const atBottom = scrollHeight - scrollTop - clientHeight < 40
    setIsAutoScroll(atBottom)
  }

  const handleSend = () => {
    if (!input.trim() || !connected) return
    const newMsg: Message = {
      id: Date.now().toString(),
      type: "user",
      text: input,
      timestamp: new Date().toLocaleTimeString("zh-CN", { hour12: false }),
    }
    setMessages((prev) => [...prev, newMsg])
    setInput("")
    setIsAutoScroll(true)
  }

  return (
    <div className="flex flex-col flex-1 min-w-0 bg-background">
      {/* Header */}
      <div className="flex items-center justify-between px-4 h-10 border-b border-border bg-card/50 shrink-0">
        <div className="flex items-center gap-2">
          <AudioLines className="w-3.5 h-3.5 text-primary" />
          <span className="text-xs font-medium text-foreground">会话记录</span>
          <span className="text-[10px] font-mono text-muted-foreground px-1.5 py-0.5 rounded bg-secondary">
            {messages.length} 条消息
          </span>
        </div>
        <div className="flex items-center gap-3 text-[10px] font-mono text-muted-foreground">
          {connected && (
            <>
              <span className="flex items-center gap-1">
                <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
                LIVE
              </span>
              <span>PCM 16kHz / Opus</span>
            </>
          )}
        </div>
      </div>

      {/* Messages */}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto py-3 min-h-0"
      >
        <div className="flex flex-col gap-1">
          {messages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}
          <div ref={bottomRef} />
        </div>
      </div>

      {/* Scroll to bottom */}
      {!isAutoScroll && (
        <div className="absolute bottom-16 right-4 z-10">
          <button
            onClick={() => {
              scrollToBottom()
              setIsAutoScroll(true)
            }}
            className="flex items-center gap-1 px-2.5 py-1.5 rounded-full bg-secondary border border-border text-xs text-muted-foreground hover:text-foreground transition-colors shadow-lg cursor-pointer"
          >
            <ChevronDown className="w-3 h-3" />
            新消息
          </button>
        </div>
      )}

      {/* Input */}
      <div className="flex items-center gap-2 px-4 py-3 border-t border-border bg-card/30 shrink-0">
        <div className="flex-1 flex items-center gap-2 px-3 py-2 rounded-lg bg-background border border-border focus-within:border-primary/70 focus-within:ring-1 focus-within:ring-primary/60 transition-[border-color,box-shadow]">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSend()}
            placeholder={connected ? "输入指令 (例: listen detect)" : "请先连接 WebSocket..."}
            readOnly={!connected}
            className="flex-1 bg-transparent text-xs text-foreground placeholder:text-muted-foreground/50 outline-none caret-primary read-only:opacity-70 font-mono"
          />
          <kbd className="hidden sm:block text-[9px] text-muted-foreground/40 font-mono px-1 py-0.5 rounded border border-border/50">
            Enter
          </kbd>
        </div>
        <button
          onClick={handleSend}
          disabled={!connected || !input.trim()}
          className="flex items-center justify-center w-8 h-8 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-30 disabled:cursor-not-allowed cursor-pointer"
        >
          <Send className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  )
}
