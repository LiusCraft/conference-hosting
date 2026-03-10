"use client"

import { useState, useEffect } from "react"
import { Menu, X, Monitor } from "lucide-react"
import Link from "next/link"

const navLinks = [
  { label: "架构概览", href: "#architecture" },
  { label: "协议兼容", href: "#protocol" },
  { label: "核心功能", href: "#features" },
  { label: "跨平台策略", href: "#platform" },
  { label: "性能优化", href: "#performance" },
  { label: "模块化设计", href: "#modularity" },
  { label: "路线规划", href: "#roadmap" },
]

export function Navigation() {
  const [scrolled, setScrolled] = useState(false)
  const [mobileOpen, setMobileOpen] = useState(false)

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 20)
    window.addEventListener("scroll", onScroll)
    return () => window.removeEventListener("scroll", onScroll)
  }, [])

  return (
    <header
      className={`fixed top-0 left-0 right-0 z-50 transition-all duration-300 ${
        scrolled
          ? "bg-background/80 backdrop-blur-xl border-b border-border"
          : "bg-transparent"
      }`}
    >
      <nav className="mx-auto flex max-w-7xl items-center justify-between px-6 py-4">
        <a href="#" className="flex items-center gap-3">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" className="text-primary-foreground">
              <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </div>
          <span className="text-lg font-semibold tracking-tight text-foreground">
            AI Meeting Host
          </span>
        </a>

        <div className="hidden items-center gap-1 md:flex">
          {navLinks.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
            >
              {link.label}
            </a>
          ))}
        </div>

        <div className="hidden items-center gap-3 md:flex">
          <Link
            href="/mockup"
            className="flex items-center gap-1.5 rounded-md px-3 py-2 text-sm text-primary transition-colors hover:text-primary/80"
          >
            <Monitor size={14} />
            UI 原型
          </Link>
          <a
            href="https://github.com/LiusCraft/conference-hosting"
            target="_blank"
            rel="noreferrer"
            className="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
          >
            GitHub 仓库
          </a>
          <a
            href="https://linx.qiniu.com/"
            target="_blank"
            rel="noreferrer"
            className="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
          >
            在线智能体
          </a>
          <a
            href="#roadmap"
            className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            查看路线图
          </a>
        </div>

        <button
          className="flex h-9 w-9 items-center justify-center rounded-md text-foreground md:hidden"
          onClick={() => setMobileOpen(!mobileOpen)}
          aria-label={mobileOpen ? "关闭菜单" : "打开菜单"}
        >
          {mobileOpen ? <X size={20} /> : <Menu size={20} />}
        </button>
      </nav>

      {mobileOpen && (
        <div className="border-t border-border bg-background/95 backdrop-blur-xl md:hidden">
          <div className="flex flex-col gap-1 px-6 py-4">
            {navLinks.map((link) => (
              <a
                key={link.href}
                href={link.href}
                onClick={() => setMobileOpen(false)}
                className="rounded-md px-3 py-2.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
              >
                {link.label}
              </a>
            ))}
            <div className="mt-3 border-t border-border pt-3 flex flex-col gap-2">
              <Link
                href="/mockup"
                className="flex items-center gap-1.5 rounded-lg border border-primary/30 bg-primary/10 px-4 py-2.5 text-center text-sm font-medium text-primary"
                onClick={() => setMobileOpen(false)}
              >
                <Monitor size={14} />
                查看 UI 原型
              </Link>
              <a
                href="https://github.com/LiusCraft/conference-hosting"
                target="_blank"
                rel="noreferrer"
                className="block rounded-lg border border-border bg-secondary px-4 py-2.5 text-center text-sm font-medium text-foreground"
                onClick={() => setMobileOpen(false)}
              >
                GitHub 仓库
              </a>
              <a
                href="https://linx.qiniu.com/"
                target="_blank"
                rel="noreferrer"
                className="block rounded-lg border border-border bg-secondary px-4 py-2.5 text-center text-sm font-medium text-foreground"
                onClick={() => setMobileOpen(false)}
              >
                获取在线智能体
              </a>
              <a
                href="#roadmap"
                className="block rounded-lg bg-primary px-4 py-2.5 text-center text-sm font-medium text-primary-foreground"
                onClick={() => setMobileOpen(false)}
              >
                查看路线图
              </a>
            </div>
          </div>
        </div>
      )}
    </header>
  )
}
