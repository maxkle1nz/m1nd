import { useRef } from "react";
import { motion, useInView } from "framer-motion";

const speedStats = [
  {
    value: "1.36µs",
    label: "activate 1K nodes",
    sublabel: "in-memory Rust — no GC, no pauses",
    color: "#00f5ff",
    detail: "pure Rust binary. 0 LLM tokens consumed.",
  },
  {
    value: "543ns",
    label: "blast radius",
    sublabel: "impact depth=3, nanosecond scale",
    color: "#00ff88",
    detail: "computed before your first edit is typed.",
  },
  {
    value: "0",
    label: "files opened",
    sublabel: "pre-indexed graph — no file reads, ever",
    color: "#ffb700",
    detail: "fewer tokens. lower costs. every query.",
  },
  {
    value: "84%",
    label: "token savings",
    sublabel: "vs grep-based navigation workflows",
    color: "#ff00aa",
    detail: "46 m1nd queries vs ~210 grep ops. 3.1s vs 35min.",
  },
];

function ScanLine() {
  return (
    <div className="absolute inset-0 overflow-hidden pointer-events-none">
      <motion.div
        className="absolute left-0 right-0 h-px"
        style={{ background: "linear-gradient(90deg, transparent, #00f5ff44, transparent)" }}
        animate={{ top: ["0%", "100%"] }}
        transition={{ duration: 3.5, repeat: Infinity, ease: "linear" }}
      />
    </div>
  );
}

function StatCard({
  value,
  label,
  sublabel,
  detail,
  color,
  index,
}: {
  value: string;
  label: string;
  sublabel: string;
  detail: string;
  color: string;
  index: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { once: true });

  return (
    <motion.div
      ref={ref}
      initial={{ opacity: 0, y: 24 }}
      animate={inView ? { opacity: 1, y: 0 } : {}}
      transition={{ duration: 0.6, delay: index * 0.12, ease: "easeOut" }}
      className="flex flex-col gap-2 p-6 rounded-xl border relative overflow-hidden group"
      style={{ borderColor: `${color}28`, background: "rgba(5,5,22,0.7)" }}
    >
      <div
        className="absolute inset-0 opacity-0 group-hover:opacity-100 transition-opacity duration-500"
        style={{ background: `radial-gradient(ellipse at top left, ${color}12, transparent 70%)` }}
      />
      <div
        className="absolute inset-0 pointer-events-none"
        style={{ background: `radial-gradient(ellipse at top left, ${color}08, transparent 60%)` }}
      />
      <div
        className="text-5xl md:text-6xl font-bold font-mono tracking-tighter relative z-10"
        style={{ color, textShadow: `0 0 20px ${color}66` }}
      >
        {value}
      </div>
      <div className="relative z-10">
        <p className="text-lg font-semibold text-foreground">{label}</p>
        <p className="text-sm text-muted-foreground mt-0.5">{sublabel}</p>
      </div>
      <div className="text-xs font-mono mt-2 relative z-10" style={{ color: `${color}99` }}>
        // {detail}
      </div>
      <div
        className="absolute bottom-0 left-0 h-[2px]"
        style={{ width: "100%", background: `linear-gradient(90deg, ${color}88, transparent)` }}
      />
    </motion.div>
  );
}

export function SpeedSection() {
  const sectionRef = useRef<HTMLElement>(null);
  const inView = useInView(sectionRef, { once: true, margin: "-80px" });

  return (
    <section
      ref={sectionRef}
      className="relative py-28 border-t border-border/40 overflow-hidden"
      style={{ background: "rgba(0,0,0,0.3)" }}
    >
      <ScanLine />
      <div
        className="absolute inset-0 pointer-events-none"
        style={{
          background: "radial-gradient(ellipse at 50% 0%, rgba(0,245,255,0.07) 0%, transparent 65%)",
        }}
      />

      <div className="container mx-auto px-6 relative z-10">
        <motion.div
          className="text-center mb-16"
          initial={{ opacity: 0, y: 20 }}
          animate={inView ? { opacity: 1, y: 0 } : {}}
          transition={{ duration: 0.8 }}
        >
          <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-6">
            performance
          </div>

          <blockquote
            className="text-2xl md:text-4xl lg:text-5xl font-bold font-sans tracking-tight leading-tight mb-6 max-w-4xl mx-auto"
            style={{
              background: "linear-gradient(135deg, #00f5ff, #00ff88, #ffb700)",
              WebkitBackgroundClip: "text",
              WebkitTextFillColor: "transparent",
              backgroundClip: "text",
            }}
          >
            m1nd finds the cut before the model even finishes reading.
          </blockquote>

          <p className="text-muted-foreground text-lg max-w-2xl mx-auto">
            Pure Rust. In-memory graph. Zero LLM tokens spent on navigation.
            <br className="hidden md:block" />
            The graph already knows what's up. Your agent just asks.
          </p>
        </motion.div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-5">
          {speedStats.map((stat, i) => (
            <StatCard key={stat.label} {...stat} index={i} />
          ))}
        </div>

        <motion.div
          className="mt-12 flex flex-col items-center gap-4 text-center"
          initial={{ opacity: 0 }}
          animate={inView ? { opacity: 1 } : {}}
          transition={{ duration: 1, delay: 0.8 }}
        >
          <div
            className="inline-block font-mono text-xs text-muted-foreground border border-border/30 rounded px-4 py-2"
            style={{ background: "rgba(0,245,255,0.03)" }}
          >
            grep reads files &nbsp;·&nbsp; m1nd reads the graph
          </div>
          <p className="text-sm text-muted-foreground/70 max-w-sm">
            Less file waste. Instant connection to anywhere in the code.
            <br />
            <span className="text-primary/80 font-medium">Saves time. Saves money.</span>
          </p>
        </motion.div>
      </div>
    </section>
  );
}
