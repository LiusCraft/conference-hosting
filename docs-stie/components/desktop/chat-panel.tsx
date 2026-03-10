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
  { id: "2", type: "system", text: "hello 握手成功 | session_id=session-001 | features.mcp=true", timestamp: "14:23:01" },
  { id: "3", type: "system", text: "音频路由已启用: input=loopback:BlackHole 2ch, output=BlackHole 2ch", timestamp: "14:23:02" },
  { id: "4", type: "system", text: "AEC enabled (AEC3 real-time, 16kHz mono)", timestamp: "14:23:02" },
  { id: "5", type: "stt", text: "请帮我汇总今天下午的会议安排。", timestamp: "14:23:10" },
  { id: "6", type: "tool", text: "mcp.tools/list -> tools=[calendar.get_events, note.create, task.add]", timestamp: "14:23:10" },
  { id: "7", type: "tool", text: "mcp.tools/call calendar.get_events({\"range\":\"today_afternoon\"})", timestamp: "14:23:11" },
  { id: "8", type: "ai", text: "今天下午共有 3 场会议：14:00 产品评审、15:30 研发同步、17:00 客户复盘。需要我生成简要提醒吗？", timestamp: "14:23:13", audioDuration: "6.8s" },
  { id: "9", type: "stt", text: "好的，帮我把研发同步会议加一个会前 10 分钟提醒。", timestamp: "14:23:25" },
  { id: "10", type: "tool", text: "mcp.tools/call task.add({\"title\":\"研发同步会前提醒\",\"at\":\"15:20\"})", timestamp: "14:23:26" },
  { id: "11", type: "ai", text: "已创建提醒：15:20 通知你准备参加研发同步会议。", timestamp: "14:23:27", audioDuration: "3.1s" },
  { id: "12", type: "system", text: "intent_trace: 2 steps | mcp route=calendar.get_events -> task.add", timestamp: "14:23:27" },
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
