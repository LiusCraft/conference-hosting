import { Navigation } from "@/components/navigation"
import { HeroSection } from "@/components/hero-section"
import { ArchitectureSection } from "@/components/architecture-section"
import { ProtocolSection } from "@/components/protocol-section"
import { FeaturesSection } from "@/components/features-section"
import { DesignPrinciplesSection } from "@/components/design-principles-section"
import { PlatformSection } from "@/components/platform-section"
import { PerformanceSection } from "@/components/performance-section"
import { ModularitySection } from "@/components/modularity-section"
import { RoadmapSection } from "@/components/roadmap-section"
import { Footer } from "@/components/footer"

export default function Page() {
  return (
    <main className="min-h-screen bg-background text-foreground">
      <Navigation />
      <HeroSection />
      <ArchitectureSection />
      <ProtocolSection />
      <FeaturesSection />
      <DesignPrinciplesSection />
      <PlatformSection />
      <PerformanceSection />
      <ModularitySection />
      <RoadmapSection />
      <Footer />
    </main>
  )
}
