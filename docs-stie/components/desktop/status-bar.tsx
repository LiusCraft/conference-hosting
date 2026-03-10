"use client"

import { Clock, Cpu, HardDrive, Zap } from "lucide-react"
import { useState, useEffect } from "react"

interface StatusBarProps {
  connected: boolean
}

export function StatusBar({ connected }: StatusBarProps) {
  const [time, setTime] = useState("")

  useEffect(() => {
    const update = () => {
      setTime(new Date().toLocaleTimeString("zh-CN", { hour12: false }))
    }
    update()
    const interval = setInterval(update, 1000)
    return () => clearInterval(interval)
  }, [])

  return (
    <div className="flex items-center justify-between h-6 px-4 bg-secondary/40 border-t border-border shrink-0 select-none">
      <div className="flex items-center gap-4 text-[10px] font-mono text-muted-foreground">
        <span className="flex items-center gap-1">
          <Cpu className="w-3 h-3" />
          CPU 2.3%
        </span>
        <span className="flex items-center gap-1">
          <HardDrive className="w-3 h-3" />
          RAM 48 MB
        </span>
        {connected && (
          <span className="flex items-center gap-1 text-primary">
            <Zap className="w-3 h-3" />
            RTT 48ms | AEC 118ms | 20ms 帧
          </span>
        )}
      </div>
      <div className="flex items-center gap-1 text-[10px] font-mono text-muted-foreground">
        <Clock className="w-3 h-3" />
        {time}
      </div>
    </div>
  )
}
