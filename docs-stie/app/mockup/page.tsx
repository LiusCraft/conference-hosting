import { DesktopAppMockup } from "@/components/desktop/desktop-app-mockup"
import Link from "next/link"
import { ArrowLeft } from "lucide-react"

export const metadata = {
  title: "AI Meeting Host - 桌面应用 UI 原型",
  description: "基于 Rust + GPUI 构建的 AI 会议托管工具桌面应用交互式界面原型",
}

export default function MockupPage() {
  return (
    <main className="min-h-screen bg-background text-foreground flex flex-col items-center justify-center p-4 md:p-8">
      {/* Back Link + Title */}
      <div className="flex flex-col items-center gap-4 mb-8 w-full max-w-[1200px]">
        <div className="flex items-center justify-between w-full">
          <Link
            href="/"
            className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            <ArrowLeft className="w-3.5 h-3.5" />
            返回产品概念
          </Link>
          <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-primary/10 border border-primary/20">
            <span className="w-2 h-2 rounded-full bg-primary animate-pulse" />
            <span className="text-[11px] font-mono text-primary tracking-wider">PROTOTYPE</span>
          </div>
        </div>

        <div className="flex flex-col items-center gap-2">
          <h1 className="text-2xl md:text-3xl font-semibold text-foreground text-balance text-center tracking-tight">
            AI Meeting Host
          </h1>
          <p className="text-sm text-muted-foreground text-center max-w-md text-pretty leading-relaxed">
            Rust + GPUI 桌面应用界面原型 -- 实时音频网关，
            连接在线会议与 AI 语音平台
          </p>
        </div>
      </div>

      {/* Desktop App Mockup */}
      <DesktopAppMockup />

      {/* Footer annotation */}
      <div className="flex flex-col items-center gap-2 mt-8">
        <div className="flex items-center gap-4 text-[10px] font-mono text-muted-foreground/50">
          <span>Rust + GPUI</span>
          <span className="w-1 h-1 rounded-full bg-border" />
          <span>Tokio async</span>
          <span className="w-1 h-1 rounded-full bg-border" />
          <span>cpal + Opus</span>
          <span className="w-1 h-1 rounded-full bg-border" />
          <span>macOS / Windows / Linux</span>
        </div>
        <p className="text-[10px] text-muted-foreground/40">
          交互式 UI 原型 -- 可点击侧边栏控件、设备选择器和连接按钮体验完整交互
        </p>
      </div>
    </main>
  )
}
