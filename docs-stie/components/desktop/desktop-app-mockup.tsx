"use client"

import { useState, useEffect, useCallback } from "react"
import { WindowChrome } from "./window-chrome"
import { Sidebar } from "./sidebar"
import { ChatPanel } from "./chat-panel"
import { StatusBar } from "./status-bar"
import { SettingsPanel } from "./settings-panel"

export function DesktopAppMockup() {
  const [connected, setConnected] = useState(true)
  const [micActive, setMicActive] = useState(true)
  const [speakerActive, setSpeakerActive] = useState(true)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [inputLevel, setInputLevel] = useState(0)
  const [outputLevel, setOutputLevel] = useState(0)

  // Simulate audio levels
  const generateLevel = useCallback((base: number, variance: number) => {
    return Math.max(0, Math.min(100, base + (Math.random() - 0.5) * variance))
  }, [])

  useEffect(() => {
    if (!connected) {
      setInputLevel(0)
      setOutputLevel(0)
      return
    }

    let inputBase = 35
    let outputBase = 25

    const interval = setInterval(() => {
      if (micActive) {
        // Simulate conversation patterns: occasional spikes
        const spike = Math.random() > 0.85 ? 30 : 0
        inputBase = inputBase * 0.9 + (35 + spike) * 0.1
        setInputLevel(generateLevel(inputBase, 20))
      } else {
        setInputLevel(0)
      }

      if (speakerActive) {
        const spike = Math.random() > 0.8 ? 40 : 0
        outputBase = outputBase * 0.9 + (25 + spike) * 0.1
        setOutputLevel(generateLevel(outputBase, 15))
      } else {
        setOutputLevel(0)
      }
    }, 80)

    return () => clearInterval(interval)
  }, [connected, micActive, speakerActive, generateLevel])

  return (
    <div className="relative flex flex-col w-full" style={{ height: "680px" }}>
      <WindowChrome title="AI Meeting Host v0.1.0-alpha">
        <div className="flex flex-col flex-1 min-h-0">
          <div className="flex flex-1 min-h-0 relative">
            <Sidebar
              connected={connected}
              onToggleConnect={() => setConnected((c) => !c)}
              onOpenSettings={() => setSettingsOpen(true)}
              micActive={micActive}
              onToggleMic={() => setMicActive((m) => !m)}
              speakerActive={speakerActive}
              onToggleSpeaker={() => setSpeakerActive((s) => !s)}
              inputLevel={inputLevel}
              outputLevel={outputLevel}
            />
            <ChatPanel connected={connected} />
            <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} />
          </div>
          <StatusBar connected={connected} />
        </div>
      </WindowChrome>
    </div>
  )
}
