import { motion } from "framer-motion";

const MODELS = [
  { name: "Claude Opus 4.6",    maker: "Anthropic", color: "#CC9B7A", bg: "#CC9B7A14" },
  { name: "Claude Sonnet 4.x",  maker: "Anthropic", color: "#CC9B7A", bg: "#CC9B7A14" },
  { name: "GPT-5.4 Thinking",   maker: "OpenAI",    color: "#10A37F", bg: "#10A37F14" },
  { name: "GPT-5.3 Instant",    maker: "OpenAI",    color: "#10A37F", bg: "#10A37F14" },
  { name: "GPT-5.4 mini",       maker: "OpenAI",    color: "#10A37F", bg: "#10A37F14" },
  { name: "Gemini 2.5 Pro",     maker: "Google",    color: "#4285F4", bg: "#4285F414" },
  { name: "Llama 4",            maker: "Meta",       color: "#0668E1", bg: "#0668E114" },
  { name: "any MCP model",      maker: "",           color: "#00f5ff", bg: "#00f5ff10", isWild: true },
];

const CLIENTS = [
  { name: "Claude Code",    color: "#CC9B7A", bg: "#CC9B7A14" },
  { name: "Claude Desktop", color: "#CC9B7A", bg: "#CC9B7A14" },
  { name: "Cursor",         color: "#7b61ff", bg: "#7b61ff14" },
  { name: "Windsurf",       color: "#00cba9", bg: "#00cba914" },
  { name: "VS Code",        color: "#007ACC", bg: "#007ACC14" },
  { name: "ChatGPT",        color: "#10A37F", bg: "#10A37F14" },
  { name: "Cline",          color: "#00ff88", bg: "#00ff8814" },
  { name: "Continue",       color: "#5B8AF5", bg: "#5B8AF514" },
  { name: "Zed",            color: "#9ca3af", bg: "#9ca3af14" },
];

function Pill({
  name, color, bg, isWild, delay,
}: {
  name: string; color: string; bg: string; isWild?: boolean; delay: number;
}) {
  return (
    <motion.span
      initial={{ opacity: 0, scale: 0.88 }}
      whileInView={{ opacity: 1, scale: 1 }}
      viewport={{ once: true }}
      transition={{ duration: 0.3, delay }}
      className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border font-mono text-[11px] select-none whitespace-nowrap"
      style={{
        borderColor: `${color}30`,
        background: bg,
        color,
        borderStyle: isWild ? "dashed" : "solid",
      }}
    >
      {isWild && (
        <span style={{ fontSize: 9, opacity: 0.7 }}>⟁</span>
      )}
      {name}
    </motion.span>
  );
}

export function CompatibilityStrip() {
  return (
    <section className="py-10 border-b border-border/20 relative overflow-hidden">
      <div
        className="absolute inset-0 pointer-events-none"
        style={{ background: "linear-gradient(180deg, rgba(0,245,255,0.025) 0%, transparent 100%)" }}
      />
      <div className="container mx-auto px-4 lg:px-6 relative z-10">

        <motion.div
          className="flex items-center justify-center gap-3 mb-5"
          initial={{ opacity: 0, y: 8 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.5 }}
        >
          <div className="h-px flex-1 bg-border/20 max-w-[120px]" />
          <span className="font-mono text-[10px] text-muted-foreground/40 tracking-widest uppercase">
            works with every model · every client · april 2026
          </span>
          <div className="h-px flex-1 bg-border/20 max-w-[120px]" />
        </motion.div>

        <div className="flex flex-col gap-3">
          {/* Models row */}
          <div className="flex flex-wrap justify-center gap-1.5">
            <motion.span
              initial={{ opacity: 0 }}
              whileInView={{ opacity: 1 }}
              viewport={{ once: true }}
              transition={{ duration: 0.4 }}
              className="font-mono text-[9px] text-muted-foreground/30 uppercase tracking-widest self-center mr-1"
            >
              models
            </motion.span>
            {MODELS.map((m, i) => (
              <Pill
                key={m.name}
                name={m.name}
                color={m.color}
                bg={m.bg}
                isWild={m.isWild}
                delay={i * 0.04}
              />
            ))}
          </div>

          {/* Clients row */}
          <div className="flex flex-wrap justify-center gap-1.5">
            <motion.span
              initial={{ opacity: 0 }}
              whileInView={{ opacity: 1 }}
              viewport={{ once: true }}
              transition={{ duration: 0.4, delay: 0.1 }}
              className="font-mono text-[9px] text-muted-foreground/30 uppercase tracking-widest self-center mr-1"
            >
              clients
            </motion.span>
            {CLIENTS.map((c, i) => (
              <Pill
                key={c.name}
                name={c.name}
                color={c.color}
                bg={c.bg}
                delay={0.15 + i * 0.04}
              />
            ))}
          </div>
        </div>

        <motion.p
          className="text-center mt-4 font-mono text-[10px] text-muted-foreground/25"
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ duration: 0.5, delay: 0.5 }}
        >
          MCP is the open standard. Any agent that speaks it calls m1nd the same way.
        </motion.p>

      </div>
    </section>
  );
}
