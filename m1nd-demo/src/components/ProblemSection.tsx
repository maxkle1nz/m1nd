import { motion } from "framer-motion";

const problems = [
  "grep loops that read every file to find one function",
  "blast radius is a guess until something breaks in production",
  "context window flooded with irrelevant code",
  "every session starts from scratch — no memory of what was found",
];

const solutions = [
  "one graph query — authority found in subseconds, no files opened",
  "blast radius computed before the first edit is written",
  "surgical context — only the nodes that matter, nothing else",
  "investigation trails persist across every session",
];

export function ProblemSection() {
  return (
    <section id="problem" className="py-32 relative bg-background border-t border-border/50">
      <div className="container mx-auto px-6 relative z-10">
        <div className="max-w-4xl mx-auto text-center mb-20">
          <motion.div
            initial={{ opacity: 0, y: 16 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.6 }}
          >
            <h2 className="text-3xl md:text-5xl font-bold font-sans tracking-tight mb-6">
              grep was built for humans.
              <br />
              <span className="text-muted-foreground font-normal">your agent is paying the price.</span>
            </h2>
            <p className="text-xl text-muted-foreground max-w-2xl mx-auto">
              30-year-old tools. file-by-file reads.
              <br />
              tokens burned for no reason.
              <br />
              <br />
              tokens = money.
              <br />
              waste = exponential.
              <br />
              <br />
              and it starts on day one.
            </p>
          </motion.div>
        </div>

        <div className="grid md:grid-cols-2 gap-8 max-w-5xl mx-auto">
          <motion.div
            initial={{ opacity: 0, x: -20 }}
            whileInView={{ opacity: 1, x: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.6 }}
            className="p-8 rounded-xl border border-destructive/20 bg-destructive/5 relative overflow-hidden group"
          >
            <div className="absolute inset-0 bg-gradient-to-br from-destructive/10 to-transparent opacity-0 group-hover:opacity-100 transition-opacity" />
            <h3 className="text-lg font-mono font-semibold mb-6 text-destructive/80 tracking-wide uppercase">
              Terminal-era tools
            </h3>
            <ul className="space-y-5 text-muted-foreground">
              {problems.map((p, i) => (
                <li key={i} className="flex items-start gap-3 text-sm leading-relaxed">
                  <span className="text-destructive font-bold mt-0.5 flex-shrink-0">×</span>
                  {p}
                </li>
              ))}
            </ul>
          </motion.div>

          <motion.div
            initial={{ opacity: 0, x: 20 }}
            whileInView={{ opacity: 1, x: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.6, delay: 0.1 }}
            className="p-8 rounded-xl border border-primary/30 bg-primary/5 relative overflow-hidden group shadow-[0_0_30px_rgba(0,245,255,0.05)]"
          >
            <div className="absolute inset-0 bg-gradient-to-br from-primary/10 to-transparent opacity-0 group-hover:opacity-100 transition-opacity" />
            <h3 className="text-lg font-mono font-semibold mb-6 text-primary/80 tracking-wide uppercase">
              m1nd — agent-native
            </h3>
            <ul className="space-y-5 text-muted-foreground">
              {solutions.map((s, i) => (
                <li key={i} className="flex items-start gap-3 text-sm leading-relaxed">
                  <span className="text-primary font-bold mt-0.5 flex-shrink-0">✓</span>
                  {s}
                </li>
              ))}
            </ul>
          </motion.div>
        </div>

        <motion.p
          className="text-center mt-12 font-mono text-sm text-muted-foreground/60"
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ duration: 0.8, delay: 0.4 }}
        >
          // the same information. a completely different substrate.
        </motion.p>
      </div>
    </section>
  );
}
