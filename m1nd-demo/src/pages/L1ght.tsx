import { useRef } from "react";
import { motion, useInView } from "framer-motion";
import { NavBar } from "@/components/NavBar";
import { SEO } from "@/components/SEO";
import { Link } from "wouter";

const AMBER = "#ffb700";
const AMBER_DIM = "#ffb70022";
const AMBER_MED = "#ffb70044";
const VIOLET = "#7b61ff";
const PINK = "#ff00aa";

/* ─── L1ght Wordmark ─────────────────────────────────────────── */
function L1ghtWordmark({ size = 1 }: { size?: number }) {
  const fs = Math.round(22 * size);
  return (
    <span
      className="font-bold tracking-tight select-none"
      style={{ fontSize: fs, letterSpacing: "-0.03em", lineHeight: 1 }}
    >
      <span style={{ color: "rgba(226,232,240,0.92)" }}>l</span>
      <span
        style={{
          background: `linear-gradient(135deg, ${AMBER} 0%, #ff8c00 100%)`,
          WebkitBackgroundClip: "text",
          WebkitTextFillColor: "transparent",
          filter: `drop-shadow(0 0 8px ${AMBER}88)`,
        }}
      >
        1
      </span>
      <span style={{ color: "rgba(226,232,240,0.92)" }}>ght</span>
    </span>
  );
}

/* ─── Floating document nodes visual ───────────────────────────── */
const DOC_NODES = [
  { x: 12, y: 18, label: "arXiv:2312.04117", kind: "Paper", delay: 0 },
  { x: 72, y: 12, label: "US Patent 11,847,992", kind: "Patent", delay: 0.15 },
  { x: 88, y: 55, label: "Nature · Vol 623", kind: "Journal", delay: 0.3 },
  { x: 58, y: 78, label: "Conversation · Jan 14", kind: "Memory", delay: 0.45 },
  { x: 18, y: 70, label: "RFC 9110 · HTTP", kind: "RFC", delay: 0.6 },
  { x: 42, y: 35, label: "Transformer.pdf", kind: "PDF", delay: 0.1 },
];

const EDGES = [
  [0, 5], [5, 1], [5, 2], [3, 5], [4, 3], [2, 3],
];

function KnowledgeOrb() {
  return (
    <div className="relative w-full h-[340px] md:h-[420px] pointer-events-none select-none">
      <svg
        className="absolute inset-0 w-full h-full"
        viewBox="0 0 100 100"
        preserveAspectRatio="xMidYMid meet"
      >
        <defs>
          <radialGradient id="ambGlow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor={AMBER} stopOpacity="0.12" />
            <stop offset="100%" stopColor={AMBER} stopOpacity="0" />
          </radialGradient>
          <filter id="glow">
            <feGaussianBlur stdDeviation="0.8" result="blur" />
            <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
          </filter>
        </defs>
        <ellipse cx="50" cy="50" rx="38" ry="30" fill="url(#ambGlow)" />
        {EDGES.map(([a, b], i) => (
          <motion.line
            key={i}
            x1={DOC_NODES[a].x} y1={DOC_NODES[a].y}
            x2={DOC_NODES[b].x} y2={DOC_NODES[b].y}
            stroke={AMBER}
            strokeWidth="0.3"
            strokeOpacity="0.35"
            filter="url(#glow)"
            initial={{ pathLength: 0, opacity: 0 }}
            animate={{ pathLength: 1, opacity: 1 }}
            transition={{ duration: 1.2, delay: 0.8 + i * 0.15, ease: "easeOut" }}
          />
        ))}
        {DOC_NODES.map((n, i) => (
          <motion.g key={i} initial={{ opacity: 0, scale: 0 }} animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.5, delay: 0.3 + n.delay }}>
            <circle cx={n.x} cy={n.y} r="2.2" fill={AMBER} opacity="0.9" filter="url(#glow)" />
            <circle cx={n.x} cy={n.y} r="4.5" fill={AMBER} opacity="0.07" />
          </motion.g>
        ))}
      </svg>
      {DOC_NODES.map((n, i) => (
        <motion.div
          key={i}
          className="absolute"
          style={{ left: `${n.x}%`, top: `${n.y}%`, transform: "translate(-50%, -50%)" }}
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.5 + n.delay }}
        >
          <div
            className="rounded px-2 py-0.5 border font-mono text-[9px] whitespace-nowrap backdrop-blur-sm"
            style={{ borderColor: `${AMBER}35`, background: `${AMBER}08`, color: `${AMBER}cc` }}
          >
            <span style={{ opacity: 0.5, marginRight: 4 }}>{n.kind}</span>
            {n.label}
          </div>
        </motion.div>
      ))}
    </div>
  );
}

