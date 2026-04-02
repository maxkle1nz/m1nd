import { useState, type ReactElement } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface FAQ {
  q: string;
  a: string | ReactElement;
  tag?: string;
}

const FAQS: FAQ[] = [
  {
    tag: "vs. alternatives",
    q: "How is m1nd different from Copilot, Cursor, or semantic search?",
    a: (
      <>
        Copilot and Cursor are editors that use LLMs to write code. m1nd is infrastructure that gives those LLMs a structured map of your codebase before they write anything. Semantic search returns documents that are <em>textually similar</em> to your query. m1nd returns nodes that are <em>structurally connected</em> — it understands that <code>check_expiry()</code> calls <code>SessionManager</code> which reads <code>settings.py</code>. Text search doesn't know that. The graph does.
      </>
    ),
  },
  {
    tag: "languages",
    q: "Does it work with any language, or only Python?",
    a: "m1nd indexes 14 languages out of the box: Python, TypeScript, JavaScript, Rust, Go, Java, C, C++, C#, Kotlin, Ruby, PHP, Swift, and Bash. The graph is built from AST-level analysis — function calls, imports, class hierarchies, module boundaries — not text patterns. If your codebase mixes languages (e.g. a Python API with a TypeScript frontend), m1nd indexes both and cross-links them.",
  },
  {
    tag: "privacy",
    q: "Does my code get sent to an external server?",
    a: "No. m1nd is local-first and always will be. The binary runs on your machine, the graph lives in RAM on your machine, and all queries are answered locally. Nothing leaves your environment. This is a deliberate architectural decision — it's why the traversal is 543ns instead of a round-trip to an API.",
  },
  {
    tag: "model support",
    q: "Which LLMs and AI clients does m1nd support?",
    a: (
      <>
        m1nd is MCP-native — it works with every client and model that speaks the Model Context Protocol. As of April 2026 that means:
        <br /><br />
        <strong style={{ color: "rgba(226,232,240,0.8)" }}>Models:</strong> Claude Opus 4.6 · Claude Sonnet 4.x · GPT-5.4 Thinking · GPT-5.3 Instant · GPT-5.4 mini · Gemini 2.5 Pro · Llama 4 — and any model released tomorrow that runs through an MCP client.
        <br /><br />
        <strong style={{ color: "rgba(226,232,240,0.8)" }}>Clients:</strong> Claude Code · Claude Desktop · Cursor · Windsurf · VS Code · ChatGPT desktop · Cline · Continue · Zed · and any custom agent that speaks MCP.
        <br /><br />
        You configure m1nd once in your MCP client settings. No per-model setup. No API key. The tool calls are identical regardless of which model is driving the agent — GPT-5.4 calls <code>m1nd.seek()</code> the same way Claude Opus 4.6 does.
      </>
    ),
  },
  {
    tag: "how it works",
    q: "How does m1nd decide which 4 nodes to return from 9,767?",
    a: "Three stages in 0.18s. First, your query is scored against pre-computed 128-dimensional embeddings for every node in the graph (PageRank-weighted TF-IDF + cosine similarity). The top-k candidates become seeds. Second, a spreading activation wave fires outward from each seed, following typed edges across up to 4 hops — 120 nodes get scored. Third, a composite score (PageRank × activation strength × edge weight) eliminates 116 of those 120. The 4 survivors are returned with their caller chains, callees, and test references already attached.",
  },
  {
    tag: "m1nd vs l1ght",
    q: "What is the difference between m1nd and l1ght?",
    a: "m1nd indexes code — it builds a knowledge graph of your functions, classes, modules, and their relationships. l1ght indexes everything else — research papers, articles, documentation, memories, conversations, Jupyter notebooks, PDFs. Both use the same graph substrate, so a single query can traverse code written last week, a research paper you read last month, and a Slack thread from last quarter. Both ship today, both are free, both are part of the same MIT-licensed binary.",
  },
  {
    tag: "pricing",
    q: "How much does it cost?",
    a: (
      <>
        m1nd is free to self-host. The binary is open source under MIT — you can clone, build, and run it today. A managed cloud version (zero-config, team sharing, hosted graph updates) is in private beta. If you want early access to the cloud version or need a commercial license for enterprise use, reach out at <a href="mailto:kleinz@m1nd.world" className="text-primary underline-offset-2 underline">kleinz@m1nd.world</a>.
      </>
    ),
  },
  {
    tag: "getting started",
    q: "How long does it take to set up?",
    a: 'Under 2 minutes. Install the binary (cargo install m1nd-mcp or brew install m1nd-mcp/tap/m1nd-mcp), add 4 lines to your MCP client config pointing to your codebase directory, and run m1nd warmup to build the initial graph. First warmup on a 50K-line codebase takes about 8 seconds. After that, incremental updates run in the background as files change.',
  },
];

