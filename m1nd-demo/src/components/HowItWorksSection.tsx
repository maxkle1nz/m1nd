import { motion } from "framer-motion";

const STEPS = [
  {
    n: "01",
    glyph: "⍂",
    glyphColor: "#00ff88",
    title: "Ingest",
    body: "m1nd ingests code, docs, and related structure into one live graph. The agent stops treating the repo like a pile of files and starts from connected system truth.",
    stat: "14 languages",
    statSub: "plus docs and memory lanes",
    statColor: "#00ff88",
    connector: true,
  },
  {
    n: "02",
    glyph: "⍌",
    glyphColor: "#00f5ff",
    title: "Orient",
    body: "The agent asks by structure and intent. m1nd returns what is connected, risky, and relevant before the model burns tokens reconstructing context from scratch.",
    stat: "543ns",
    statSub: "blast radius at depth = 3",
    statColor: "#00f5ff",
    connector: true,
  },
  {
    n: "03",
    glyph: "⍐",
    glyphColor: "#7b61ff",
    title: "Act",
    body: "The result comes back as operable context: connected nodes, blast radius, likely co-changes, and the exact slice the agent should verify before it edits.",
    stat: "84% fewer tokens",
    statSub: "vs grep and file wandering",
    statColor: "#7b61ff",
    connector: false,
  },
];

export function HowItWorksSection() {
  return (
    <section id="how-it-works" className="py-24 border-b border-border/20 relative overflow-hidden">
      <div
        className="absolute inset-0 pointer-events-none"
        style={{
          background:
            "radial-gradient(ellipse 70% 50% at 50% 50%, rgba(0,245,255,0.03) 0%, transparent 70%)",
        }}
      />

      <div className="container mx-auto px-4 lg:px-6 relative z-10">
        <motion.div
          className="text-center mb-16"
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.55 }}
        >
          <p className="font-mono text-[10px] text-muted-foreground/30 tracking-widest uppercase mb-3">
            how it works
          </p>
          <h2 className="text-3xl md:text-4xl font-bold tracking-tight">
            Ingest. Orient. Act.
          </h2>
          <p className="mt-3 text-muted-foreground/60 text-sm max-w-lg mx-auto font-mono">
            The point is not just speed. The point is giving the agent operational context before it changes the system.
          </p>
        </motion.div>

        <div className="relative flex flex-col md:flex-row items-start gap-0 md:gap-0">
          {STEPS.map((step, i) => (
            <div key={step.n} className="flex md:flex-col flex-row items-start md:items-start flex-1 relative">
              {/* Step card */}
              <motion.div
                className="flex-1 md:flex-none w-full px-4 md:px-6"
                initial={{ opacity: 0, y: 24 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true }}
                transition={{ duration: 0.5, delay: i * 0.12 }}
              >
                {/* Number + glyph row */}
                <div className="flex items-baseline gap-2 mb-4">
                  <span
                    className="font-mono text-6xl font-bold select-none leading-none"
                    style={{ color: "rgba(255,255,255,0.04)" }}
                  >
                    {step.n}
                  </span>
                  <span
                    className="font-mono text-xl leading-none"
                    style={{ color: step.glyphColor }}
                  >
                    {step.glyph}
                  </span>
                </div>

                {/* Title */}
                <h3
                  className="text-xl font-bold mb-3 tracking-tight"
                  style={{ color: step.glyphColor }}
                >
                  {step.title}
                </h3>

                {/* Body */}
                <p className="text-sm text-muted-foreground/65 leading-relaxed mb-5 font-mono max-w-[320px]">
                  {step.body}
                </p>

                {/* Stat callout */}
                <div
                  className="inline-flex flex-col px-3 py-2 rounded-lg border"
                  style={{
                    borderColor: `${step.statColor}25`,
                    background: `${step.statColor}08`,
                  }}
                >
                  <span
                    className="font-mono text-lg font-bold"
                    style={{ color: step.statColor }}
                  >
                    {step.stat}
                  </span>
                  <span className="font-mono text-[10px] text-muted-foreground/40 mt-0.5">
                    {step.statSub}
                  </span>
                </div>
              </motion.div>

              {/* Connector arrow (desktop: horizontal, mobile: vertical) */}
              {step.connector && (
                <motion.div
                  className="hidden md:flex absolute right-0 top-[2.4rem] translate-x-1/2 items-center pointer-events-none z-10"
                  initial={{ opacity: 0, scaleX: 0 }}
                  whileInView={{ opacity: 1, scaleX: 1 }}
                  viewport={{ once: true }}
                  transition={{ duration: 0.4, delay: i * 0.12 + 0.25 }}
                  style={{ transformOrigin: "left center" }}
                >
                  <div
                    className="h-px w-8"
                    style={{
                      background: `linear-gradient(90deg, ${step.glyphColor}40, ${STEPS[i + 1].glyphColor}40)`,
                    }}
                  />
                  <svg width="6" height="8" viewBox="0 0 6 8" fill="none" style={{ flexShrink: 0 }}>
                    <path d="M0 0L6 4L0 8V0Z" fill={STEPS[i + 1].glyphColor} opacity={0.4} />
                  </svg>
                </motion.div>
              )}

              {/* Mobile connector */}
              {step.connector && (
                <div className="md:hidden flex flex-col items-center w-8 pt-14 flex-shrink-0">
                  <div
                    className="w-px h-12"
                    style={{
                      background: `linear-gradient(180deg, ${step.glyphColor}40, ${STEPS[i + 1].glyphColor}40)`,
                    }}
                  />
                  <svg width="8" height="6" viewBox="0 0 8 6" fill="none">
                    <path d="M0 0H8L4 6H0V0Z" fill={STEPS[i + 1].glyphColor} opacity={0.4} />
                  </svg>
                </div>
              )}
            </div>
          ))}
        </div>

        {/* Bottom line */}
        <motion.div
          className="mt-14 flex items-center justify-center gap-4"
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ duration: 0.5, delay: 0.45 }}
        >
          <div className="h-px flex-1 bg-border/15 max-w-[80px]" />
          <span className="font-mono text-[10px] text-muted-foreground/25 tracking-widest uppercase">
            local-first · MCP-native · durable operational context
          </span>
          <div className="h-px flex-1 bg-border/15 max-w-[80px]" />
        </motion.div>
      </div>
    </section>
  );
}