/* ─── Format chips ─────────────────────────────────────────────── */
const FORMATS = [
  { label: "Scientific Papers", icon: "∂", color: "#B31B1B" },
  { label: "Patents", icon: "®", color: "#A78BFA" },
  { label: "PDFs", icon: "◧", color: "#E44D26" },
  { label: "ArXiv", icon: "∂", color: "#B31B1B" },
  { label: "PubMed", icon: "⊕", color: "#E64A19" },
  { label: "DOI links", icon: "○", color: "#5B616B" },
  { label: "Web Articles", icon: "◈", color: "#06B6D4" },
  { label: "Markdown", icon: "↓", color: "#6e8efb" },
  { label: "Jupyter", icon: "◎", color: "#F37626" },
  { label: "Conversations", icon: "◎", color: "#EC4899" },
  { label: "Slack threads", icon: "⊞", color: "#4A154B" },
  { label: "Email threads", icon: "□", color: "#EA4335" },
  { label: "Notion pages", icon: "⊡", color: "#FFFFFF" },
  { label: "Wikis", icon: "⊡", color: "#10B981" },
  { label: "RFCs", icon: "∮", color: "#14B8A6" },
  { label: "HTML", icon: "⟨⟩", color: "#E44D26" },
  { label: "CSV / Tables", icon: "⊟", color: "#16A34A" },
  { label: "Slides", icon: "□", color: "#6366F1" },
  { label: "Memories", icon: "◉", color: "#8B5CF6" },
  { label: "Issues / PRs", icon: "◆", color: "#F59E0B" },
];

function FormatChip({ label, icon, color, delay }: { label: string; icon: string; color: string; delay: number }) {
  return (
    <motion.span
      initial={{ opacity: 0, scale: 0.85 }}
      whileInView={{ opacity: 1, scale: 1 }}
      viewport={{ once: true }}
      transition={{ duration: 0.3, delay }}
      className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border font-mono text-[11px] whitespace-nowrap select-none"
      style={{ borderColor: `${color}30`, background: `${color}10`, color }}
    >
      <span style={{ fontSize: 10 }}>{icon}</span>
      {label}
    </motion.span>
  );
}

/* ─── How it works ─────────────────────────────────────────────── */
const HOW_STEPS = [
  {
    n: "01", glyph: "◧", title: "Read",
    color: AMBER,
    body: "Drop a folder, a URL, a DOI, or a patent number. l1ght ingests it immediately — extracting named entities, concepts, citations, claims, and arguments. No manual tagging. No schema setup.",
    stat: "20+ formats", statSub: "PDFs · Patents · ArXiv · Web · Notebooks",
  },
  {
    n: "02", glyph: "⟁", title: "Connect",
    color: PINK,
    body: "Every concept becomes a node. Every citation, cross-reference, and shared argument becomes a typed edge. A paper from 2019 automatically links to the patent it inspired and the Slack thread where you discussed it.",
    stat: "cross-document edges", statSub: "formed automatically · zero configuration",
  },
  {
    n: "03", glyph: "𝔻", title: "Query",
    color: VIOLET,
    body: "Ask: \"What papers contradict the claim in section 3?\" or \"Which prior patents share the mechanism in claim 12?\" Spreading activation returns a surgical subgraph — not a keyword list.",
    stat: "milliseconds", statSub: "local · in-RAM · no API round-trip",
  },
];

/* ─── Stats strip ──────────────────────────────────────────────── */
const L1GHT_STATS = [
  { value: "< 2s", label: "to ingest a full-length paper", sub: "PDF · ArXiv · Patent · Web" },
  { value: "sub-ms", label: "query latency across 10K docs", sub: "in-RAM · no index warming" },
  { value: "20+", label: "formats ingested natively", sub: "zero configuration" },
  { value: "0", label: "API calls during query", sub: "fully local · air-gapped ready" },
  { value: "1", label: "shared graph with m1nd code index", sub: "code + knowledge · one query" },
];

