import { useRef, useEffect, useState, useCallback } from "react";
import { motion, useInView } from "framer-motion";
import { Link } from "wouter";

type LT = "prompt" | "output" | "success" | "warning" | "dim" | "blank";

interface L {
  at: number;
  type: LT;
  text: string;
}

const C: Record<LT, string> = {
  prompt:  "#e2e8f0",
  output:  "#7a8faa",
  success: "#00ff88",
  warning: "#ffb700",
  dim:     "#2e3f52",
  blank:   "transparent",
};

const LEFT: L[] = [
  { at: 300,   type: "dim",     text: "# Task: where is session timeout set? what breaks if I change it?" },
  { at: 900,   type: "prompt",  text: '$ grep -r "timeout" . --include="*.py"' },
  { at: 1100,  type: "dim",     text: "Searching 335 files..." },
  { at: 1500,  type: "output",  text: "./config/settings.py:45:TIMEOUT = 30" },
  { at: 1650,  type: "output",  text: "./config/settings.py:67:REQUEST_TIMEOUT = 10" },
  { at: 1800,  type: "output",  text: "./session/manager.py:23:session_timeout = 1800" },
  { at: 1950,  type: "output",  text: "./cache/redis.py:12:connection_timeout = 5" },
  { at: 2100,  type: "output",  text: "./middleware/auth.py:89:timeout_threshold = 300" },
  { at: 2250,  type: "output",  text: "./tests/test_session.py:45:assert session.timeout == 1800" },
  { at: 2400,  type: "output",  text: "./api/gateway.py:167:gateway_timeout = 15" },
  { at: 2550,  type: "output",  text: "./workers/task_runner.py:33:task_timeout = 600" },
  { at: 2700,  type: "output",  text: "./integrations/slack.py:11:request_timeout = 3" },
  { at: 2850,  type: "output",  text: "./integrations/stripe.py:8:api_timeout = 10" },
  { at: 3000,  type: "output",  text: "./db/connection.py:22:db_timeout = 30" },
  { at: 3150,  type: "output",  text: "./db/connection.py:47:query_timeout = 5000" },
  { at: 3300,  type: "output",  text: "./celery_app.py:14:task_soft_time_limit = 300" },
  { at: 3450,  type: "warning", text: "... [+841 more matches across 23 files]" },
  { at: 3750,  type: "dim",     text: "# too many — narrowing to session_timeout" },
  { at: 4200,  type: "prompt",  text: '$ grep -r "session_timeout" . --include="*.py"' },
  { at: 4500,  type: "output",  text: "./session/manager.py:23:session_timeout = 1800" },
  { at: 4650,  type: "output",  text: "./session/manager.py:89:    if self.session_timeout < 0:" },
  { at: 4800,  type: "output",  text: "./session/manager.py:134:    expiry = now + session_timeout" },
  { at: 4950,  type: "output",  text: "./middleware/auth.py:89:    if timeout > session_timeout:" },
  { at: 5100,  type: "output",  text: "./api/auth.py:55:    token_ttl = session_timeout // 2" },
  { at: 5250,  type: "dim",     text: "... [+29 more results]" },
  { at: 5600,  type: "dim",     text: "# need full context — opening main file" },
  { at: 6100,  type: "prompt",  text: "$ cat session/manager.py" },
  { at: 6400,  type: "dim",     text: "  session/manager.py — 280 lines" },
  { at: 6600,  type: "output",  text: "class SessionManager:" },
  { at: 6750,  type: "output",  text: "    def __init__(self, config, timeout=1800):" },
  { at: 6900,  type: "output",  text: "    def check_expiry(self, session_id: str) -> bool:" },
  { at: 7050,  type: "output",  text: "    def renew(self, session_id: str, delta: int) -> None:" },
  { at: 7200,  type: "dim",     text: "    ...reading line 80/280..." },
  { at: 8200,  type: "dim",     text: "    ...reading line 160/280..." },
  { at: 9200,  type: "dim",     text: "    ...reading line 240/280..." },
  { at: 10000, type: "dim",     text: "    complete — ~6,200 tokens consumed" },
  { at: 10500, type: "prompt",  text: "$ cat config/settings.py" },
  { at: 10800, type: "dim",     text: "  config/settings.py — 340 lines" },
  { at: 11000, type: "output",  text: 'SESSION_TIMEOUT = int(os.getenv("SESSION_TIMEOUT", 1800))' },
  { at: 11150, type: "output",  text: 'REQUEST_TIMEOUT = int(os.getenv("REQUEST_TIMEOUT", 10))' },
  { at: 11300, type: "dim",     text: "    ...reading line 90/340..." },
  { at: 12300, type: "dim",     text: "    ...reading line 180/340..." },
  { at: 13300, type: "dim",     text: "    ...reading line 270/340..." },
  { at: 14100, type: "dim",     text: "    complete — ~7,400 tokens consumed" },
  { at: 14600, type: "prompt",  text: '$ grep -r "SessionManager" . --include="*.py"' },
  { at: 15000, type: "dim",     text: "  12 caller sites found — need to open each one" },
  { at: 15400, type: "prompt",  text: "$ cat api/routes.py" },
  { at: 15700, type: "dim",     text: "  api/routes.py — 190 lines" },
  { at: 15850, type: "dim",     text: "    ...reading line 50/190..." },
  { at: 16550, type: "dim",     text: "    ...reading line 130/190..." },
  { at: 17150, type: "dim",     text: "    complete — ~4,100 tokens" },
  { at: 17500, type: "prompt",  text: "$ cat api/auth.py" },
  { at: 17800, type: "dim",     text: "  api/auth.py — 220 lines" },
  { at: 17950, type: "dim",     text: "    ...reading line 55/220..." },
  { at: 18750, type: "dim",     text: "    ...reading line 150/220..." },
  { at: 19350, type: "dim",     text: "    complete — ~4,800 tokens" },
  { at: 19700, type: "prompt",  text: "$ cat workers/cleanup.py" },
  { at: 20000, type: "dim",     text: "  workers/cleanup.py — 110 lines" },
  { at: 20200, type: "dim",     text: "    ...reading line 55/110..." },
  { at: 20800, type: "dim",     text: "    complete — ~2,200 tokens" },
  { at: 21100, type: "warning", text: "⚠  context window: 74% full — 28,700/38,400 tokens" },
  { at: 21700, type: "prompt",  text: "$ cat middleware/auth.py" },
  { at: 22000, type: "dim",     text: "  middleware/auth.py — 300 lines" },
  { at: 22200, type: "dim",     text: "    ...reading line 60/300..." },
  { at: 23200, type: "dim",     text: "    ...reading line 160/300..." },
  { at: 24200, type: "dim",     text: "    ...reading line 260/300..." },
  { at: 25000, type: "dim",     text: "    complete — ~6,600 tokens" },
  { at: 25400, type: "warning", text: "⚠  context window: 91% full — cannot open more files" },
  { at: 25900, type: "dim",     text: "# truncating — incomplete analysis risk" },
  { at: 26400, type: "warning", text: "⚠  6 callers still unread — forced to skip" },
  { at: 27000, type: "prompt",  text: '$ grep -r "verify_session" . --include="*.py"' },
  { at: 27400, type: "dim",     text: "  5 more files reference this function" },
  { at: 27800, type: "warning", text: "⚠  cannot open — context full. truncating." },
  { at: 28400, type: "dim",     text: "# STILL RUNNING — 28.4s and counting..." },
  { at: 30000, type: "dim",     text: "# STILL RUNNING — 30.0s and counting..." },
  { at: 32000, type: "dim",     text: "# STILL RUNNING — 32.0s and counting..." },
  { at: 34000, type: "dim",     text: "# STILL RUNNING — 34.0s and counting..." },
  { at: 36000, type: "dim",     text: "# STILL RUNNING — 36.0s and counting..." },
];


