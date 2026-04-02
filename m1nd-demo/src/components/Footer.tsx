import { Button } from "@/components/ui/button";
import { M1ndInline } from "@/components/M1ndWordmark";

export function Footer() {
  return (
    <footer className="py-32 relative bg-background border-t border-border/50 overflow-hidden text-center">
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_bottom,rgba(0,245,255,0.1)_0%,transparent_60%)]" />

      <div className="container mx-auto px-6 relative z-10 flex flex-col items-center">
        <p className="font-mono text-xs tracking-widest uppercase mb-4 flex items-center justify-center gap-2">
          <span style={{ color: "#00f5ff", opacity: 0.6 }}>⍌</span>
          <span style={{ color: "#7b61ff", opacity: 0.6 }}>⍐</span>
          <span style={{ color: "#00ff88", opacity: 0.6 }}>⍂</span>
          <span style={{ color: "#ffb700", opacity: 0.6 }}>𝔻</span>
          <span style={{ color: "#ff00aa", opacity: 0.6 }}>⟁</span>
          <span className="text-primary/30 mx-2">·</span>
          <span className="text-primary/40">m1nd + l1ght — one graph</span>
        </p>
        <h2 className="text-4xl md:text-6xl font-bold font-sans tracking-tight mb-4">
          Start building with <M1ndInline glow />.
        </h2>
        <p className="text-muted-foreground max-w-md mx-auto mb-10 text-lg">
          Code, research, and memory — all in one graph. One install. One query.
        </p>

        <div className="flex flex-col sm:flex-row items-center gap-4 mb-24">
          <a href="https://github.com/maxkle1nz/m1nd" target="_blank" rel="noreferrer" className="w-full sm:w-auto">
            <Button size="lg" className="w-full bg-primary text-primary-foreground hover:bg-primary/90 text-lg px-8 shadow-[0_0_20px_rgba(0,245,255,0.3)]">
              Get m1nd
            </Button>
          </a>
          <a href="https://m1nd.world/wiki/" target="_blank" rel="noreferrer" className="w-full sm:w-auto">
            <Button size="lg" variant="outline" className="w-full border-primary/20 text-primary hover:bg-primary/10 text-lg px-8">
              Read the Wiki
            </Button>
          </a>
        </div>

        {/* Community row */}
        <div className="flex flex-wrap justify-center gap-3 mb-10">
          {[
            { label: "GitHub",             href: "https://github.com/maxkle1nz/m1nd",                      color: "#00f5ff" },
            { label: "GitHub Discussions", href: "https://github.com/maxkle1nz/m1nd/discussions",          color: "#00f5ff" },
            { label: "Report an Issue",    href: "https://github.com/maxkle1nz/m1nd/issues",               color: "#ff00aa" },
            { label: "Wiki",               href: "https://m1nd.world/wiki/",                               color: "#7b61ff" },
          ].map(({ label, href, color }) => (
            <a
              key={label}
              href={href}
              target="_blank"
              rel="noreferrer"
              className="text-[11px] font-mono px-3 py-1.5 rounded border transition-all hover:opacity-90"
              style={{
                color: `${color}99`,
                borderColor: `${color}20`,
                background: `${color}08`,
              }}
            >
              {label} ↗
            </a>
          ))}
        </div>

        <div className="flex flex-col sm:flex-row justify-between items-center gap-4 w-full max-w-4xl border-t border-border/30 pt-8 text-sm text-muted-foreground">
          <p className="font-mono text-xs" style={{ color: "#ffffff30" }}>
            © {new Date().getFullYear()} m1nd ·{" "}
            <span style={{ color: "#00ff8860" }}>MIT License</span>
            {" "}· v0.6.1
          </p>
          <div className="flex items-center gap-6 text-xs font-mono">
            <a href="https://m1nd.world/wiki/" target="_blank" rel="noreferrer" className="text-primary/40 hover:text-primary/70 transition-colors tracking-widest uppercase">
              Wiki
            </a>
            <a href="/l1ght" className="hover:opacity-80 transition-opacity tracking-widest uppercase"
              style={{ color: "#ffb70055" }}>
              l1ght
            </a>
            <a href="/use-cases" className="text-primary/40 hover:text-primary/70 transition-colors tracking-widest uppercase">
              Use Cases
            </a>
            <a href="/demo" className="text-primary/40 hover:text-primary/70 transition-colors tracking-widest uppercase">
              Demo
            </a>
          </div>
        </div>
      </div>
    </footer>
  );
}
