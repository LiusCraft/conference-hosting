"use client"

import type { ReactNode } from "react"

interface WindowChromeProps {
  children: ReactNode
  title?: string
}

export function WindowChrome({ children, title = "AI Meeting Host" }: WindowChromeProps) {
  return (
    <div className="flex flex-col rounded-xl border border-border overflow-hidden shadow-2xl shadow-black/50 bg-background max-w-[1200px] w-full mx-auto">
      {/* Title Bar */}
      <div className="flex items-center h-11 px-4 bg-secondary/60 border-b border-border shrink-0 select-none">
        {/* Traffic Lights */}
        <div className="flex items-center gap-2 mr-4">
          <div className="w-3 h-3 rounded-full bg-[#ff5f57] border border-[#e0443e]" />
          <div className="w-3 h-3 rounded-full bg-[#febc2e] border border-[#dea123]" />
          <div className="w-3 h-3 rounded-full bg-[#28c840] border border-[#1aab29]" />
        </div>

        {/* Window Title */}
        <div className="flex-1 text-center">
          <span className="text-xs font-medium text-muted-foreground tracking-wide">{title}</span>
        </div>

        {/* Right spacer to center title */}
        <div className="w-14" />
      </div>

      {/* Window Body */}
      <div className="flex flex-1 min-h-0">
        {children}
      </div>
    </div>
  )
}