function StatsStrip() {
  return (
    <section className="py-16 border-b border-border/15 relative overflow-hidden">
      <div className="absolute inset-0 pointer-events-none"
        style={{ background: `radial-gradient(ellipse 80% 50% at 50% 50%, ${AMBER}05 0%, transparent 70%)` }} />
      <div className="container mx-auto px-4 lg:px-6 relative z-10">
        <div className="grid grid-cols-2 md:grid-cols-5 gap-6 md:gap-4">
          {L1GHT_STATS.map((s, i) => (
            <motion.div
              key={s.value + i}
              className="flex flex-col items-center text-center gap-1"
              initial={{ opacity: 0, y: 16 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.45, delay: i * 0.08 }}
            >
              <span className="font-mono text-3xl md:text-4xl font-bold" style={{ color: AMBER }}>{s.value}</span>
              <span className="text-sm text-foreground/70 leading-snug max-w-[140px]">{s.label}</span>
              <span className="font-mono text-[11px] text-muted-foreground/45 tracking-wide mt-0.5">{s.sub}</span>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}

/* ─── Query demo ────────────────────────────────────────────────── */
const DEMO_QUERIES = [
  {
    label: "Prior art search",
    query: "Which patents share the mechanism in claim 12 of US11847992?",
    result: [
      { id: "US10,923,441", title: "Attention-gated transformer inference system", match: "claim 3, 7 — shared diffusion head mechanism" },
      { id: "EP3912057A1", title: "Sparse activation routing for language models", match: "claim 1 — identical routing topology" },
      { id: "US11,244,220", title: "Neural weight sharing across modalities", match: "claim 9 — overlapping activation protocol" },
    ],
    latency: "1.8ms",
    color: VIOLET,
  },
  {
    label: "Literature challenge map",
    query: "Papers published after 2022 that challenge the attention head pruning findings in Voita et al.?",
    result: [
      { id: "arXiv:2309.04841", title: "Rethinking Head Importance in Transformers", match: "section 4 — direct rebuttal of pruning stability claims" },
      { id: "arXiv:2401.12065", title: "Non-uniform Pruning Dynamics at Scale", match: "section 3.2 — contradicts low-rank head hypothesis" },
    ],
    latency: "0.9ms",
    color: AMBER,
  },
  {
    label: "Memory recall",
    query: "What did I read last month about federated learning that connects to RFC 9110?",
    result: [
      { id: "Conversation · Mar 18", title: "Fed learning + HTTP/2 push discussion (Slack #research)", match: "directly referenced HTTP semantics" },
      { id: "arXiv:2311.18702", title: "Privacy-Preserving Federated Inference over HTTP", match: "section 2 cites RFC 9110 §8.4" },
    ],
    latency: "2.1ms",
    color: "#00f5ff",
  },
];

function QueryDemo() {
  return (
    <section className="py-24 border-b border-border/20 relative overflow-hidden">
      <div className="absolute inset-0 pointer-events-none"
        style={{ background: `radial-gradient(ellipse 70% 50% at 50% 0%, ${AMBER}06 0%, transparent 60%)` }} />
      <div className="container mx-auto px-4 lg:px-6 relative z-10">
        <motion.div
          className="text-center mb-14"
          initial={{ opacity: 0, y: 16 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }}>
          <p className="font-mono text-[10px] tracking-widest uppercase mb-3" style={{ color: `${AMBER}55` }}>
            what queries look like
          </p>
          <h2 className="text-3xl md:text-4xl font-bold tracking-tight">
            Natural language in. Surgical graph out.
          </h2>
          <p className="mt-3 text-muted-foreground/50 text-sm font-mono max-w-md mx-auto">
            Not a keyword list. Not 10 blue links. The exact nodes and edges that answer the question.
          </p>
        </motion.div>

        <div className="flex flex-col gap-6 max-w-3xl mx-auto">
          {DEMO_QUERIES.map((q, qi) => (
            <motion.div
              key={qi}
              className="rounded-2xl border overflow-hidden"
              style={{ borderColor: `${q.color}22`, background: `${q.color}04` }}
              initial={{ opacity: 0, y: 20 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.5, delay: qi * 0.1 }}
            >
              {/* Query bar */}
              <div className="flex items-center gap-3 px-5 py-4 border-b" style={{ borderColor: `${q.color}15`, background: `${q.color}06` }}>
                <span className="font-mono text-[9px] tracking-widest uppercase px-2 py-0.5 rounded border"
                  style={{ borderColor: `${q.color}30`, color: `${q.color}99`, background: `${q.color}10` }}>
                  {q.label}
                </span>
                <span className="font-mono text-xs text-muted-foreground/50 flex-1 leading-relaxed">
                  ❯ {q.query}
                </span>
              </div>
              {/* Results */}
              <div className="divide-y" style={{ borderColor: "rgba(255,255,255,0.04)" }}>
                {q.result.map((r, ri) => (
                  <div key={ri} className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
                    <span className="font-mono text-xs shrink-0 pt-0.5" style={{ color: `${q.color}90` }}>{r.id}</span>
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-foreground/85 leading-snug">{r.title}</p>
                      <p className="font-mono text-xs text-muted-foreground/60 mt-0.5 leading-relaxed">{r.match}</p>
                    </div>
                  </div>
                ))}
              </div>
              {/* Footer */}
              <div className="px-5 py-2.5 border-t flex items-center justify-between"
                style={{ borderColor: `${q.color}10`, background: `${q.color}04` }}>
                <span className="font-mono text-[10px] text-muted-foreground/45">spreading activation · typed edge traversal</span>
                <span className="font-mono text-[11px]" style={{ color: `${q.color}80` }}>{q.latency}</span>
              </div>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}

/* ─── L1ght FAQ ─────────────────────────────────────────────────── */
const L1GHT_FAQ = [
  {
    q: "How does l1ght ingest a document?",
    a: "Give it a URL, a DOI, a patent number, a file path, or a folder. l1ght fetches and parses it immediately — extracting named entities, concepts, citations, claims, and the relationships between them. No manual tagging, no schema setup, no pipeline to configure.",
  },
  {
    q: "Does it require an internet connection?",
    a: "Only to fetch remote sources (URLs, DOIs, ArXiv papers) on first ingest. Once a document is in the graph, everything — all queries, all traversals — runs fully local and offline. l1ght never phones home and never sends your data anywhere.",
  },
  {
    q: "How is it different from semantic / vector search?",
    a: "Vector search ranks documents by embedding cosine similarity. l1ght traverses a typed graph of relationships — citations, shared claims, counter-arguments, conceptual overlaps — using spreading activation. It doesn't find the most \"similar\" document. It finds the document that is structurally most relevant to your query given the connections in your corpus.",
  },
  {
    q: "Does l1ght replace Zotero, Obsidian, or Notion?",
    a: "No — those are note-taking and reference management tools. l1ght is a query layer. It doesn't replace how you read or organize; it adds a graph-queryable intelligence layer on top of everything you've already ingested. You can keep using Zotero to manage citations and use l1ght to query across all of them at once.",
  },
  {
    q: "Can I keep my documents private?",
    a: "Yes. l1ght runs entirely in-process on your machine. No document content, no query, and no result ever leaves your local environment. It is safe for confidential research, legal documents, proprietary filings, and private conversations.",
  },
  {
    q: "How does l1ght connect to m1nd's code graph?",
    a: "They run on the same graph substrate. When both are active, a single query can traverse code nodes and document nodes in one hop — e.g. \"which papers informed the design of this function?\" or \"which RFC section does this implementation reference?\" No extra configuration needed.",
  },
  {
    q: "Which AI agents and clients work with l1ght?",
    a: "Any MCP-compatible client — Claude Code, Claude Desktop, Cursor, Windsurf, VS Code with MCP, ChatGPT, Cline, Continue, Zed. l1ght exposes the same MCP tool surface as m1nd, so if your agent speaks MCP, it speaks l1ght.",
  },
  {
    q: "Is l1ght production-ready?",
    a: "Yes. It ships in the same binary as m1nd under the same MIT license. There is no separate install, no separate config, and no stability difference. l1ght is not an experimental feature — it's one of the two core modes of the m1nd engine.",
  },
];

function L1ghtFAQ() {
  return (
    <section className="py-24 border-b border-border/20 relative">
      <div className="container mx-auto px-4 lg:px-6 max-w-3xl">
        <motion.div
          className="text-center mb-14"
          initial={{ opacity: 0, y: 16 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }}>
          <p className="font-mono text-[10px] tracking-widest uppercase mb-3" style={{ color: `${AMBER}55` }}>
            questions
          </p>
          <h2 className="text-3xl md:text-4xl font-bold tracking-tight">
            l1ght FAQ
          </h2>
        </motion.div>

        <div className="flex flex-col gap-3">
          {L1GHT_FAQ.map((item, i) => (
            <motion.div
              key={i}
              className="rounded-xl border p-5 group"
              style={{ borderColor: `${AMBER}15`, background: `${AMBER}04` }}
              initial={{ opacity: 0, y: 12 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.4, delay: i * 0.06 }}
            >
              <p className="font-semibold text-sm text-foreground/90 mb-2 leading-snug">{item.q}</p>
              <p className="text-sm text-muted-foreground/60 leading-relaxed font-mono">{item.a}</p>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}

/* ─── Audience cards ───────────────────────────────────────────── */
const AUDIENCES = [
  {
    role: "Researchers",
    icon: "∂",
    color: "#B31B1B",
    headline: "Track how ideas evolve across 200 papers without reading 200 papers.",
    body: "l1ght maps citations, counter-arguments, and replications across your entire literature corpus. Ask \"what challenges the consensus in this field?\" and get the exact papers, not a search page.",
    tags: ["ArXiv", "PubMed", "DOI", "PDFs"],
  },
  {
    role: "Patent Attorneys",
    icon: "®",
    color: "#A78BFA",
    headline: "Find prior art across 10,000 patents before filing.",
    body: "l1ght indexes claims, independent claims, and inventor chains into a queryable graph. \"Which patents share the mechanism in claim 3?\" is a single query, not a day of USPTO searches.",
    tags: ["US Patents", "EP Patents", "Claims", "Prior Art"],
  },
  {
    role: "Analysts",
    icon: "◈",
    color: "#06B6D4",
    headline: "Connect market reports, earnings calls, and news into one graph.",
    body: "Ingest 10 competitors' 10-Ks, 40 quarters of earnings transcripts, and 500 news articles. Ask \"where are the contradictions between their public statements and their filings?\"",
    tags: ["PDFs", "Web articles", "CSVs", "Transcripts"],
  },
  {
    role: "Scientists & R&D",
    icon: "⊕",
    color: "#10B981",
    headline: "Build a live knowledge graph of your lab's entire research history.",
    body: "Every paper the lab has read, every internal report, every Jupyter notebook — one graph. New team members get full institutional memory from day one, not a folder of PDFs.",
    tags: ["Papers", "Notebooks", "Reports", "Wikis"],
  },
];

function AudienceCard({ role, icon, color, headline, body, tags, delay }: typeof AUDIENCES[0] & { delay: number }) {
  return (
    <motion.div
      className="rounded-2xl border p-6 flex flex-col gap-4 relative overflow-hidden"
      style={{ borderColor: `${color}20`, background: `${color}06` }}
      initial={{ opacity: 0, y: 24 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true }}
      transition={{ duration: 0.5, delay }}
    >
      <div
        className="absolute top-0 right-0 w-32 h-32 rounded-full pointer-events-none"
        style={{ background: `radial-gradient(circle, ${color}10 0%, transparent 70%)`, transform: "translate(30%, -30%)" }}
      />
      <div className="flex items-center gap-2">
        <span className="font-mono text-xl" style={{ color }}>{icon}</span>
        <span className="font-mono text-[10px] tracking-widest uppercase" style={{ color, opacity: 0.7 }}>{role}</span>
      </div>
      <p className="text-base font-bold leading-snug tracking-tight">{headline}</p>
      <p className="text-sm text-muted-foreground/65 leading-relaxed font-mono">{body}</p>
      <div className="flex flex-wrap gap-1.5 mt-auto">
        {tags.map(t => (
          <span key={t} className="px-2 py-0.5 rounded-full border font-mono text-[10px]"
            style={{ borderColor: `${color}25`, background: `${color}08`, color: `${color}cc` }}>
            {t}
          </span>
        ))}
      </div>
    </motion.div>
  );
}

/* ─── Install buttons ──────────────────────────────────────────── */
function InstallButtons() {
  return (
    <div className="flex flex-col sm:flex-row items-center gap-4 w-full justify-center">
      <a
        href="https://github.com/maxkle1nz/m1nd"
        target="_blank"
        rel="noreferrer"
        className="w-full sm:w-auto px-8 py-3 rounded-lg font-mono text-sm font-bold transition-all hover:opacity-90 active:scale-95 whitespace-nowrap text-center"
        style={{ background: AMBER, color: "#050510", boxShadow: `0 0 20px ${AMBER}44` }}
      >
        Install l1ght
      </a>
      <a
        href="https://m1nd.world/wiki/"
        target="_blank"
        rel="noreferrer"
        className="w-full sm:w-auto px-8 py-3 rounded-lg font-mono text-sm font-bold border transition-all hover:opacity-80 whitespace-nowrap text-center"
        style={{ borderColor: `${AMBER}35`, color: AMBER, background: `${AMBER}08` }}
      >
        Read the wiki
      </a>
    </div>
  );
}

/* ─── One graph section ────────────────────────────────────────── */
function OneGraphSection() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { once: true });

  return (
    <section ref={ref} className="py-24 border-b border-border/20 relative overflow-hidden">
      <div className="absolute inset-0 pointer-events-none"
        style={{ background: "radial-gradient(ellipse 60% 50% at 50% 50%, rgba(255,183,0,0.04) 0%, transparent 70%)" }} />
      <div className="container mx-auto px-4 lg:px-6 relative z-10 text-center">
        <motion.p
          className="font-mono text-[10px] text-muted-foreground/30 tracking-widest uppercase mb-4"
          initial={{ opacity: 0 }} animate={inView ? { opacity: 1 } : {}}
          transition={{ duration: 0.5 }}
        >
          the bigger picture
        </motion.p>
        <motion.h2
          className="text-3xl md:text-5xl font-bold tracking-tight mb-6"
          initial={{ opacity: 0, y: 16 }} animate={inView ? { opacity: 1, y: 0 } : {}}
          transition={{ duration: 0.55, delay: 0.1 }}
        >
          One graph.{" "}
          <span style={{ color: "#00f5ff" }}>Code.</span>{" "}
          <span style={{ color: AMBER }}>Knowledge.</span>{" "}
          <span style={{ color: VIOLET }}>Memory.</span>
        </motion.h2>
        <motion.p
          className="text-muted-foreground/60 max-w-xl mx-auto text-base font-mono leading-relaxed mb-12"
          initial={{ opacity: 0 }} animate={inView ? { opacity: 1 } : {}}
          transition={{ duration: 0.5, delay: 0.2 }}
        >
          m1nd indexes your code. l1ght indexes your knowledge. Both run on the same graph substrate — so a single query can traverse a function written last week, the paper that inspired it, and the Slack thread where you decided to build it.
        </motion.p>

        <div className="flex flex-col md:flex-row items-center justify-center gap-6 max-w-2xl mx-auto">
          {[
            { name: "m1nd", label: "code graph", color: "#00f5ff", sub: "functions · classes · imports · call chains" },
            { name: "+", label: "", color: "rgba(255,255,255,0.2)", sub: "" },
            { name: "l1ght", label: "knowledge graph", color: AMBER, sub: "papers · patents · memory · conversations" },
          ].map((item, i) => (
            <motion.div
              key={i}
              className="flex flex-col items-center gap-1"
              initial={{ opacity: 0, scale: 0.88 }}
              animate={inView ? { opacity: 1, scale: 1 } : {}}
              transition={{ duration: 0.45, delay: 0.3 + i * 0.1 }}
            >
              {item.label ? (
                <>
                  <div
                    className="px-6 py-3 rounded-xl border text-2xl font-bold"
                    style={{ borderColor: `${item.color}30`, background: `${item.color}08`, color: item.color,
                      boxShadow: `0 0 24px ${item.color}18` }}
                  >
                    {item.name}
                  </div>
                  <p className="font-mono text-[11px] text-muted-foreground/45 uppercase tracking-widest mt-1">{item.label}</p>
                  <p className="font-mono text-[10px] text-muted-foreground/35 mt-0.5 max-w-[160px] text-center">{item.sub}</p>
                </>
              ) : (
                <span className="text-3xl font-bold" style={{ color: item.color }}>{item.name}</span>
              )}
            </motion.div>
          ))}
        </div>

        <motion.div
          className="mt-10 inline-flex items-center gap-2 px-4 py-2 rounded-full border font-mono text-xs"
          style={{ borderColor: `${VIOLET}25`, background: `${VIOLET}08`, color: `${VIOLET}cc` }}
          initial={{ opacity: 0 }} animate={inView ? { opacity: 1 } : {}}
          transition={{ duration: 0.5, delay: 0.55 }}
        >
          <span>⍐</span>
          one query · one result · no context switching
        </motion.div>
      </div>
    </section>
  );
}

/* ─── Page ─────────────────────────────────────────────────────── */
export default function L1ght() {
  return (
    <main className="w-full min-h-screen bg-background">
      <SEO
        title="l1ght — Knowledge Graph for Research, Patents & Memory"
        description="l1ght ingests scientific papers, patents, PDFs, articles, and conversations into a queryable knowledge graph. Find connections across everything you've ever read — in milliseconds."
        canonicalPath="/l1ght"
      />
      <NavBar />

      {/* ── Hero ── */}
      <section className="relative min-h-screen flex flex-col items-center justify-center pt-24 pb-16 overflow-hidden">
        <div className="absolute inset-0 pointer-events-none">
          <div style={{ background: `radial-gradient(ellipse 80% 60% at 50% 30%, ${AMBER}08 0%, transparent 70%)`, position: "absolute", inset: 0 }} />
          <div style={{ background: "radial-gradient(ellipse 60% 40% at 50% 80%, rgba(123,97,255,0.06) 0%, transparent 70%)", position: "absolute", inset: 0 }} />
        </div>

        <div className="container mx-auto px-4 lg:px-6 relative z-10 flex flex-col items-center text-center">
          {/* Badge */}
          <motion.div
            className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full border font-mono text-xs mb-8"
            style={{ borderColor: `${AMBER}30`, background: `${AMBER}08`, color: `${AMBER}cc` }}
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.5 }}
          >
            <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: AMBER }} />
            ships with m1nd · MIT · free
          </motion.div>

          {/* Wordmark large */}
          <motion.div
            className="mb-6"
            initial={{ opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.55, delay: 0.08 }}
          >
            <L1ghtWordmark size={5} />
          </motion.div>

          {/* Headline */}
          <motion.h1
            className="text-4xl md:text-6xl lg:text-7xl font-bold tracking-tight max-w-4xl leading-[1.05] mb-6"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.6, delay: 0.16 }}
          >
            You're not missing{" "}
            <span style={{ color: AMBER }}>information.</span>
            <br />
            You're missing the{" "}
            <span style={{ color: VIOLET }}>connections</span>{" "}
            between it.
          </motion.h1>

          {/* Sub */}
          <motion.p
            className="text-lg md:text-xl text-muted-foreground/65 max-w-2xl mx-auto mb-10 leading-relaxed"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.6, delay: 0.28 }}
          >
            l1ght ingests patents, scientific papers, PDFs, articles, and conversations into a knowledge graph queryable in milliseconds. Everything you've ever read — finally connected.
          </motion.p>

          {/* CTA */}
          <motion.div
            className="w-full"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.5, delay: 0.38 }}
          >
            <InstallButtons />
            <p className="mt-4 font-mono text-xs text-muted-foreground/45 text-center">
              cargo install m1nd-mcp &nbsp;·&nbsp; brew install m1nd-mcp/tap/m1nd-mcp
            </p>
          </motion.div>

          {/* Knowledge orb visual */}
          <motion.div
            className="w-full max-w-3xl mt-10"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 1, delay: 0.5 }}
          >
            <KnowledgeOrb />
          </motion.div>
        </div>
      </section>

      {/* ── Format strip ── */}
      <section className="py-14 border-t border-b border-border/15 relative overflow-hidden">
        <div className="absolute inset-0 pointer-events-none"
          style={{ background: `linear-gradient(180deg, ${AMBER}06 0%, transparent 100%)` }} />
        <div className="container mx-auto px-4 lg:px-6 relative z-10">
          <motion.div className="text-center mb-8"
            initial={{ opacity: 0, y: 10 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }}>
            <p className="font-mono text-[10px] tracking-widest uppercase mb-2"
              style={{ color: `${AMBER}55` }}>what l1ght reads</p>
            <h2 className="text-2xl md:text-3xl font-bold tracking-tight">
              Every format. Zero configuration.
            </h2>
          </motion.div>
          <div className="flex flex-wrap justify-center gap-2">
            {FORMATS.map((f, i) => (
              <FormatChip key={f.label} {...f} delay={i * 0.03} />
            ))}
          </div>
          <motion.p className="text-center mt-6 font-mono text-xs text-muted-foreground/50"
            initial={{ opacity: 0 }} whileInView={{ opacity: 1 }} viewport={{ once: true }}
            transition={{ delay: 0.6 }}>
            Drop a folder, paste a URL, or give l1ght a DOI. It handles the rest.
          </motion.p>
        </div>
      </section>

      {/* ── How it works ── */}
      <section className="py-24 border-b border-border/20 relative">
        <div className="container mx-auto px-4 lg:px-6">
          <motion.div className="text-center mb-16"
            initial={{ opacity: 0, y: 16 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }}>
            <p className="font-mono text-[10px] tracking-widest uppercase mb-3"
              style={{ color: `${AMBER}50` }}>how l1ght works</p>
            <h2 className="text-3xl md:text-4xl font-bold tracking-tight">
              Read. Connect. Query.
            </h2>
            <p className="mt-3 text-muted-foreground/55 text-sm font-mono max-w-md mx-auto">
              Three stages. No ontology design. No schema setup. No data pipeline.
            </p>
          </motion.div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
            {HOW_STEPS.map((step, i) => (
              <motion.div
                key={step.n}
                className="relative rounded-2xl border p-6 flex flex-col gap-4"
                style={{ borderColor: `${step.color}20`, background: `${step.color}05` }}
                initial={{ opacity: 0, y: 24 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true }}
                transition={{ duration: 0.5, delay: i * 0.12 }}
              >
                <div className="flex items-baseline gap-2">
                  <span className="font-mono text-5xl font-bold leading-none"
                    style={{ color: "rgba(255,255,255,0.04)" }}>{step.n}</span>
                  <span className="font-mono text-xl" style={{ color: step.color }}>{step.glyph}</span>
                </div>
                <h3 className="text-xl font-bold" style={{ color: step.color }}>{step.title}</h3>
                <p className="text-sm text-muted-foreground/65 leading-relaxed font-mono flex-1">{step.body}</p>
                <div className="inline-flex flex-col px-3 py-2 rounded-lg border mt-auto"
                  style={{ borderColor: `${step.color}20`, background: `${step.color}08` }}>
                  <span className="font-mono text-base font-bold" style={{ color: step.color }}>{step.stat}</span>
                  <span className="font-mono text-[11px] text-muted-foreground/55 mt-0.5">{step.statSub}</span>
                </div>
              </motion.div>
            ))}
          </div>
        </div>
      </section>

      {/* ── Stats strip ── */}
      <StatsStrip />

      {/* ── Query demo ── */}
      <QueryDemo />

      {/* ── Audience cards ── */}
      <section className="py-24 border-b border-border/20 relative">
        <div className="container mx-auto px-4 lg:px-6">
          <motion.div className="text-center mb-14"
            initial={{ opacity: 0, y: 16 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }}>
            <p className="font-mono text-[10px] tracking-widest uppercase mb-3" style={{ color: `${AMBER}50` }}>
              built for knowledge workers
            </p>
            <h2 className="text-3xl md:text-4xl font-bold tracking-tight">
              Who uses l1ght
            </h2>
          </motion.div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
            {AUDIENCES.map((a, i) => (
              <AudienceCard key={a.role} {...a} delay={i * 0.1} />
            ))}
          </div>
        </div>
      </section>

      {/* ── One graph ── */}
      <OneGraphSection />

      {/* ── L1ght FAQ ── */}
      <L1ghtFAQ />

      {/* ── Bottom CTA ── */}
      <section className="py-32 relative overflow-hidden text-center">
        <div className="absolute inset-0 pointer-events-none"
          style={{ background: `radial-gradient(ellipse 70% 60% at 50% 100%, ${AMBER}10 0%, transparent 65%)` }} />
        <div className="container mx-auto px-4 lg:px-6 relative z-10 flex flex-col items-center">
          <motion.div
            className="font-mono text-[10px] tracking-widest uppercase mb-5 flex items-center gap-2"
            style={{ color: `${AMBER}55` }}
            initial={{ opacity: 0 }} whileInView={{ opacity: 1 }} viewport={{ once: true }}>
            <span>𝔻</span> open source · MIT license · free forever
          </motion.div>
          <motion.h2
            className="text-4xl md:text-6xl font-bold tracking-tight mb-4"
            initial={{ opacity: 0, y: 16 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }}
            transition={{ duration: 0.55, delay: 0.08 }}>
            Stop searching.{" "}
            <span style={{
              background: `linear-gradient(135deg, ${AMBER} 0%, #ff8c00 100%)`,
              WebkitBackgroundClip: "text", WebkitTextFillColor: "transparent",
              filter: `drop-shadow(0 0 20px ${AMBER}44)`
            }}>Start querying.</span>
          </motion.h2>
          <motion.p
            className="text-muted-foreground/55 max-w-lg mx-auto mb-10 text-lg leading-relaxed"
            initial={{ opacity: 0 }} whileInView={{ opacity: 1 }} viewport={{ once: true }}
            transition={{ delay: 0.2 }}>
            The paper you read last month, the patent filed yesterday, the conversation from last quarter — all in the same graph, all queryable in milliseconds.
          </motion.p>
          <motion.div
            className="w-full"
            initial={{ opacity: 0, y: 12 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }}
            transition={{ delay: 0.3 }}>
            <InstallButtons />
            <p className="mt-4 font-mono text-xs text-muted-foreground/45 text-center">
              cargo install m1nd-mcp &nbsp;·&nbsp; brew install m1nd-mcp/tap/m1nd-mcp
            </p>
          </motion.div>
          <motion.div
            className="mt-16 flex flex-col sm:flex-row justify-between items-center gap-4 w-full max-w-3xl border-t pt-8 text-sm text-muted-foreground"
            style={{ borderColor: "rgba(255,255,255,0.06)" }}
            initial={{ opacity: 0 }} whileInView={{ opacity: 1 }} viewport={{ once: true }}
            transition={{ delay: 0.4 }}>
            <p className="font-mono text-[11px] text-muted-foreground/30">© {new Date().getFullYear()} m1nd / l1ght. Part of the same graph.</p>
            <div className="flex items-center gap-6 text-xs font-mono">
              <Link href="/" className="transition-colors tracking-widest uppercase hover:opacity-70"
                style={{ color: "#00f5ff55" }}>m1nd</Link>
              <Link href="/use-cases" className="text-muted-foreground/25 hover:text-muted-foreground/50 transition-colors tracking-widest uppercase">Use Cases</Link>
              <Link href="/demo" className="text-muted-foreground/25 hover:text-muted-foreground/50 transition-colors tracking-widest uppercase">Demo</Link>
              <a href="https://m1nd.world/wiki/" target="_blank" rel="noreferrer"
                className="text-muted-foreground/25 hover:text-muted-foreground/50 transition-colors tracking-widest uppercase">Wiki</a>
            </div>
          </motion.div>
        </div>
      </section>
    </main>
  );
}