const RIGHT: L[] = [
  { at: 300,  type: "dim",     text: "# Task: where is session timeout set? what breaks if I change it?" },
  { at: 700,  type: "prompt",  text: '> m1nd.seek("session timeout configuration")' },
  { at: 880,  type: "dim",     text: "# activating graph — 9,767 nodes in memory" },
  { at: 1080, type: "success", text: "✓ 0.18s — 4 nodes located" },
  { at: 1180, type: "output",  text: "  config/settings.py  SESSION_TIMEOUT=1800" },
  { at: 1280, type: "output",  text: "  session/manager.py  check_expiry()" },
  { at: 1380, type: "output",  text: "  middleware/auth.py  verify_session()" },
  { at: 1480, type: "output",  text: "  tests/test_session.py  timeout assertions" },
  { at: 1600, type: "blank",   text: "" },
  { at: 1700, type: "prompt",  text: '> m1nd.impact("file::config/settings.py")' },
  { at: 1880, type: "success", text: "✓ 0.001s — blast radius computed" },
  { at: 1980, type: "output",  text: "  direct: 3 callers" },
  { at: 2080, type: "output",  text: "  indirect: 7 downstream files" },
  { at: 2180, type: "warning", text: "  △ HIGH RISK: session invalidation cascade" },
  { at: 2300, type: "blank",   text: "" },
  { at: 2400, type: "prompt",  text: '> m1nd.surgical_context_v2("config/settings.py", radius=2)' },
  { at: 2600, type: "success", text: "✓ 0.001s — surgical context assembled" },
  { at: 2750, type: "output",  text: "  84% fewer tokens than reading all files" },
  { at: 2900, type: "success", text: "  ✓ DONE — sending to model now" },
];

