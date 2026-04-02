import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { FeatureSection } from "./FeatureSection";

const CYAN  = "#00f5ff";
const GREEN = "#00ff88";
const AMBER = "#ffb700";

/* ── Graph data — percentage positions (0-100) in the visual area ── */
const NODES = [
  { id: 0, px: 12, py: 50, label: "db/queries.ts",     sub: "query(sql)",    color: AMBER, side: "above" as const },
  { id: 1, px: 36, py: 50, label: "api/handler.ts",     sub: "handle(req)",   color: CYAN,  side: "below" as const },
  { id: 2, px: 62, py: 50, label: "auth/middleware.ts", sub: "verify(token)", color: CYAN,  side: "above" as const },
  { id: 3, px: 88, py: 50, label: "client/hooks.ts",    sub: "useQuery(fn)",  color: CYAN,  side: "below" as const },
  { id: 4, px: 36, py: 20, label: "cache/redis.ts",     sub: "get / set",     color: GREEN, side: "above" as const },
  { id: 5, px: 62, py: 80, label: "logger/index.ts",    sub: "debug(msg)",    color: GREEN, side: "below" as const },
];

const EDGES = [
  { from: 0, to: 1, color: CYAN,  step: 1 },
  { from: 1, to: 2, color: CYAN,  step: 2 },
  { from: 2, to: 3, color: CYAN,  step: 3 },
  { from: 1, to: 4, color: GREEN, step: 4 },
  { from: 2, to: 5, color: GREEN, step: 5 },
];

/* ── Step timing (ms per step before advancing) ── */
const STEP_MS = [550, 1100, 1100, 1100, 850, 850, 2600];

/* ── Status text per step ── */
const STATUS: Record<number, { text: string; color: string }> = {
  0: { text: "scanning origin node…",                        color: "#ffffff45" },
  1: { text: "hop 1 → api/handler.ts",                       color: "#ffffff55" },
  2: { text: "hop 2 → auth/middleware.ts",                   color: "#ffffff55" },
  3: { text: "hop 3 → client/hooks.ts  ·  destination reached", color: GREEN  },
  4: { text: "branch discovered: cache/redis.ts",             color: `${GREEN}cc` },
  5: { text: "branch discovered: logger/index.ts",            color: `${GREEN}cc` },
  6: { text: "complete  ·  4 hops  ·  2 branches  ·  0.18ms", color: CYAN    },
};

function nodeActive(id: number, step: number) {
  if (id === 0) return true;
  const e = EDGES.find(e => e.to === id);
  return e ? step >= e.step : false;
}

function edgeActive(idx: number, step: number) {
  return step >= EDGES[idx].step;
}

/* Particle tracks the main-path node for steps 0-3, stays at N3 for branches */
function particleNode(step: number) {
  return NODES[Math.min(step, 3)];
}

/* ─── NodeDot ─────────────────────────────────────────────────────── */
function NodeDot({ n, active }: { n: typeof NODES[0]; active: boolean }) {
  return (
    <div
      className="absolute"
      style={{
        left: `${n.px}%`,
        top:  `${n.py}%`,
        transform: "translate(-50%, -50%)",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 5,
        zIndex: 2,
        pointerEvents: "none",
      }}
    >
      {/* Label above */}
      {n.side === "above" && (
        <div style={{ textAlign: "center", marginBottom: 2 }}>
          <div style={{
            fontSize: 10, whiteSpace: "nowrap",
            color: active ? n.color : "#ffffff22",
            transition: "color 0.5s",
            letterSpacing: "0.01em",
          }}>{n.label}</div>
          <div style={{
            fontSize: 8, marginTop: 2,
            color: active ? "#ffffff50" : "#ffffff12",
            transition: "color 0.5s",
          }}>{n.sub}</div>
        </div>
      )}

      {/* Dot */}
      <div style={{ position: "relative", display: "flex", alignItems: "center", justifyContent: "center" }}>
        {active && (
          <motion.div
            style={{
              position: "absolute",
              width: 20, height: 20,
              borderRadius: "50%",
              border: `1px solid ${n.color}`,
              top: "50%", left: "50%",
              marginTop: -10, marginLeft: -10,
            }}
            animate={{ scale: [1, 2.4], opacity: [0.55, 0] }}
            transition={{ duration: 1.8, repeat: Infinity, ease: "easeOut" }}
          />
        )}
        <div style={{
          width: 10, height: 10,
          borderRadius: "50%",
          background: active ? n.color : "transparent",
          border: `1.5px solid ${active ? n.color : "#ffffff22"}`,
          boxShadow: active ? `0 0 8px ${n.color}80, 0 0 18px ${n.color}30` : "none",
          transition: "all 0.4s",
        }} />
      </div>

      {/* Label below */}
      {n.side === "below" && (
        <div style={{ textAlign: "center", marginTop: 2 }}>
          <div style={{
            fontSize: 10, whiteSpace: "nowrap",
            color: active ? n.color : "#ffffff22",
            transition: "color 0.5s",
            letterSpacing: "0.01em",
          }}>{n.label}</div>
          <div style={{
            fontSize: 8, marginTop: 2,
            color: active ? "#ffffff50" : "#ffffff12",
            transition: "color 0.5s",
          }}>{n.sub}</div>
        </div>
      )}
    </div>
  );
}

