import { NavBar } from "@/components/NavBar";
import { Hero } from "@/components/Hero";
import { SEO } from "@/components/SEO";
import { SpeedSection } from "@/components/SpeedSection";
import { ProblemSection } from "@/components/ProblemSection";
import { OrientationFeature } from "@/components/OrientationFeature";
import { ImpactFeature } from "@/components/ImpactFeature";
import { MemoryFeature } from "@/components/MemoryFeature";
import { ReasoningFeature } from "@/components/ReasoningFeature";
import { SurgicalFeature } from "@/components/SurgicalFeature";
import { NavigationFeature } from "@/components/NavigationFeature";
import { WorkflowSection } from "@/components/WorkflowSection";
import { EcosystemSection } from "@/components/EcosystemSection";
import { InstallSection } from "@/components/InstallSection";
import { Footer } from "@/components/Footer";
import { LandingComparison } from "@/components/LandingComparison";
import { CompatibilityStrip } from "@/components/CompatibilityStrip";
import { HowItWorksSection } from "@/components/HowItWorksSection";
import { FAQSection } from "@/components/FAQSection";

const METRICS = [
  { value: "1.36µs", label: "graph activation", sub: "1,000 nodes" },
  { value: "84%",    label: "fewer tokens",      sub: "vs grep/cat" },
  { value: "543ns",  label: "blast radius",      sub: "depth=3" },
  { value: "0.18s",  label: "full code analysis", sub: "end-to-end" },
];

function MetricsStrip() {
  return (
    <div className="border-t border-b border-border/20 bg-background">
      <div className="container mx-auto px-6 py-5">
        <div className="flex flex-wrap items-center justify-center gap-x-10 gap-y-4 md:gap-x-20">
          {METRICS.map(({ value, label, sub }) => (
            <div key={label} className="flex flex-col items-center gap-0.5 min-w-[90px]">
              <span className="text-2xl font-bold font-mono text-primary tabular-nums">{value}</span>
              <span className="text-[11px] font-mono text-muted-foreground/70 uppercase tracking-wider">{label}</span>
              <span className="text-[10px] font-mono text-muted-foreground/35">{sub}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default function Landing() {
  return (
    <main className="w-full min-h-screen bg-background">
      <SEO
        title="m1nd — Graph Intelligence for MCP AI Agents"
        description="m1nd gives AI agents an in-memory knowledge graph of your codebase, research, and memory. Activate 1,000 nodes in 1.36µs. 84% fewer tokens than grep. Built for MCP, runs in Rust."
        canonicalPath="/"
      />
      <NavBar />
      <Hero />
      <SpeedSection />
      <MetricsStrip />
      <CompatibilityStrip />
      <HowItWorksSection />
      <LandingComparison />
      <ProblemSection />
      <OrientationFeature />
      <ImpactFeature />
      <MemoryFeature />
      <ReasoningFeature />
      <SurgicalFeature />
      <NavigationFeature />
      <WorkflowSection />
      <InstallSection />
      <EcosystemSection />
      <FAQSection />
      <Footer />
    </main>
  );
}