const RIGHT_DONE = 3000;
const TOTAL_MS   = 40000;

const PH = 380;
const HH = 40;
const SH = 34;
const BH = PH - HH - SH;

function Terminal({
  title, subtitle, accent, lines, doneAt, elapsed, isLeft,
}: {
  title: string; subtitle: string; accent: string;
  lines: L[]; doneAt: number | null; elapsed: number; isLeft?: boolean;
}) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const visible = lines.filter(l => l.at <= elapsed);
  const isDone    = doneAt != null && elapsed >= doneAt;
  const hasStarted = elapsed > 150;

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [visible.length]);

  let timeStr = "";
  if (hasStarted) {
    if (isDone && doneAt != null) {
      timeStr = `${(doneAt / 1000).toFixed(3)}s`;
    } else {
      timeStr = `${(elapsed / 1000).toFixed(2)}s`;
    }
  }

  return (
    <div style={{
      height: PH,
      display: "flex",
      flexDirection: "column",
      borderRadius: 12,
      border: `1px solid ${accent}28`,
      background: "#05050f",
      overflow: "hidden",
    }}>
      <div style={{
        height: HH, flexShrink: 0,
        background: "#080818",
        borderBottom: `1px solid ${accent}18`,
        display: "flex", alignItems: "center", gap: 8, padding: "0 16px",
      }}>
        <div style={{ display: "flex", gap: 6, flexShrink: 0 }}>
          <div style={{ width: 10, height: 10, borderRadius: "50%", background: "rgba(239,68,68,0.5)" }} />
          <div style={{ width: 10, height: 10, borderRadius: "50%", background: "rgba(234,179,8,0.5)" }} />
          <div style={{ width: 10, height: 10, borderRadius: "50%", background: "rgba(34,197,94,0.5)" }} />
        </div>
        <span style={{
          flex: 1, textAlign: "center", fontSize: 11,
          fontFamily: "Space Mono, monospace", color: `${accent}88`,
        }}>
          {title}
        </span>
      </div>

      <div style={{
        height: SH, flexShrink: 0,
        background: "#060614",
        borderBottom: `1px solid ${accent}12`,
        display: "flex", alignItems: "center",
        justifyContent: "space-between", padding: "0 16px",
      }}>
        <span style={{ fontSize: 10, fontFamily: "Space Mono, monospace", color: "rgba(148,163,184,0.4)" }}>
          {subtitle}
        </span>
        {hasStarted && (
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{
              fontSize: 10, fontFamily: "Space Mono, monospace",
              color: isDone ? "#00ff88" : "rgba(148,163,184,0.45)",
            }}>
              {timeStr}
            </span>
            {isDone ? (
              <span style={{
                fontSize: 10, fontFamily: "Space Mono, monospace", fontWeight: "bold",
                padding: "1px 8px", borderRadius: 4,
                background: "#00ff8814", color: "#00ff88", border: "1px solid #00ff8830",
              }}>DONE ✓</span>
            ) : (
              <span style={{
                fontSize: 10, fontFamily: "Space Mono, monospace",
                padding: "1px 8px", borderRadius: 4,
                background: `${accent}12`, color: accent, border: `1px solid ${accent}28`,
                animation: "lc-pulse 2s ease-in-out infinite",
              }}>
                {isLeft ? "STILL RUNNING…" : "RUNNING"}
              </span>
            )}
          </div>
        )}
      </div>

      <div
        ref={bodyRef}
        style={{
          height: BH,
          overflowY: "auto",
          overflowX: "hidden",
          scrollbarWidth: "none" as const,
          scrollBehavior: "smooth" as const,
          padding: 16,
          fontFamily: "Space Mono, monospace",
          fontSize: 11,
          lineHeight: "20px",
          wordBreak: "break-word" as const,
        }}
      >
        {visible.map((line, i) =>
          line.type === "blank" ? (
            <div key={i} style={{ height: 6 }} />
          ) : (
            <div key={i} style={{ color: C[line.type] }}>
              {line.type !== "prompt" ? "\u00A0\u00A0" : ""}{line.text}
            </div>
          )
        )}
        {hasStarted && !isDone && (
          <span style={{
            display: "inline-block", width: 6, height: 13, marginLeft: 2,
            background: accent, animation: "lc-blink 1s step-end infinite",
          }} />
        )}
      </div>
    </div>
  );
}

