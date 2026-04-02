import { motion } from "framer-motion";

const steps = [
  {
    id: "ingest",
    title: "Index",
    desc: "The codebase is parsed into a live graph. Every node, edge, and import path indexed — once.",
  },
  {
    id: "seek",
    title: "Query",
    desc: "The agent fires a single graph query. Authority found in subseconds. No file reading.",
  },
  {
    id: "impact",
    title: "Blast Radius",
    desc: "Impact cone computed before any edit begins. Every dependent node surfaced instantly.",
  },
  {
    id: "context",
    title: "Surgical Context",
    desc: "Target isolated with callers and callees. Only what matters enters the context window.",
  },
  {
    id: "apply",
    title: "Edit",
    desc: "Precision changes applied. Graph re-indexed. No structural surprises on the next query.",
  },
  {
    id: "report",
    title: "Remember",
    desc: "The investigation trail is saved. Next session resumes where this one left off.",
  },
];

export function WorkflowSection() {
  return (
    <section id="workflow" className="py-32 bg-background border-t border-border/50 relative overflow-hidden">
      <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,rgba(0,245,255,0.05)_0%,transparent_70%)]" />

      <div className="container mx-auto px-6 relative z-10">
        <div className="text-center mb-20">
          <motion.div
            initial={{ opacity: 0, y: 16 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.6 }}
          >
            <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-6">
              agent-native pipeline
            </div>
            <h2 className="text-3xl md:text-5xl font-bold font-sans tracking-tight mb-6">
              Six operations.
              <br />
              <span className="text-muted-foreground font-normal">Zero file wandering.</span>
            </h2>
            <p className="text-xl text-muted-foreground max-w-2xl mx-auto">
              This is what the interaction loop looks like when the tool was designed for the agent,
              not adapted from the terminal.
            </p>
          </motion.div>
        </div>

        <div className="max-w-5xl mx-auto relative">
          <div className="absolute top-6 left-0 w-full h-px bg-border/30 hidden md:block" />

          <div className="grid grid-cols-1 md:grid-cols-6 gap-8 relative">
            {steps.map((step, idx) => (
              <motion.div
                key={step.id}
                initial={{ opacity: 0, y: 20 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true }}
                transition={{ duration: 0.5, delay: idx * 0.08 }}
                className="flex flex-col items-center text-center relative group"
              >
                <div className="w-12 h-12 rounded-full bg-background border border-primary/25 flex items-center justify-center mb-5 z-10 group-hover:border-primary/60 group-hover:shadow-[0_0_14px_rgba(0,245,255,0.35)] transition-all duration-300">
                  <span className="text-primary font-mono text-xs">{String(idx + 1).padStart(2, "0")}</span>
                </div>
                <h3 className="font-bold text-foreground mb-2 text-sm tracking-wide">{step.title}</h3>
                <p className="text-xs text-muted-foreground leading-relaxed">{step.desc}</p>
              </motion.div>
            ))}
          </div>
        </div>

        <motion.div
          className="text-center mt-16"
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ duration: 0.8, delay: 0.6 }}
        >
          <p className="font-mono text-xs text-muted-foreground/50">
            // the graph was always the right data structure. it just took agents to prove it.
          </p>
        </motion.div>
      </div>
    </section>
  );
}