function Item({ faq, isOpen, onToggle }: { faq: FAQ; isOpen: boolean; onToggle: () => void }) {
  return (
    <div
      className="border-b border-border/20 last:border-b-0"
    >
      <button
        onClick={onToggle}
        className="w-full text-left py-5 flex items-start gap-4 group"
        aria-expanded={isOpen}
      >
        {faq.tag && (
          <span
            className="flex-shrink-0 font-mono text-[9px] tracking-widest uppercase mt-1 px-2 py-0.5 rounded border transition-colors duration-200"
            style={{
              borderColor: isOpen ? "rgba(0,245,255,0.35)" : "rgba(148,163,184,0.12)",
              color: isOpen ? "#00f5ff" : "rgba(148,163,184,0.4)",
              background: isOpen ? "rgba(0,245,255,0.06)" : "transparent",
            }}
          >
            {faq.tag}
          </span>
        )}
        <span
          className="flex-1 font-sans text-base font-semibold leading-snug transition-colors duration-200"
          style={{ color: isOpen ? "#e2e8f0" : "rgba(226,232,240,0.7)" }}
        >
          {faq.q}
        </span>
        <span
          className="flex-shrink-0 w-5 h-5 mt-0.5 flex items-center justify-center rounded transition-all duration-300"
          style={{
            color: isOpen ? "#00f5ff" : "rgba(148,163,184,0.4)",
            transform: isOpen ? "rotate(45deg)" : "rotate(0deg)",
            fontSize: 18,
            lineHeight: 1,
          }}
          aria-hidden
        >
          +
        </span>
      </button>

      <AnimatePresence initial={false}>
        {isOpen && (
          <motion.div
            key="body"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.28, ease: [0.4, 0, 0.2, 1] }}
            style={{ overflow: "hidden" }}
          >
            <div
              className="pb-6 text-[15px] leading-relaxed text-muted-foreground/80 font-sans pl-0"
              style={{ paddingLeft: faq.tag ? "88px" : undefined }}
            >
              <div className="prose-none [&_code]:font-mono [&_code]:text-primary/80 [&_code]:text-[13px] [&_code]:bg-primary/8 [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_em]:text-foreground/80 [&_em]:not-italic [&_em]:font-medium [&_a]:text-primary">
                {faq.a}
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export function FAQSection() {
  const [openIdx, setOpenIdx] = useState<number | null>(0);

  const toggle = (i: number) => setOpenIdx(prev => (prev === i ? null : i));

  return (
    <section className="py-20 border-b border-border/20 relative" id="faq">
      <div
        className="absolute inset-0 pointer-events-none"
        style={{ background: "radial-gradient(ellipse at 50% 80%, rgba(0,245,255,0.03), transparent 60%)" }}
      />
      <div className="container mx-auto px-4 lg:px-6 relative z-10">
        <motion.div
          className="text-center mb-12"
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.65 }}
        >
          <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-5">
            common questions
          </div>
          <h2 className="text-3xl md:text-5xl font-bold font-sans tracking-tight mb-3">
            Before you ship it to your agent
          </h2>
          <p className="text-muted-foreground font-mono text-sm max-w-lg mx-auto">
            The questions every developer asks before adding a new tool to their stack.
          </p>
        </motion.div>

        <div className="max-w-3xl mx-auto">
          {FAQS.map((faq, i) => (
            <motion.div
              key={i}
              initial={{ opacity: 0, y: 10 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.4, delay: i * 0.06 }}
            >
              <Item faq={faq} isOpen={openIdx === i} onToggle={() => toggle(i)} />
            </motion.div>
          ))}
        </div>

        <motion.div
          className="text-center mt-12 pt-10 border-t border-border/20"
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6, delay: 0.4 }}
        >
          <p className="text-muted-foreground/60 font-mono text-xs mb-4">
            Something else? The wiki has the full technical reference.
          </p>
          <a
            href="https://m1nd.world/wiki/"
            target="_blank"
            rel="noreferrer"
            className="inline-block font-mono text-xs text-primary/60 border border-primary/20 rounded px-5 py-2.5 hover:bg-primary/10 hover:text-primary transition-all"
          >
            Read the docs →
          </a>
        </motion.div>
      </div>
    </section>
  );
}