export function LandingComparison() {
  const [elapsed, setElapsed] = useState(0);
  const [phase, setPhase] = useState<"idle" | "running" | "done">("idle");
  const rafRef  = useRef<number>(0);
  const t0Ref   = useRef<number>(0);
  const wrapRef = useRef<HTMLDivElement>(null);
  const inView  = useInView(wrapRef, { once: true, margin: "-60px 0px" });

  const stop = useCallback(() => {
    if (rafRef.current) cancelAnimationFrame(rafRef.current);
  }, []);

  const play = useCallback(() => {
    stop();
    setElapsed(0);
    setPhase("running");
    t0Ref.current = performance.now();
    const tick = (now: number) => {
      const e = now - t0Ref.current;
      setElapsed(e);
      if (e < TOTAL_MS) {
        rafRef.current = requestAnimationFrame(tick);
      } else {
        setElapsed(TOTAL_MS);
        setPhase("done");
      }
    };
    rafRef.current = requestAnimationFrame(tick);
  }, [stop]);

  useEffect(() => {
    if (!inView) return;
    const t = setTimeout(play, 600);
    return () => clearTimeout(t);
  }, [inView, play]);

  useEffect(() => () => stop(), [stop]);

  return (
    <section className="py-20 border-b border-border/20 relative" id="live-demo">
      <style>{`
        @keyframes lc-blink  { 50% { opacity: 0; } }
        @keyframes lc-pulse  { 0%, 100% { opacity: 1; } 50% { opacity: 0.55; } }
      `}</style>

      <div
        className="absolute inset-0 pointer-events-none"
        style={{ background: "radial-gradient(ellipse at 50% 0%, rgba(0,245,255,0.05), transparent 60%)" }}
      />

      <div className="container mx-auto px-4 lg:px-6 relative z-10">
        <motion.div
          className="text-center mb-10"
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.7 }}
        >
          <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-5">
            live comparison
          </div>
          <h2 className="text-3xl md:text-5xl font-bold font-sans tracking-tight mb-3">
            The same task. Two different substrates.
          </h2>
          <p className="text-muted-foreground font-mono max-w-xl mx-auto text-sm">
            "Where is session timeout configured? What would break if I change it?"
          </p>
        </motion.div>

        <div ref={wrapRef} className="grid grid-cols-1 md:grid-cols-2 gap-3 lg:gap-4">
          <div className="order-2 md:order-1">
            <Terminal
              title="Traditional Agent — grep/glob/cat"
              subtitle="codex-cli · bash tools"
              accent="#ff4444"
              lines={LEFT}
              doneAt={null}
              elapsed={elapsed}
              isLeft
            />
          </div>
          <div className="order-1 md:order-2">
            <Terminal
              title="Agent + m1nd — graph-first"
              subtitle="m1nd-mcp · Rust · in-memory"
              accent="#00f5ff"
              lines={RIGHT}
              doneAt={RIGHT_DONE}
              elapsed={elapsed}
            />
          </div>
        </div>

        <div className="flex flex-col sm:flex-row items-center justify-between mt-6 gap-3">
          <div className={phase !== "done" ? "hidden" : ""}>
            <button
              onClick={play}
              className="font-mono text-xs border border-primary/20 text-primary/60 rounded px-4 py-2 hover:bg-primary/10 hover:text-primary transition-all"
            >
              ↺ replay
            </button>
          </div>
          <div className={phase !== "done" ? "mx-auto" : ""}>
            <Link href="/demo">
              <span className="inline-block font-mono text-xs text-primary/60 border border-primary/20 rounded px-4 py-2 hover:bg-primary/10 hover:text-primary transition-all cursor-pointer">
                See the 0.18s slow-motion breakdown →
              </span>
            </Link>
          </div>
        </div>
      </div>
    </section>
  );
}
