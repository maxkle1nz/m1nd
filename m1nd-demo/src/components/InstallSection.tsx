import { motion } from "framer-motion";

const STEP_INSTALL = `# install the MCP server binary
cargo install m1nd-mcp

# or with Homebrew
brew install m1nd-mcp/tap/m1nd-mcp`;

const STEP_CONFIG = `{
  "mcpServers": {
    "m1nd": {
      "command": "m1nd-mcp",
      "env": {
        "M1ND_GRAPH_SOURCE": "/tmp/m1nd-graph.json",
        "M1ND_PLASTICITY_STATE": "/tmp/m1nd-plasticity.json"
      }
    }
  }
}`;

const STEP_INGEST = `// first session — ingest your codebase
{
  "tool": "m1nd.ingest",
  "arguments": {
    "agent_id": "your-agent",
    "path": "./src",
    "adapter": "code",
    "mode": "replace"
  }
}

// graph is now live — 0 files read, 0 tokens spent`;

const PLAYBOOK = [
  {
    rule: "Always activate before searching",
    tool: "m1nd.activate",
    detail: "Spreading activation finds structurally relevant nodes in µs — before any grep or file read.",
    color: "#00f5ff",
  },
  {
    rule: "Measure blast radius before every edit",
    tool: "m1nd.impact",
    detail: "Returns direct and indirect callers sorted by coupling risk. Use it before touching any file.",
    color: "#ff6b00",
  },
  {
    rule: "Load context in one call, not many",
    tool: "m1nd.surgical_context_v2",
    detail: "Pulls the target file plus its connected callers and callees in a single graph query.",
    color: "#00ff88",
  },
  {
    rule: "Ask by intent, not by filename",
    tool: "m1nd.seek",
    detail: "Finds code by structural meaning. 'Where is shutdown guarded against cancel?' — no filenames needed.",
    color: "#ffb700",
  },
];