/* ─── Main scene ──────────────────────────────────────────────────── */
function TraceScene({ step }: { step: number }) {
  const pNode = particleNode(step);
  const transMs = STEP_MS[Math.min(step, 5)] * 0.88;

  return (
    <div className="relative w-full h-full overflow-hidden" style={{ background: "#050510" }}>

      {/* SVG layer — only draws edges */}
      <svg
        className="absolute inset-0"
        width="100%" height="100%"
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        style={{ overflow: "visible" }}
      >
        {/* Dim base edges (always shown) */}
        {EDGES.map((e, i) => (
          <line
            key={`b${i}`}
            x1={NODES[e.from].px} y1={NODES[e.from].py}
            x2={NODES[e.to].px}   y2={NODES[e.to].py}
            stroke="#ffffff" strokeWidth="0.12" strokeOpacity="0.09"
            strokeDasharray="0.9 2.2"
          />
        ))}

        {/* Active edge: glow + flowing dash */}
        {EDGES.map((e, i) =>
          edgeActive(i, step) ? (
            <g key={`a${i}`}>
              <line
                x1={NODES[e.from].px} y1={NODES[e.from].py}
                x2={NODES[e.to].px}   y2={NODES[e.to].py}
                stroke={e.color} strokeWidth="0.65" strokeOpacity="0.14"
              />
              <line
                x1={NODES[e.from].px} y1={NODES[e.from].py}
                x2={NODES[e.to].px}   y2={NODES[e.to].py}
                stroke={e.color} strokeWidth="0.22" strokeOpacity="0.78"
                strokeDasharray="1.6 3.8"
              >
                <animate
                  attributeName="stroke-dashoffset"
                  from="0" to="-5.4"
                  dur="0.55s"
                  repeatCount="indefinite"
                />
              </line>
            </g>
          ) : null
        )}
      </svg>

      {/* HTML node dots + labels */}
      {NODES.map(n => (
        <NodeDot key={n.id} n={n} active={nodeActive(n.id, step)} />
      ))}

      {/* Traveling particle (HTML div — perfectly circular, CSS glow) */}
      <motion.div
        style={{
          position: "absolute",
          width: 8,
          height: 8,
          borderRadius: "50%",
          background: step <= 3 ? CYAN : GREEN,
          boxShadow: `0 0 8px ${step <= 3 ? CYAN : GREEN}90, 0 0 18px ${step <= 3 ? CYAN : GREEN}40`,
          transform: "translate(-50%, -50%)",
          zIndex: 20,
          pointerEvents: "none",
        }}
        initial={{ left: `${NODES[0].px}%`, top: `${NODES[0].py}%` }}
        animate={{ left: `${pNode.px}%`, top: `${pNode.py}%` }}
        transition={{ duration: transMs / 1000, ease: "easeInOut" }}
      />

      {/* Particle glow ring */}
      <motion.div
        style={{
          position: "absolute",
          width: 22,
          height: 22,
          borderRadius: "50%",
          background: step <= 3 ? CYAN : GREEN,
          opacity: 0.12,
          transform: "translate(-50%, -50%)",
          zIndex: 19,
          pointerEvents: "none",
        }}
        initial={{ left: `${NODES[0].px}%`, top: `${NODES[0].py}%` }}
        animate={{ left: `${pNode.px}%`, top: `${pNode.py}%` }}
        transition={{ duration: transMs / 1000, ease: "easeInOut" }}
      />
    </div>
  );
}

/* ─── Export ─────────────────────────────────────────────────────── */
export function NavigationFeature() {
  const [step, setStep] = useState(0);

  useEffect(() => {
    const t = setTimeout(
      () => setStep(s => (s >= 6 ? 0 : s + 1)),
      STEP_MS[Math.min(step, STEP_MS.length - 1)]
    );
    return () => clearTimeout(t);
  }, [step]);

  const st = STATUS[step];

  return (
    <FeatureSection
      title="Trace Any Path Through the Codebase"
      subtitle="Deep Navigation"
      description="Ask m1nd to trace the full call path from database layer to client hook — crossing 4 architectural boundaries in a single call. Every hop is a typed graph edge. No import parsing, no file reading, no tokens burned."
      align="right"
    >
      <div
        className="w-full h-full flex flex-col"
        style={{ background: "#050510", fontFamily: '"Space Mono", "Courier New", monospace' }}
      >
        {/* Query bar */}
        <div
          className="shrink-0 px-4 py-2.5 border-b text-[11px] leading-none"
          style={{ borderColor: "#ffffff0d" }}
        >
          <span style={{ color: AMBER, opacity: 0.55 }}>› </span>
          <span style={{ color: "#ffffff50" }}>m1nd.</span>
          <span style={{ color: CYAN }}>trace</span>
          <span style={{ color: "#ffffff30" }}>(</span>
          <span style={{ color: AMBER }}>"db/queries.ts"</span>
          <span style={{ color: "#ffffff25" }}>, </span>
          <span style={{ color: CYAN }}>"client/hooks.ts"</span>
          <span style={{ color: "#ffffff30" }}>)</span>
        </div>

        {/* Graph */}
        <div className="flex-1 min-h-0">
          <TraceScene step={step} />
        </div>

        {/* Status bar */}
        <div
          className="shrink-0 px-4 py-2.5 border-t text-[11px]"
          style={{ borderColor: "#ffffff0d", minHeight: 38 }}
        >
          <AnimatePresence mode="wait">
            <motion.span
              key={step}
              initial={{ opacity: 0, y: 5 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -5 }}
              transition={{ duration: 0.18 }}
              style={{ color: st.color }}
            >
              {st.text}
            </motion.span>
          </AnimatePresence>
        </div>
      </div>
    </FeatureSection>
  );
}
