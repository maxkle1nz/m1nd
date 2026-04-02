import { motion } from "framer-motion";

const languages = [
  { name: "Python",     color: "#3776AB", bg: "#3776AB18", icon: "🐍" },
  { name: "TypeScript", color: "#3178C6", bg: "#3178C618", icon: "𝙏𝙎" },
  { name: "JavaScript", color: "#F7DF1E", bg: "#F7DF1E18", icon: "𝙅𝙎" },
  { name: "Rust",       color: "#CE4A08", bg: "#CE4A0818", icon: "⚙" },
  { name: "Go",         color: "#00ADD8", bg: "#00ADD818", icon: "◈" },
  { name: "Java",       color: "#ED8B00", bg: "#ED8B0018", icon: "☕" },
  { name: "C / C++",   color: "#00599C", bg: "#00599C18", icon: "⚡" },
  { name: "C#",        color: "#68217A", bg: "#68217A18", icon: "#" },
  { name: "Kotlin",    color: "#7F52FF", bg: "#7F52FF18", icon: "𝙆" },
  { name: "Ruby",      color: "#CC342D", bg: "#CC342D18", icon: "◆" },
  { name: "PHP",       color: "#777BB4", bg: "#777BB418", icon: "𝙃" },
  { name: "Swift",     color: "#FA7343", bg: "#FA734318", icon: "◀" },
  { name: "Bash",      color: "#4EAA25", bg: "#4EAA2518", icon: "$" },
  { name: "SQL",       color: "#336791", bg: "#33679118", icon: "⛁" },
];

const formats = [
  { name: "PDF",      color: "#E44D26", bg: "#E44D2618", icon: "📄" },
  { name: "Markdown", color: "#6e8efb", bg: "#6e8efb18", icon: "𝙈↓" },
  { name: "Jupyter",  color: "#F37626", bg: "#F3762618", icon: "◎" },
  { name: "HTML",     color: "#E44D26", bg: "#E44D2618", icon: "⟨⟩" },
  { name: "YAML",     color: "#CB171E", bg: "#CB171E18", icon: "—" },
  { name: "JSON",     color: "#F0DB4F", bg: "#F0DB4F18", icon: "{}" },
  { name: "TOML",     color: "#9C4221", bg: "#9C422118", icon: "⊞" },
  { name: "RST",      color: "#4A90D9", bg: "#4A90D918", icon: "∷" },
  { name: "CSV",      color: "#16A34A", bg: "#16A34A18", icon: "⊟" },
  { name: "arXiv",    color: "#B31B1B", bg: "#B31B1B18", icon: "∂" },
  { name: "DOI",      color: "#5B616B", bg: "#5B616B18", icon: "○" },
  { name: ".ipynb",   color: "#DA5B0B", bg: "#DA5B0B18", icon: "◉" },
];

const memory = [
  { name: "Articles",      color: "#06B6D4", bg: "#06B6D418", icon: "◈" },
  { name: "Memories",      color: "#8B5CF6", bg: "#8B5CF618", icon: "◉" },
  { name: "Wikis",         color: "#10B981", bg: "#10B98118", icon: "⊡" },
  { name: "Issues",        color: "#F59E0B", bg: "#F59E0B18", icon: "◆" },
  { name: "Conversations", color: "#EC4899", bg: "#EC489918", icon: "◎" },
  { name: "Slides",        color: "#6366F1", bg: "#6366F118", icon: "□" },
  { name: "RFCs",          color: "#14B8A6", bg: "#14B8A618", icon: "∮" },
  { name: "Patents",       color: "#A78BFA", bg: "#A78BFA18", icon: "®" },
];

function Chip({ name, color, bg, icon, delay }: { name: string; color: string; bg: string; icon: string; delay: number }) {
  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.9 }}
      whileInView={{ opacity: 1, scale: 1 }}
      viewport={{ once: true }}
      transition={{ duration: 0.35, delay }}
      className="flex items-center gap-1.5 px-3 py-1.5 rounded-full border text-xs font-mono font-medium select-none"
      style={{ borderColor: `${color}35`, background: bg, color }}
    >
      <span className="text-[11px] leading-none" style={{ fontFamily: "monospace" }}>{icon}</span>
      <span>{name}</span>
    </motion.div>
  );
}

export function EcosystemSection() {
  return (
    <section className="py-20 border-b border-border/20 relative overflow-hidden">
      <div className="absolute inset-0 pointer-events-none" style={{ background: "radial-gradient(ellipse at 50% 50%, rgba(0,245,255,0.03), transparent 65%)" }} />
      <div className="container mx-auto px-6 relative z-10">
        <motion.div
          className="text-center mb-12"
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6 }}
        >
          <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-5">
            supported everywhere
          </div>
          <h2 className="text-2xl md:text-4xl font-bold font-sans tracking-tight mb-3">
            One graph. Code, research, and memory.
          </h2>
          <p className="text-muted-foreground max-w-xl mx-auto text-base">
            m1nd indexes code. l1ght reads papers, articles, memories, and conversations — and merges them into the same queryable graph. One query traverses all of it.
          </p>
        </motion.div>

        <div className="space-y-6">
          <div>
            <p className="text-[10px] font-mono text-muted-foreground/40 uppercase tracking-widest mb-3 text-center">
              code — 14 languages
            </p>
            <div className="flex flex-wrap justify-center gap-2">
              {languages.map((l, i) => (
                <Chip key={l.name} {...l} delay={i * 0.03} />
              ))}
            </div>
          </div>

          <div className="h-px bg-border/20 mx-auto max-w-xl" />

          <div>
            <p className="text-[10px] font-mono text-muted-foreground/40 uppercase tracking-widest mb-3 text-center">
              documents &amp; research — l1ght
            </p>
            <div className="flex flex-wrap justify-center gap-2">
              {formats.map((f, i) => (
                <Chip key={f.name} {...f} delay={i * 0.03 + 0.3} />
              ))}
            </div>
          </div>

          <div className="h-px bg-border/20 mx-auto max-w-xl" />

          <div>
            <p className="text-[10px] font-mono text-muted-foreground/40 uppercase tracking-widest mb-3 text-center">
              memory &amp; context — l1ght
            </p>
            <div className="flex flex-wrap justify-center gap-2">
              {memory.map((m, i) => (
                <Chip key={m.name} {...m} delay={i * 0.03 + 0.6} />
              ))}
            </div>
          </div>
        </div>

        <motion.p
          className="text-center mt-10 text-xs font-mono text-muted-foreground/30"
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ delay: 0.8 }}
        >
          Code, papers, articles, memories, and conversations — one graph, one query.
        </motion.p>
      </div>
    </section>
  );
}