function CodeBlock({ code, lang = "bash" }: { code: string; lang?: string }) {
  const lines = code.split("\n");
  return (
    <div
      className="rounded-lg border overflow-hidden font-mono text-[11px] leading-5"
      style={{ borderColor: "#00f5ff14", background: "#05050f" }}
    >
      <div
        className="flex items-center gap-2 px-3 py-1.5 border-b"
        style={{ borderColor: "#00f5ff10", background: "#080818" }}
      >
        <div className="flex gap-1">
          <div className="w-2 h-2 rounded-full bg-red-500/40" />
          <div className="w-2 h-2 rounded-full bg-yellow-500/40" />
          <div className="w-2 h-2 rounded-full bg-green-500/40" />
        </div>
        <span className="text-[10px] text-muted-foreground/30 ml-1">{lang}</span>
      </div>
      <div className="p-4 space-y-0.5 overflow-x-auto">
        {lines.map((line, i) => {
          const isComment = line.trim().startsWith("#") || line.trim().startsWith("//");
          const isKey = /^  "[\w-]+"/.test(line);
          return (
            <div
              key={i}
              style={{
                color: isComment ? "#3a4a5c" : isKey ? "#00ff88" : "#e2e8f0",
                minHeight: line === "" ? "0.75rem" : undefined,
              }}
            >
              {line || "\u00a0"}
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function InstallSection() {
  const steps = [
    {
      num: "01",
      label: "Install",
      title: "Install the MCP server",
      desc: "A single Rust binary. No runtime deps. Ships with every MCP-compatible client.",
      code: STEP_INSTALL,
      lang: "bash",
    },
    {
      num: "02",
      label: "Configure",
      title: "Add to your MCP config",
      desc: "Drop it into your host's MCP server list. Claude Code, Cursor, Windsurf, GitHub Copilot coding agent, Zed, Continue, and Antigravity all have an entrypoint.",
      code: STEP_CONFIG,
      lang: "json — mcp config",
    },
    {
      num: "03",
      label: "Ingest",
      title: "Ingest your codebase",
      desc: "Your agent calls m1nd.ingest once. The graph is live. Every subsequent query runs against memory, not the filesystem.",
      code: STEP_INGEST,
      lang: "json — first tool call",
    },
  ];

  return (
    <section id="install" className="py-24 border-b border-border/20 relative">
      <div
        className="absolute inset-0 pointer-events-none"
        style={{ background: "radial-gradient(ellipse at 50% 0%, rgba(0,255,136,0.04), transparent 60%)" }}
      />
      <div className="container mx-auto px-4 lg:px-6 relative z-10">
        <motion.div
          className="text-center mb-16"
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.7 }}
        >
          <div className="inline-block font-mono text-xs text-green-400/60 tracking-widest uppercase border border-green-400/20 rounded px-3 py-1 mb-5">
            installation
          </div>
          <h2 className="text-3xl md:text-5xl font-bold font-sans tracking-tight mb-4">
            Up and running in under a minute.
          </h2>
          <p className="text-muted-foreground font-mono max-w-xl mx-auto text-sm">
            One binary. One config entry. One ingest call.{" "}
            <span className="text-green-400/80">Then the graph is live.</span>
          </p>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.5 }}
          className="mb-10 rounded-xl border border-primary/15 px-5 py-4 font-mono text-xs text-muted-foreground/70"
          style={{ background: "rgba(0,245,255,0.03)" }}
        >
          MCP entrypoints exist across major hosts: <span style={{ color: "#00f5ff" }}>Claude Code</span>,{" "}
          <span style={{ color: "#00f5ff" }}>Cursor</span>, <span style={{ color: "#00f5ff" }}>Windsurf</span>,{" "}
          <span style={{ color: "#00f5ff" }}>GitHub Copilot</span>, <span style={{ color: "#00f5ff" }}>Zed</span>,{" "}
          <span style={{ color: "#00f5ff" }}>Continue</span>, and editor-specific native proxies like{" "}
          <span style={{ color: "#00ff88" }}>Antigravity</span> when you want a hot daemon instead of a cold stdio server.
        </motion.div>

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-20">
          {steps.map((step, i) => (
            <motion.div
              key={step.num}
              initial={{ opacity: 0, y: 20 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.5, delay: i * 0.1 }}
              className="flex flex-col gap-4"
            >
              <div className="flex items-center gap-3">
                <span
                  className="text-[10px] font-mono font-bold px-2 py-1 rounded"
                  style={{
                    background: "#00ff8810",
                    color: "#00ff88",
                    border: "1px solid #00ff8825",
                  }}
                >
                  {step.num}
                </span>
                <span className="font-mono text-xs text-muted-foreground/50 tracking-widest uppercase">
                  {step.label}
                </span>
              </div>
              <div>
                <h3 className="font-sans font-semibold text-base mb-1">{step.title}</h3>
                <p className="text-sm text-muted-foreground leading-relaxed">{step.desc}</p>
              </div>
              <div className="flex-1">
                <CodeBlock code={step.code} lang={step.lang} />
              </div>
            </motion.div>
          ))}
        </div>

        <div className="border border-border/20 rounded-2xl overflow-hidden">
          <div
            className="px-6 py-4 border-b border-border/20"
            style={{ background: "rgba(0,245,255,0.02)" }}
          >
            <div className="flex items-center gap-3">
              <div
                className="w-1.5 h-1.5 rounded-full"
                style={{ background: "#00f5ff", boxShadow: "0 0 6px #00f5ff" }}
              />
              <span className="font-mono text-xs tracking-widest uppercase text-primary/60">
                agent playbook — how m1nd expects to be used
              </span>
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-0">
            {PLAYBOOK.map((item, i) => (
              <motion.div
                key={i}
                initial={{ opacity: 0 }}
                whileInView={{ opacity: 1 }}
                viewport={{ once: true }}
                transition={{ duration: 0.4, delay: i * 0.07 }}
                className="p-6 border-b border-r border-border/15 last:border-b-0"
                style={{
                  borderColor: "rgba(255,255,255,0.06)",
                }}
              >
                <div className="flex items-start gap-4">
                  <div
                    className="mt-1 w-1 h-12 rounded-full flex-shrink-0"
                    style={{ background: `linear-gradient(to bottom, ${item.color}60, transparent)` }}
                  />
                  <div className="flex-1">
                    <div className="flex items-center gap-2 mb-1.5 flex-wrap">
                      <span className="text-sm font-semibold">{item.rule}</span>
                    </div>
                    <code
                      className="text-[11px] font-mono px-2 py-0.5 rounded mb-2 inline-block"
                      style={{
                        background: `${item.color}10`,
                        color: item.color,
                        border: `1px solid ${item.color}25`,
                      }}
                    >
                      {item.tool}
                    </code>
                    <p className="text-sm text-muted-foreground leading-relaxed">{item.detail}</p>
                  </div>
                </div>
              </motion.div>
            ))}
          </div>

          <div
            className="px-6 py-5 border-t border-border/20 flex flex-col sm:flex-row items-start sm:items-center gap-4"
            style={{ background: "rgba(255,183,0,0.02)" }}
          >
            <div className="flex-1">
              <div className="flex items-center gap-2 mb-1">
                <span
                  className="text-[10px] font-mono px-2 py-0.5 rounded"
                  style={{ background: "#ffb70010", color: "#ffb700", border: "1px solid #ffb70025" }}
                >
                  m1nd.help
                </span>
                <span className="text-xs font-mono text-muted-foreground/50">in-context guidance</span>
              </div>
              <p className="text-sm text-muted-foreground">
                m1nd ships a full in-context help system. If your agent is ever uncertain which tool to call,{" "}
                <code className="text-amber-400/80 font-mono text-xs">m1nd.help</code> returns the right next step with examples — including use-case-specific guidance for whatever the agent is trying to do.
              </p>
            </div>
            <div className="flex-shrink-0">
              <div
                className="font-mono text-[10px] px-3 py-2 rounded border leading-5"
                style={{
                  background: "#ffb70008",
                  borderColor: "#ffb70020",
                  color: "#3a4a5c",
                }}
              >
                <span style={{ color: "#7a8faa" }}>{"{"}</span>
                <br />
                <span style={{ color: "#ffb700" }}>{"  \"tool\""}</span>
                <span style={{ color: "#7a8faa" }}>: </span>
                <span style={{ color: "#00ff88" }}>{'"m1nd.help"'}</span>
                <span style={{ color: "#7a8faa" }}>,</span>
                <br />
                <span style={{ color: "#ffb700" }}>{"  \"arguments\""}</span>
                <span style={{ color: "#7a8faa" }}>: {"{"}</span>
                <br />
                <span style={{ color: "#7a8faa" }}>{"    \"agent_id\""}: </span>
                <span style={{ color: "#00f5ff" }}>{'"atlas"'}</span>
                <br />
                <span style={{ color: "#7a8faa" }}>{"  }"}</span>
                <br />
                <span style={{ color: "#7a8faa" }}>{"}"}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
