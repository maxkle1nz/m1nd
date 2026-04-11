import { motion } from "framer-motion";

const problems = [
  "agents rebuild repo structure from raw files every turn",
  "blast radius is still a guess when code, docs, and tests are disconnected",
  "context windows fill up before the agent reaches the real decision point",
  "investigations reset between sessions, so the same orientation tax gets paid again",
];

const solutions = [
  "one graph query returns structural truth instead of another round of file hunting",
  "change impact is surfaced before the first edit lands",
  "surgical context binds code, docs, and connected call paths into one operable slice",
  "continuity survives across sessions through trails, memory, audit, and runtime state",
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
              Most agent loops still start blind.
              <br />
              <span className="text-muted-foreground font-normal">m1nd exists to stop that.</span>
            </h2>
            <p className="text-xl text-muted-foreground max-w-2xl mx-auto">
              Search, read, search again, guess, edit, discover impact too late.
              <br />
              That loop was tolerable for humans.
              <br />
              It is expensive for agents.
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
              Stateless agent loop
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
              m1nd as first layer
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
          // the same task. a completely different substrate.
        </motion.p>
      </div>
    </section>
  );
}
