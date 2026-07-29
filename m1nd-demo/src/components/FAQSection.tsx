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
        Copilot and Cursor are agent hosts and editors. m1nd is the layer those agents call before they search, edit, review, or change a system. Semantic search returns documents that are <em>textually similar</em> to a query. m1nd returns context that is <em>structurally connected</em> across code, docs, and change. Ask it what a change touches, what moves with it, and what should be verified next.
      </>
    ),
  },
  {
    tag: "languages",
    q: "Does it work with any language, or only Python?",
    a: "m1nd ships dedicated extractors for more than twenty languages, routed by file extension: Python, TypeScript, JavaScript, Rust, Go, Java, C, C++, C#, Kotlin, Ruby, PHP, Swift, Scala, Bash, SQL, Lua, R, Elixir, Dart, Zig, Haskell and OCaml. The stricter claim (call-graph edges plus cross-file import resolution, proven end to end) covers 12 of them; the rest fall back to file-level structure. Mixed codebases land in one graph: a Python API and a TypeScript frontend get indexed together and cross-linked.",
  },
  {
    tag: "privacy",
    q: "Does my code get sent to an external server?",
    a: "No. m1nd is local-first and always will be. The binary runs on your machine, the graph lives in memory on your machine, and every query is answered locally. There is no telemetry, no account, and no API key. It also means there is no second copy of your code sitting in someone else's index, going stale on its own schedule.",
  },
  {
    tag: "model support",
    q: "Which LLMs and AI clients does m1nd support?",
    a: (
      <>
        m1nd is MCP-native: it works with every client and model that speaks the Model Context Protocol, across <strong style={{ color: "rgba(226,232,240,0.8)" }}>22 supported hosts</strong> (run <code>m1nd hosts plan</code> for yours):
        <br /><br />
        <strong style={{ color: "rgba(226,232,240,0.8)" }}>Models:</strong> Claude Opus · Claude Sonnet · GPT-5 · Gemini 2.5 Pro · Llama, and any model released tomorrow that runs through an MCP client.
        <br /><br />
        <strong style={{ color: "rgba(226,232,240,0.8)" }}>Clients:</strong> Claude Code · Codex · Cursor · Windsurf · GitHub Copilot · VS Code · Cline · Continue · Zed · Antigravity · and any custom agent that speaks MCP.
        <br /><br />
        You configure m1nd once. No per-model setup. No API key. The tool calls are identical regardless of which model is driving the agent. GPT-5 calls <code>seek()</code> the same way Claude Opus does.
      </>
    ),
  },
  {
    tag: "how it works",
    q: "How does m1nd pick the 4 nodes it returns from a whole graph?",
    a: "seek scores every node three ways: exact keyword match, trigram similarity, and a PageRank-style centrality prior that is gated so a well-connected hub can never outrank an actually relevant hit. Compile the optional embed feature and a small local embedding model joins the mix (fetched once, then fully offline). Spreading activation then expands from the top candidates along typed edges, and the survivors come back with their callers, callees and test references attached.",
  },
  {
    tag: "m1nd vs l1ght",
    q: "What is the difference between m1nd and l1ght?",
    a: "m1nd is the operating layer. l1ght is the document and knowledge lane inside that operating layer. m1nd gives agents durable operational context across code, docs, and change. l1ght is how specs, notes, papers, RFCs, memory, and other non-code artifacts become first-class graph surfaces inside the same system.",
  },
  {
    tag: "pricing",
    q: "How much does it cost?",
    a: (
      <>
        m1nd is free and MIT. Clone it, build it, run it, ship it inside your company. There is no cloud tier and no account; your graph never leaves your machine. If you need commercial support, a signed contract, or an invoice with a legal entity behind it, write me at <a href="mailto:kleinz@m1nd.world" className="text-primary underline-offset-2 underline">kleinz@m1nd.world</a> and we will sort it out.
      </>
    ),
  },
  {
    tag: "getting started",
    q: "How long does it take to set up?",
    a: "A few minutes. Run npx -y @maxkle1nz/m1nd update apply --yes to install the signed native runtime (it needs cosign on your PATH; cargo install m1nd-mcp is the unverified alternative). Confirm with npx -y @maxkle1nz/m1nd doctor, then m1nd hosts plan --host claude prints the exact MCP config for your host. Paste it, restart your agent, and its first north call orients on its own, ingesting the repo if the graph is empty. The ingest runs once; the graph stays warm after that.",
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
            Before you make it the first layer
          </h2>
          <p className="text-muted-foreground font-mono text-sm max-w-lg mx-auto">
            The questions teams ask before putting a new system in front of their agents.
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
