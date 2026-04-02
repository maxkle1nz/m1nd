import { useRef, useCallback, useEffect, useState } from "react";
import { useFrame } from "@react-three/fiber";
import { Html, Stars } from "@react-three/drei";
import * as THREE from "three";
import { motion, AnimatePresence } from "framer-motion";
import { Link } from "wouter";
import { NavBar } from "@/components/NavBar";
import { Footer } from "@/components/Footer";
import { GraphCanvas } from "@/components/three/GraphCanvas";
import { SEO } from "@/components/SEO";
import { ScriptLine } from "@/components/TerminalPanel";

const LEFT_COMPLETE_AT = 52000;
const RIGHT_COMPLETE_AT = 3050;
const TOTAL_DURATION = 58000;

const LEFT_SCRIPT: ScriptLine[] = [
  { startAt: 0,     text: "# Task: where is session timeout set? what breaks if I change it?", type: "dim" },
  { startAt: 400,   text: '$ grep -r "timeout" . --include="*.py"', type: "prompt" },
  { startAt: 1300,  text: "Searching 335 files...", type: "dim" },
  { startAt: 1600,  text: "./config/settings.py:45:TIMEOUT = 30", type: "output" },
  { startAt: 1800,  text: "./config/settings.py:67:REQUEST_TIMEOUT = 10", type: "output" },
  { startAt: 2000,  text: "./session/manager.py:23:session_timeout = 1800", type: "output" },
  { startAt: 2200,  text: "./cache/redis.py:12:connection_timeout = 5", type: "output" },
  { startAt: 2400,  text: "./middleware/auth.py:89:timeout_threshold = 300", type: "output" },
  { startAt: 2600,  text: "./tests/test_session.py:45:assert session.timeout == 1800", type: "output" },
  { startAt: 2800,  text: "./api/gateway.py:167:gateway_timeout = 15", type: "output" },
  { startAt: 3000,  text: "./workers/task_runner.py:33:task_timeout = 600", type: "output" },
  { startAt: 3200,  text: "./integrations/slack.py:11:request_timeout = 3", type: "output" },
  { startAt: 3400,  text: "./integrations/stripe.py:8:api_timeout = 10", type: "output" },
  { startAt: 3600,  text: "./db/connection.py:22:db_timeout = 30", type: "output" },
  { startAt: 3800,  text: "./db/connection.py:47:query_timeout = 5000", type: "output" },
  { startAt: 4000,  text: "./celery_app.py:14:task_soft_time_limit = 300", type: "output" },
  { startAt: 4200,  text: "./celery_app.py:89:worker_lost_wait_timeout = 10.0", type: "output" },
  { startAt: 4400,  text: "./notifications/email.py:19:smtp_timeout = 10", type: "output" },
  { startAt: 4600,  text: "./monitoring/healthcheck.py:7:health_check_timeout = 2", type: "output" },
  { startAt: 4800,  text: '... [+841 more matches across 23 files]', type: "dim" },
  { startAt: 5400,  text: "", type: "blank" },
  { startAt: 5500,  text: "# too many results — narrowing to session_timeout", type: "dim" },
  { startAt: 6200,  text: '$ grep -r "session_timeout" . --include="*.py"', type: "prompt" },
  { startAt: 7000,  text: "./session/manager.py:23:session_timeout = 1800", type: "output" },
  { startAt: 7200,  text: "./session/manager.py:89:    if self.session_timeout < 0:", type: "output" },
  { startAt: 7400,  text: "./session/manager.py:134:        expiry = time.time() + self.session_timeout", type: "output" },
  { startAt: 7600,  text: './config/settings.py:23:SESSION_TIMEOUT = int(os.getenv("SESSION_TIMEOUT", 1800))', type: "output" },
  { startAt: 7800,  text: "./middleware/auth.py:89:    if elapsed > session_timeout:", type: "output" },
  { startAt: 8000,  text: "./api/auth.py:55:    token_ttl = session_timeout // 2", type: "output" },
  { startAt: 8200,  text: "... [+29 more results]", type: "dim" },
  { startAt: 9000,  text: "", type: "blank" },
  { startAt: 9100,  text: "# need full context — opening main file", type: "dim" },
  { startAt: 9800,  text: "$ cat src/session/manager.py", type: "prompt" },
  { startAt: 10500, text: "# session/manager.py — 280 lines", type: "dim" },
  { startAt: 10700, text: "class SessionManager:", type: "output" },
  { startAt: 10900, text: "    def __init__(self, config, timeout=1800):", type: "output" },
  { startAt: 11100, text: "    def check_expiry(self, session_id: str) -> bool:", type: "output" },
  { startAt: 11300, text: "    def renew(self, session_id: str, delta: int) -> None:", type: "output" },
  { startAt: 11500, text: "    def invalidate(self, session_id: str) -> None:", type: "output" },
  { startAt: 11700, text: "    ...reading line 80/280...", type: "dim" },
  { startAt: 12700, text: "    ...reading line 140/280...", type: "dim" },
  { startAt: 13700, text: "    ...reading line 200/280...", type: "dim" },
  { startAt: 14700, text: "    ...reading line 260/280...", type: "dim" },
  { startAt: 15500, text: "    complete — 280 lines, ~6,200 tokens consumed", type: "dim" },
  { startAt: 16100, text: "", type: "blank" },
  { startAt: 16200, text: "$ cat config/settings.py", type: "prompt" },
  { startAt: 16900, text: "# config/settings.py — 340 lines", type: "dim" },
  { startAt: 17100, text: 'SESSION_TIMEOUT = int(os.getenv("SESSION_TIMEOUT", 1800))', type: "output" },
  { startAt: 17300, text: 'REQUEST_TIMEOUT = int(os.getenv("REQUEST_TIMEOUT", 10))', type: "output" },
  { startAt: 17500, text: 'DB_POOL_TIMEOUT = int(os.getenv("DB_POOL_TIMEOUT", 30))', type: "output" },
  { startAt: 17700, text: "    ...reading line 90/340...", type: "dim" },
  { startAt: 18700, text: "    ...reading line 180/340...", type: "dim" },
  { startAt: 19700, text: "    ...reading line 270/340...", type: "dim" },
  { startAt: 20500, text: "    complete — 340 lines, ~7,400 tokens consumed", type: "dim" },
  { startAt: 21100, text: "", type: "blank" },
  { startAt: 21200, text: "# blast radius still unknown — need to trace callers", type: "dim" },
  { startAt: 21900, text: '$ grep -r "SessionManager|from session import" . --include="*.py"', type: "prompt" },
  { startAt: 22700, text: "[12 caller sites found — need to open each one]", type: "dim" },
  { startAt: 23100, text: "$ cat api/routes.py", type: "prompt" },
  { startAt: 23800, text: "  api/routes.py — 190 lines", type: "dim" },
  { startAt: 24000, text: "    ...reading line 45/190...", type: "dim" },
  { startAt: 24700, text: "    ...reading line 120/190...", type: "dim" },
  { startAt: 25300, text: "    complete — ~4,100 tokens consumed", type: "dim" },
  { startAt: 25700, text: "$ cat api/auth.py", type: "prompt" },
  { startAt: 26400, text: "  api/auth.py — 220 lines", type: "dim" },
  { startAt: 26600, text: "    ...reading line 55/220...", type: "dim" },
  { startAt: 27400, text: "    ...reading line 150/220...", type: "dim" },
  { startAt: 28000, text: "    complete — ~4,800 tokens consumed", type: "dim" },
  { startAt: 28400, text: "$ cat workers/cleanup.py", type: "prompt" },
  { startAt: 29100, text: "  workers/cleanup.py — 110 lines", type: "dim" },
  { startAt: 29300, text: "    ...reading line 55/110...", type: "dim" },
  { startAt: 29900, text: "    complete — ~2,200 tokens consumed", type: "dim" },
  { startAt: 30200, text: "$ cat middleware/auth.py", type: "prompt" },
  { startAt: 30900, text: "  middleware/auth.py — 300 lines", type: "dim" },
  { startAt: 31100, text: "    ...reading line 60/300...", type: "dim" },
  { startAt: 32100, text: "    ...reading line 160/300...", type: "dim" },
  { startAt: 33100, text: "    ...reading line 260/300...", type: "dim" },
  { startAt: 33900, text: "    complete — ~6,600 tokens consumed", type: "dim" },
  { startAt: 34200, text: "$ cat api/webhooks.py", type: "prompt" },
  { startAt: 34900, text: "  api/webhooks.py — 175 lines", type: "dim" },
  { startAt: 35100, text: "    ...reading line 45/175...", type: "dim" },
  { startAt: 35750, text: "    ...reading line 120/175...", type: "dim" },
  { startAt: 36300, text: "    complete — ~3,800 tokens consumed", type: "dim" },
  { startAt: 36600, text: "", type: "blank" },
  { startAt: 36700, text: "⚠  context window: 74% full — 28,700/38,400 tokens", type: "warning" },
  { startAt: 37200, text: "$ cat services/billing.py", type: "prompt" },
  { startAt: 37900, text: "  services/billing.py — 260 lines", type: "dim" },
  { startAt: 38100, text: "    ...reading line 65/260...", type: "dim" },
  { startAt: 39100, text: "    ...reading line 175/260...", type: "dim" },
  { startAt: 39900, text: "    complete — ~5,700 tokens consumed", type: "dim" },
  { startAt: 40200, text: "⚠  context window: 91% full — cannot open more files", type: "warning" },
  { startAt: 40700, text: "# 5 caller files still unread — forced to skip", type: "dim" },
  { startAt: 41200, text: '$ grep -r "check_expiry|verify_session" . --include="*.py"', type: "prompt" },
  { startAt: 42000, text: "  [8 more call sites found — cannot open any]", type: "dim" },
  { startAt: 42500, text: "⚠  context window: 97% full — truncating output", type: "warning" },
  { startAt: 43000, text: "# giving up on full blast radius analysis", type: "dim" },
  { startAt: 43600, text: "", type: "blank" },
  { startAt: 43700, text: "elapsed: ~43s  ·  ~37,800 tokens consumed", type: "error" },
  { startAt: 44300, text: "tool calls: 210  ·  blast radius: STILL INCOMPLETE", type: "error" },
  { startAt: 44900, text: "callers confirmed: 7/12  ·  5 still unknown", type: "error" },
  { startAt: 45500, text: "", type: "blank" },
  { startAt: 46000, text: "# STILL RUNNING — 46.0s and counting...", type: "dim" },
  { startAt: 48000, text: "# STILL RUNNING — 48.0s and counting...", type: "dim" },
  { startAt: 50000, text: "# STILL RUNNING — 50.0s and counting...", type: "dim" },
  { startAt: 52000, text: "# STILL RUNNING — 52.0s and counting...", type: "dim" },
];

const RIGHT_SCRIPT: ScriptLine[] = [
  { startAt: 0,    text: "# Task: where is session timeout set? what breaks if I change it?", type: "dim" },
  { startAt: 250,  text: "> m1nd.seek(\"session timeout configuration\")", type: "prompt" },
  { startAt: 600,  text: "⠋ activating graph — 9,767 nodes in memory", type: "dim" },
  { startAt: 900,  text: "✓ 0.18s — 4 nodes located", type: "success" },
  { startAt: 1050, text: "  config/settings.py     SESSION_TIMEOUT=1800", type: "output" },
  { startAt: 1160, text: "  session/manager.py     check_expiry()", type: "output" },
  { startAt: 1270, text: "  middleware/auth.py      verify_session()", type: "output" },
  { startAt: 1380, text: "  tests/test_session.py  timeout assertions", type: "output" },
  { startAt: 1500, text: "", type: "separator" },
  { startAt: 1560, text: "> m1nd.impact(\"file::config/settings.py\")", type: "prompt" },
  { startAt: 1820, text: "✓ 0.001s — blast radius computed", type: "success" },
  { startAt: 1950, text: "  direct:    3 callers", type: "output" },
  { startAt: 2060, text: "  indirect:  7 downstream files", type: "output" },
  { startAt: 2200, text: "  ⚠ HIGH RISK: session invalidation cascade", type: "warning" },
  { startAt: 2320, text: "", type: "separator" },
  { startAt: 2380, text: "> m1nd.surgical_context_v2(\"config/settings.py\", radius=2)", type: "prompt" },
  { startAt: 2640, text: "✓ 0.12s — surgical context assembled", type: "success" },
  { startAt: 2780, text: "  3 files loaded  ·  callers, callees, tests attached", type: "output" },
  { startAt: 2900, text: "", type: "separator" },
  { startAt: 2960, text: "Finished in 0.30s  ·  3 tool calls  ·  84% token savings vs grep", type: "success" },
];

const SLOW_CL = 24;

const BEAT_DEFS = [
  {
    animStart: 0,
    realMs: 0,
    step: 1,
    title: "Query Parsed",
    desc: "The query string is tokenized and scored against pre-computed node embeddings already loaded in RAM. The entire codebase graph — 9,767 nodes and 26,557 edges — is resident in memory. Zero files opened. Zero disk I/O.",
    tech: "PageRank scores + TF-IDF vectors · loaded in < 1μs",
    color: "#00f5ff",
  },
  {
    animStart: 3,
    realMs: 20,
    step: 2,
    title: "Seeds Selected",
    desc: "The top-k candidate nodes are scored against the query using cosine distance on 128-dimensional embeddings. The 4 highest-ranked nodes light up as 'seeds' — these are the starting points for spreading activation. Everything else stays dark.",
    tech: "k-nearest neighbors · 128-dim embedding space · 4 seeds chosen",
    color: "#ffb700",
  },
  {
    animStart: 7,
    realMs: 60,
    step: 3,
    title: "Activation Wave",
    desc: "A spreading activation wave fires outward from each seed, following typed edges in the knowledge graph across 4 hops. Every connected node is scored based on its distance from the query seeds and its PageRank weight. 120 nodes are evaluated.",
    tech: "BFS traversal · 4 hops · 120 nodes scored · additive activation",
    color: "#4488ff",
  },
  {
    animStart: 12,
    realMs: 120,
    step: 4,
    title: "116 Nodes Pruned",
    desc: "Nodes that fall below the activation threshold are eliminated one by one. 116 of the 120 candidates don't make the cut — they fade to near-invisible. The graph collapses from noise to signal. Only 4 nodes survive.",
    tech: "Composite score = PageRank × activation × edge weight · 96.7% eliminated",
    color: "#ff6644",
  },
  {
    animStart: 17,
    realMs: 160,
    step: 5,
    title: "4 Winners Emerge",
    desc: "The 4 surviving nodes bloom to full brightness. Caller/callee chains, test references, and documentation links are automatically attached. The surgical context is assembled — no padding, no noise, no guessing.",
    tech: "Surgical context assembly · callers + callees + tests · 0 unnecessary reads",
    color: "#00ff88",
  },
  {
    animStart: 20,
    realMs: 180,
    step: 6,
    title: "Result Returned",
    desc: "The context is serialized and returned to the agent. Total elapsed time: 0.18 seconds. Zero files opened. Zero tokens consumed. The agent now has exactly the 4 nodes that matter — their code, relationships, and blast radius.",
    tech: "0.18s · 0 files opened · 0 tokens consumed · 4 nodes returned",
    color: "#00f5ff",
  },
];

function ActivationWave({ startAt, maxR, color, delay = 0 }: { startAt: number; maxR: number; color: string; delay?: number }) {
  const ref = useRef<THREE.Mesh>(null);
  useFrame(({ clock }) => {
    if (!ref.current) return;
    const t = clock.getElapsedTime();
    const phase = t % SLOW_CL;
    const wt = ((phase - startAt + SLOW_CL) % 4) / 4;
    const eased = 1 - Math.pow(1 - Math.min(wt, 1), 2);
    if (phase >= startAt && phase < startAt + 4) {
      ref.current.scale.set(eased * maxR, eased * maxR, 1);
      (ref.current.material as THREE.MeshBasicMaterial).opacity = (1 - eased) * 0.35;
    } else {
      (ref.current.material as THREE.MeshBasicMaterial).opacity = 0;
    }
  });
  return (
    <mesh ref={ref}>
      <ringGeometry args={[0.8, 1, 64]} />
      <meshBasicMaterial color={color} transparent opacity={0} side={THREE.DoubleSide} blending={THREE.AdditiveBlending} depthWrite={false} />
    </mesh>
  );
}

function QueryPulse() {
  const ref = useRef<THREE.Mesh>(null);
  useFrame(({ clock }) => {
    if (!ref.current) return;
    const t = clock.getElapsedTime() % SLOW_CL;
    const pt = (t % 2) / 2;
    ref.current.scale.set(1 + pt * 14, 1 + pt * 14, 1);
    (ref.current.material as THREE.MeshBasicMaterial).opacity = (1 - pt) * 0.5;
  });
  return (
    <mesh ref={ref}>
      <ringGeometry args={[0.3, 0.4, 48]} />
      <meshBasicMaterial color="#00f5ff" transparent opacity={0} side={THREE.DoubleSide} blending={THREE.AdditiveBlending} depthWrite={false} />
    </mesh>
  );
}

interface SlowNodeProps {
  position: [number, number, number];
  label: string;
  sublabel?: string;
  baseColor: string;
  winColor: string;
  activateAt: number;
  pruneAt: number;
  winAt: number;
  showLabel?: boolean;
}

function SlowWinnerNode({ position, label, sublabel, winColor, activateAt, winAt }: SlowNodeProps) {
  const coreRef = useRef<THREE.Mesh>(null);
  const glow1Ref = useRef<THREE.Mesh>(null);
  const glow2Ref = useRef<THREE.Mesh>(null);
  const groupRef = useRef<THREE.Group>(null);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime() % SLOW_CL;
    let brightness = 0.1;
    let color = new THREE.Color("#334466");

    if (t < activateAt) {
      brightness = 0.05;
      color.set("#223");
    } else if (t < winAt) {
      const prog = (t - activateAt) / (winAt - activateAt);
      brightness = 0.15 + prog * 0.35;
      color.setStyle("#ffb700");
    } else if (t < winAt + 3) {
      const prog = (t - winAt) / 3;
      brightness = 0.5 + prog * 1.5;
      color.setStyle(winColor);
    } else {
      brightness = 2.0;
      color.setStyle(winColor);
    }

    const pulse = 1 + Math.sin(clock.getElapsedTime() * 3.7) * 0.04;
    if (groupRef.current) groupRef.current.scale.setScalar(pulse);
    if (coreRef.current) {
      (coreRef.current.material as THREE.MeshBasicMaterial).color = color.clone().multiplyScalar(brightness);
    }
    if (glow1Ref.current) {
      (glow1Ref.current.material as THREE.MeshBasicMaterial).color = color.clone().multiplyScalar(brightness * 0.5);
      (glow1Ref.current.material as THREE.MeshBasicMaterial).opacity = brightness * 0.4;
    }
    if (glow2Ref.current) {
      (glow2Ref.current.material as THREE.MeshBasicMaterial).color = color.clone().multiplyScalar(brightness * 0.2);
      (glow2Ref.current.material as THREE.MeshBasicMaterial).opacity = brightness * 0.2;
    }
  });

  return (
    <group position={position}>
      <group ref={groupRef}>
        <mesh ref={coreRef}>
          <sphereGeometry args={[0.28, 24, 24]} />
          <meshBasicMaterial color="#223" />
        </mesh>
        <mesh ref={glow1Ref}>
          <sphereGeometry args={[0.55, 16, 16]} />
          <meshBasicMaterial color="#223" transparent opacity={0} blending={THREE.AdditiveBlending} depthWrite={false} />
        </mesh>
        <mesh ref={glow2Ref}>
          <sphereGeometry args={[1.1, 12, 12]} />
          <meshBasicMaterial color="#223" transparent opacity={0} blending={THREE.AdditiveBlending} depthWrite={false} />
        </mesh>
      </group>
      <Html center distanceFactor={14} zIndexRange={[2, 0]} style={{ pointerEvents: "none" }}>
        <div style={{
          background: "rgba(5,5,22,0.92)",
          border: `1px solid ${winColor}44`,
          borderRadius: "4px",
          padding: "3px 8px",
          fontSize: "11px",
          fontFamily: '"Space Mono", monospace',
          color: winColor,
          whiteSpace: "nowrap",
          transform: "translateY(-30px)",
          pointerEvents: "none",
        }}>
          {label}
          {sublabel && <div style={{ color: "#5577aa", fontSize: "9px", marginTop: "1px" }}>{sublabel}</div>}
        </div>
      </Html>
    </group>
  );
}

function SlowCandidateNode({ position, activateAt, pruneAt }: { position: [number, number, number]; activateAt: number; pruneAt: number }) {
  const ref = useRef<THREE.Mesh>(null);
  useFrame(({ clock }) => {
    if (!ref.current) return;
    const t = clock.getElapsedTime() % SLOW_CL;
    let opacity = 0.05;
    if (t >= activateAt && t < activateAt + 1.5) {
      const prog = (t - activateAt) / 1.5;
      opacity = prog < 0.5 ? prog * 0.6 : (1 - prog) * 0.6;
    } else if (t >= pruneAt) {
      opacity = 0.04;
    } else if (t >= activateAt) {
      opacity = 0.12;
    }
    (ref.current.material as THREE.MeshBasicMaterial).opacity = opacity;
  });
  return (
    <mesh ref={ref} position={position}>
      <sphereGeometry args={[0.18, 12, 12]} />
      <meshBasicMaterial color="#00f5ff" transparent opacity={0.05} blending={THREE.AdditiveBlending} depthWrite={false} />
    </mesh>
  );
}

function SlowWeakNode({ position, flashAt }: { position: [number, number, number]; flashAt: number }) {
  const ref = useRef<THREE.Mesh>(null);
  useFrame(({ clock }) => {
    if (!ref.current) return;
    const t = clock.getElapsedTime() % SLOW_CL;
    let opacity = 0.02;
    if (t >= flashAt && t < flashAt + 0.8) {
      const prog = (t - flashAt) / 0.8;
      opacity = Math.sin(prog * Math.PI) * 0.25;
    }
    (ref.current.material as THREE.MeshBasicMaterial).opacity = opacity;
  });
  return (
    <mesh ref={ref} position={position}>
      <sphereGeometry args={[0.1, 8, 8]} />
      <meshBasicMaterial color="#4488ff" transparent opacity={0.02} blending={THREE.AdditiveBlending} depthWrite={false} />
    </mesh>
  );
}

function SlowMoCam() {
  const waypoints = [
    { t: 0,    pos: new THREE.Vector3(4, 18, 28),   look: new THREE.Vector3(1.8, 0, 0) },
    { t: 3,    pos: new THREE.Vector3(3, 10, 20),   look: new THREE.Vector3(1.8, 0, 0) },
    { t: 7,    pos: new THREE.Vector3(-2, 14, 26),  look: new THREE.Vector3(1.8, 0, 0) },
    { t: 12,   pos: new THREE.Vector3(-8, 6, 20),   look: new THREE.Vector3(1.8, 0, 0) },
    { t: 17,   pos: new THREE.Vector3(2, 4, 14),    look: new THREE.Vector3(1.8, 0.5, 0) },
    { t: 20,   pos: new THREE.Vector3(3, 2, 10),    look: new THREE.Vector3(1.8, 0.5, 0) },
    { t: 24,   pos: new THREE.Vector3(4, 18, 28),   look: new THREE.Vector3(1.8, 0, 0) },
  ];

  useFrame(({ clock, camera }) => {
    const t = clock.getElapsedTime() % SLOW_CL;
    let i = 0;
    for (let j = 0; j < waypoints.length - 1; j++) {
      if (t >= waypoints[j].t && t < waypoints[j + 1].t) { i = j; break; }
    }
    const a = waypoints[i];
    const b = waypoints[i + 1];
    const span = b.t - a.t;
    const raw = (t - a.t) / span;
    const alpha = raw < 0.5 ? 2 * raw * raw : -1 + (4 - 2 * raw) * raw;

    camera.position.lerpVectors(a.pos, b.pos, alpha);
    const tgt = new THREE.Vector3().lerpVectors(a.look, b.look, alpha);
    camera.lookAt(tgt);
  });
  return null;
}

function QueryCenterLabel() {
  const ref = useRef<HTMLDivElement>(null);
  useFrame(({ clock }) => {
    if (!ref.current) return;
    const t = clock.getElapsedTime() % SLOW_CL;
    const alpha = t < 1 ? t : t < 2.5 ? 1 : Math.max(0, 1 - (t - 2.5) * 1.5);
    ref.current.style.opacity = String(alpha);
  });
  return (
    <Html center position={[1.8, 3.5, 0]} distanceFactor={14} zIndexRange={[5, 0]} style={{ pointerEvents: "none" }}>
      <div ref={ref} style={{
        opacity: 0,
        fontFamily: '"Space Mono", monospace',
        fontSize: "12px",
        color: "#00f5ff",
        background: "rgba(0,245,255,0.06)",
        border: "1px solid rgba(0,245,255,0.2)",
        borderRadius: "4px",
        padding: "3px 8px",
        whiteSpace: "nowrap",
        pointerEvents: "none",
      }}>
        m1nd.seek("session timeout")
      </div>
    </Html>
  );
}

function SlowMotionScene() {
  const winners: Array<{ pos: [number, number, number]; label: string; sub: string; color: string }> = [
    { pos: [0, 0, 0],     label: "config/settings.py",     sub: "SESSION_TIMEOUT=1800", color: "#00f5ff" },
    { pos: [3.5, 0.5, 0], label: "session/manager.py",     sub: "check_expiry()",       color: "#00f5ff" },
    { pos: [1.8, 2.8, 0.5],  label: "middleware/auth.py",  sub: "verify_session()",     color: "#00ff88" },
    { pos: [1.8, -2.8, -0.5], label: "tests/test_session.py", sub: "timeout assertions", color: "#00ff88" },
  ];

  const candidates: Array<[number, number, number]> = [
    [-3.5, 2, 0], [-3.5, -1.5, 0.5], [5.5, 2.5, -0.5], [5.5, -2, 0.5],
    [-1.5, 4.5, 0], [2.5, 4.5, 0.5], [-1, -4.5, 0], [2.5, -4.5, -0.5],
  ];

  const weakNodes: Array<[number, number, number]> = [
    [-7, 3, 1], [-6, -3, -1], [8, 3, 0.5], [7, -3, -0.5],
    [-2, 7, -1], [4, 7, 0], [-3, -7, 0.5], [4, -7, 1],
    [0, 0, 8], [0, 0, -8], [8, 0, 4], [-8, 0, 3],
    [-5, 5, 4], [5, 5, -4], [-5, -5, -3], [5, -5, 4],
  ];

  return (
    <GraphCanvas cameraPos={[4, 18, 28]}>
      <Stars radius={90} depth={60} count={4000} factor={4} saturation={0} fade speed={0.2} />
      <SlowMoCam />
      <QueryCenterLabel />

      <QueryPulse />
      <ActivationWave startAt={7}   maxR={14} color="#4488ff" />
      <ActivationWave startAt={7.6} maxR={18} color="#2244aa" />
      <ActivationWave startAt={8.2} maxR={24} color="#112266" />

      {winners.map((w, i) => (
        <SlowWinnerNode
          key={i}
          position={w.pos}
          label={w.label}
          sublabel={w.sub}
          baseColor="#334"
          winColor={w.color}
          activateAt={3}
          pruneAt={999}
          winAt={17}
          showLabel
        />
      ))}

      {candidates.map((pos, i) => (
        <SlowCandidateNode key={i} position={pos} activateAt={7 + i * 0.12} pruneAt={12 + i * 0.45} />
      ))}

      {weakNodes.map((pos, i) => (
        <SlowWeakNode key={i} position={pos} flashAt={7.5 + (i % 6) * 0.18} />
      ))}
    </GraphCanvas>
  );
}

function SlowMotionSection() {
  const [beatIdx, setBeatIdx] = useState(0);
  const startRef = useRef(Date.now());

  useEffect(() => {
    startRef.current = Date.now();
    const id = setInterval(() => {
      const elapsed = (Date.now() - startRef.current) / 1000;
      const phase = elapsed % SLOW_CL;
      let next = 0;
      for (let i = BEAT_DEFS.length - 1; i >= 0; i--) {
        if (phase >= BEAT_DEFS[i].animStart) { next = i; break; }
      }
      setBeatIdx(next);
    }, 80);
    return () => clearInterval(id);
  }, []);

  const beat = BEAT_DEFS[beatIdx];

  return (
    <div className="py-24 border-b border-border/20 relative overflow-hidden">
      <div className="absolute inset-0 pointer-events-none" style={{ background: "radial-gradient(ellipse at 50% 30%, rgba(0,245,255,0.04), transparent 65%)" }} />
      <div className="container mx-auto px-6 relative z-10">

        <div className="text-center mb-12">
          <motion.div initial={{ opacity: 0, y: 16 }} whileInView={{ opacity: 1, y: 0 }} viewport={{ once: true }} transition={{ duration: 0.7 }}>
            <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-6">
              100× slow motion
            </div>
            <h2 className="text-3xl md:text-5xl font-bold font-sans tracking-tight mb-4">
              What happened inside
              <br />
              <span className="text-transparent bg-clip-text bg-gradient-to-r from-primary to-secondary">
                those 0.18 seconds?
              </span>
            </h2>
            <p className="text-lg text-muted-foreground max-w-xl mx-auto">
              180ms dilated to 24 seconds. Every step of the graph traversal, visible in real time.
            </p>
          </motion.div>
        </div>

        <div className="relative rounded-xl overflow-hidden border border-primary/20 shadow-[0_0_60px_rgba(0,245,255,0.06)] mb-4 hidden md:block" style={{ height: "520px" }}>
          <SlowMotionScene />
        </div>

        <div className="md:hidden mb-4 rounded-xl border border-primary/10 overflow-hidden">
          {BEAT_DEFS.map((b, i) => (
            <div
              key={i}
              className="flex items-start gap-4 px-4 py-3 border-b border-border/10 last:border-b-0 transition-all duration-300"
              style={beatIdx === i ? { background: `${b.color}10` } : {}}
            >
              <div className="flex-shrink-0 font-mono text-xs font-bold w-12 text-right pt-0.5" style={{ color: b.color }}>
                {b.realMs}ms
              </div>
              <div className="flex-1">
                <div className="font-sans text-sm font-semibold mb-0.5" style={{ color: beatIdx === i ? b.color : "rgba(226,232,240,0.5)" }}>
                  {b.title}
                </div>
                {beatIdx === i && (
                  <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: "auto" }}
                    className="text-xs text-muted-foreground/70 leading-relaxed"
                  >
                    {b.tech}
                  </motion.div>
                )}
              </div>
              {beatIdx === i && (
                <div className="flex-shrink-0 w-1.5 h-1.5 rounded-full mt-1.5" style={{ background: b.color }} />
              )}
            </div>
          ))}
        </div>

        <div className="flex gap-1.5 mb-4">
          {BEAT_DEFS.map((b, i) => (
            <div
              key={i}
              className="flex-1 py-2.5 px-1 rounded border text-center transition-all duration-300 cursor-default"
              style={beatIdx === i ? {
                background: `${b.color}12`,
                borderColor: `${b.color}40`,
              } : {
                background: "transparent",
                borderColor: "rgba(148,163,184,0.08)",
              }}
            >
              <div
                className="font-mono text-[9px] font-bold mb-0.5 tabular-nums"
                style={{ color: beatIdx === i ? b.color : "rgba(148,163,184,0.3)" }}
              >
                t={b.realMs}ms
              </div>
              <div
                className="font-mono text-[8px] leading-tight hidden sm:block"
                style={{ color: beatIdx === i ? "rgba(226,232,240,0.7)" : "rgba(148,163,184,0.2)" }}
              >
                {b.title}
              </div>
              {beatIdx === i && (
                <div className="w-full h-0.5 mt-1.5 rounded-full" style={{ background: b.color }} />
              )}
            </div>
          ))}
        </div>

        <AnimatePresence mode="wait">
          <motion.div
            key={beatIdx}
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={{ duration: 0.3 }}
            className="rounded-xl border p-6 lg:p-8"
            style={{ borderColor: `${beat.color}22`, background: `${beat.color}06` }}
          >
            <div className="flex flex-col lg:flex-row lg:items-start gap-6">
              <div className="flex-shrink-0 flex lg:flex-col items-center lg:items-start gap-3 lg:gap-1 lg:min-w-[100px]">
                <div className="font-mono text-[10px] text-muted-foreground/40 uppercase tracking-widest">
                  Step {beat.step} / 6
                </div>
                <div
                  className="font-mono text-3xl lg:text-4xl font-bold tabular-nums leading-none"
                  style={{ color: beat.color }}
                >
                  {beat.realMs}ms
                </div>
              </div>
              <div className="flex-1">
                <h3 className="text-xl font-bold mb-3" style={{ color: beat.color }}>
                  {beat.title}
                </h3>
                <p className="text-muted-foreground leading-relaxed mb-4">
                  {beat.desc}
                </p>
                <div
                  className="inline-block font-mono text-[10px] text-muted-foreground/50 bg-background/60 rounded px-3 py-2 border border-border/20"
                >
                  {beat.tech}
                </div>
              </div>
            </div>
          </motion.div>
        </AnimatePresence>

      </div>
    </div>
  );
}

/* ─── Demo Comparison ─────────────────────────────────────────────────────── */

type LT = "prompt" | "output" | "success" | "warning" | "dim" | "blank" | "error";
interface DL { at: number; type: LT; text: string; }

const DC: Record<LT, string> = {
  prompt:  "#e2e8f0",
  output:  "#7a8faa",
  success: "#00ff88",
  warning: "#ffb700",
  error:   "#ff00aa",
  dim:     "#2e3f52",
  blank:   "transparent",
};

const DPH = 560;  // panel height
const DHH = 40;   // dots header height
const DSH = 34;   // status bar height
const DBH = DPH - DHH - DSH; // body height = 486

function DemoTerminal({
  title, subtitle, accent, lines, doneAt, elapsed, isLeft,
}: {
  title: string; subtitle: string; accent: string;
  lines: DL[]; doneAt: number | null; elapsed: number; isLeft?: boolean;
}) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const visible = lines.filter(l => l.at <= elapsed);
  const isDone     = doneAt != null && elapsed >= doneAt;
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
      height: DPH, display: "flex", flexDirection: "column",
      borderRadius: 12, border: `1px solid ${accent}28`,
      background: "#05050f", overflow: "hidden",
    }}>
      {/* dots bar */}
      <div style={{
        height: DHH, flexShrink: 0, background: "#080818",
        borderBottom: `1px solid ${accent}18`,
        display: "flex", alignItems: "center", gap: 8, padding: "0 16px",
      }}>
        <div style={{ display: "flex", gap: 6, flexShrink: 0 }}>
          <div style={{ width: 10, height: 10, borderRadius: "50%", background: "rgba(239,68,68,0.5)" }} />
          <div style={{ width: 10, height: 10, borderRadius: "50%", background: "rgba(234,179,8,0.5)" }} />
          <div style={{ width: 10, height: 10, borderRadius: "50%", background: "rgba(34,197,94,0.5)" }} />
        </div>
        <span style={{ flex: 1, textAlign: "center", fontSize: 11, fontFamily: "Space Mono, monospace", color: `${accent}88` }}>
          {title}
        </span>
      </div>

      {/* status bar */}
      <div style={{
        height: DSH, flexShrink: 0, background: "#060614",
        borderBottom: `1px solid ${accent}12`,
        display: "flex", alignItems: "center", justifyContent: "space-between", padding: "0 16px",
      }}>
        <span style={{ fontSize: 10, fontFamily: "Space Mono, monospace", color: "rgba(148,163,184,0.4)" }}>
          {subtitle}
        </span>
        {hasStarted && (
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 10, fontFamily: "Space Mono, monospace", color: isDone ? "#00ff88" : "rgba(148,163,184,0.45)" }}>
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
                animation: "dc-pulse 2s ease-in-out infinite",
              }}>
                {isLeft ? "STILL RUNNING…" : "RUNNING"}
              </span>
            )}
          </div>
        )}
      </div>

      {/* scroll body */}
      <div
        ref={bodyRef}
        style={{
          height: DBH, overflowY: "auto", overflowX: "hidden",
          scrollbarWidth: "none" as const, scrollBehavior: "smooth" as const,
          padding: 16, fontFamily: "Space Mono, monospace",
          fontSize: 11, lineHeight: "20px", wordBreak: "break-word" as const,
        }}
      >
        {visible.map((line, i) =>
          line.type === "blank" ? (
            <div key={i} style={{ height: 6 }} />
          ) : (
            <div key={i} style={{ color: DC[line.type] }}>
              {line.type !== "prompt" ? "\u00A0\u00A0" : ""}{line.text}
            </div>
          )
        )}
        {hasStarted && !isDone && (
          <span style={{
            display: "inline-block", width: 6, height: 13, marginLeft: 2,
            background: accent, animation: "dc-blink 1s step-end infinite",
          }} />
        )}
      </div>
    </div>
  );
}

const DC_LEFT: DL[] = [
  { at: 0,     type: "dim",     text: "# Task: where is session timeout set? what breaks if I change it?" },
  { at: 400,   type: "prompt",  text: '$ grep -r "timeout" . --include="*.py"' },
  { at: 1300,  type: "dim",     text: "Searching 335 files..." },
  { at: 1600,  type: "output",  text: "./config/settings.py:45:TIMEOUT = 30" },
  { at: 1800,  type: "output",  text: "./config/settings.py:67:REQUEST_TIMEOUT = 10" },
  { at: 2000,  type: "output",  text: "./session/manager.py:23:session_timeout = 1800" },
  { at: 2200,  type: "output",  text: "./cache/redis.py:12:connection_timeout = 5" },
  { at: 2400,  type: "output",  text: "./middleware/auth.py:89:timeout_threshold = 300" },
  { at: 2600,  type: "output",  text: "./tests/test_session.py:45:assert session.timeout == 1800" },
  { at: 2800,  type: "output",  text: "./api/gateway.py:167:gateway_timeout = 15" },
  { at: 3000,  type: "output",  text: "./workers/task_runner.py:33:task_timeout = 600" },
  { at: 3200,  type: "output",  text: "./integrations/slack.py:11:request_timeout = 3" },
  { at: 3400,  type: "output",  text: "./integrations/stripe.py:8:api_timeout = 10" },
  { at: 3600,  type: "output",  text: "./db/connection.py:22:db_timeout = 30" },
  { at: 3800,  type: "output",  text: "./db/connection.py:47:query_timeout = 5000" },
  { at: 4000,  type: "output",  text: "./celery_app.py:14:task_soft_time_limit = 300" },
  { at: 4200,  type: "output",  text: "./celery_app.py:89:worker_lost_wait_timeout = 10.0" },
  { at: 4400,  type: "output",  text: "./notifications/email.py:19:smtp_timeout = 10" },
  { at: 4600,  type: "output",  text: "./monitoring/healthcheck.py:7:health_check_timeout = 2" },
  { at: 4800,  type: "warning", text: "... [+841 more matches across 23 files]" },
  { at: 5500,  type: "dim",     text: "# too many results — narrowing to session_timeout" },
  { at: 6200,  type: "prompt",  text: '$ grep -r "session_timeout" . --include="*.py"' },
  { at: 7000,  type: "output",  text: "./session/manager.py:23:session_timeout = 1800" },
  { at: 7200,  type: "output",  text: "./session/manager.py:89:    if self.session_timeout < 0:" },
  { at: 7400,  type: "output",  text: "./session/manager.py:134:        expiry = time.time() + self.session_timeout" },
  { at: 7600,  type: "output",  text: './config/settings.py:23:SESSION_TIMEOUT = int(os.getenv("SESSION_TIMEOUT", 1800))' },
  { at: 7800,  type: "output",  text: "./middleware/auth.py:89:    if elapsed > session_timeout:" },
  { at: 8000,  type: "output",  text: "./api/auth.py:55:    token_ttl = session_timeout // 2" },
  { at: 8200,  type: "dim",     text: "... [+29 more results]" },
  { at: 9000,  type: "dim",     text: "# need full context — opening main file" },
  { at: 9800,  type: "prompt",  text: "$ cat src/session/manager.py" },
  { at: 10500, type: "dim",     text: "# session/manager.py — 280 lines" },
  { at: 10700, type: "output",  text: "class SessionManager:" },
  { at: 10900, type: "output",  text: "    def __init__(self, config, timeout=1800):" },
  { at: 11100, type: "output",  text: "    def check_expiry(self, session_id: str) -> bool:" },
  { at: 11300, type: "output",  text: "    def renew(self, session_id: str, delta: int) -> None:" },
  { at: 11500, type: "output",  text: "    def invalidate(self, session_id: str) -> None:" },
  { at: 11700, type: "dim",     text: "    ...reading line 80/280..." },
  { at: 12700, type: "dim",     text: "    ...reading line 140/280..." },
  { at: 13700, type: "dim",     text: "    ...reading line 200/280..." },
  { at: 14700, type: "dim",     text: "    ...reading line 260/280..." },
  { at: 15500, type: "dim",     text: "    complete — 280 lines, ~6,200 tokens consumed" },
  { at: 16200, type: "prompt",  text: "$ cat config/settings.py" },
  { at: 16900, type: "dim",     text: "# config/settings.py — 340 lines" },
  { at: 17100, type: "output",  text: 'SESSION_TIMEOUT = int(os.getenv("SESSION_TIMEOUT", 1800))' },
  { at: 17300, type: "output",  text: 'REQUEST_TIMEOUT = int(os.getenv("REQUEST_TIMEOUT", 10))' },
  { at: 17500, type: "output",  text: 'DB_POOL_TIMEOUT = int(os.getenv("DB_POOL_TIMEOUT", 30))' },
  { at: 17700, type: "dim",     text: "    ...reading line 90/340..." },
  { at: 18700, type: "dim",     text: "    ...reading line 180/340..." },
  { at: 19700, type: "dim",     text: "    ...reading line 270/340..." },
  { at: 20500, type: "dim",     text: "    complete — 340 lines, ~7,400 tokens consumed" },
  { at: 21200, type: "dim",     text: "# blast radius still unknown — need to trace callers" },
  { at: 21900, type: "prompt",  text: '$ grep -r "SessionManager" . --include="*.py"' },
  { at: 22700, type: "dim",     text: "  12 caller sites found — need to open each one" },
  { at: 23100, type: "prompt",  text: "$ cat api/routes.py" },
  { at: 23800, type: "dim",     text: "  api/routes.py — 190 lines" },
  { at: 24000, type: "dim",     text: "    ...reading line 45/190..." },
  { at: 24700, type: "dim",     text: "    ...reading line 120/190..." },
  { at: 25300, type: "dim",     text: "    complete — ~4,100 tokens consumed" },
  { at: 25700, type: "prompt",  text: "$ cat api/auth.py" },
  { at: 26400, type: "dim",     text: "  api/auth.py — 220 lines" },
  { at: 26600, type: "dim",     text: "    ...reading line 55/220..." },
  { at: 27400, type: "dim",     text: "    ...reading line 150/220..." },
  { at: 28000, type: "dim",     text: "    complete — ~4,800 tokens consumed" },
  { at: 28400, type: "prompt",  text: "$ cat workers/cleanup.py" },
  { at: 29100, type: "dim",     text: "  workers/cleanup.py — 110 lines" },
  { at: 29300, type: "dim",     text: "    ...reading line 55/110..." },
  { at: 29900, type: "dim",     text: "    complete — ~2,200 tokens consumed" },
  { at: 30200, type: "prompt",  text: "$ cat middleware/auth.py" },
  { at: 30900, type: "dim",     text: "  middleware/auth.py — 300 lines" },
  { at: 31100, type: "dim",     text: "    ...reading line 60/300..." },
  { at: 32100, type: "dim",     text: "    ...reading line 160/300..." },
  { at: 33100, type: "dim",     text: "    ...reading line 260/300..." },
  { at: 33900, type: "dim",     text: "    complete — ~6,600 tokens consumed" },
  { at: 34200, type: "prompt",  text: "$ cat api/webhooks.py" },
  { at: 34900, type: "dim",     text: "  api/webhooks.py — 175 lines" },
  { at: 35100, type: "dim",     text: "    ...reading line 45/175..." },
  { at: 35750, type: "dim",     text: "    ...reading line 120/175..." },
  { at: 36300, type: "dim",     text: "    complete — ~3,800 tokens consumed" },
  { at: 36600, type: "warning", text: "⚠  context window: 74% full — 28,700/38,400 tokens" },
  { at: 37200, type: "prompt",  text: "$ cat services/billing.py" },
  { at: 37900, type: "dim",     text: "  services/billing.py — 260 lines" },
  { at: 38100, type: "dim",     text: "    ...reading line 65/260..." },
  { at: 39100, type: "dim",     text: "    ...reading line 175/260..." },
  { at: 39900, type: "dim",     text: "    complete — ~5,700 tokens consumed" },
  { at: 40200, type: "warning", text: "⚠  context window: 91% full — cannot open more files" },
  { at: 40700, type: "dim",     text: "  5 caller files still unread — forced to skip" },
  { at: 41200, type: "prompt",  text: '$ grep -r "check_expiry|verify_session" . --include="*.py"' },
  { at: 42000, type: "dim",     text: "  [8 more call sites found — cannot open any]" },
  { at: 42500, type: "warning", text: "⚠  context window: 97% full — truncating output" },
  { at: 43100, type: "dim",     text: "  giving up on full blast radius analysis" },
  { at: 43700, type: "error",   text: "elapsed: ~43s  ·  ~37,800 tokens consumed" },
  { at: 44300, type: "error",   text: "tool calls: 210  ·  blast radius: STILL INCOMPLETE" },
  { at: 44900, type: "error",   text: "callers confirmed: 7/12  ·  5 still unknown" },
  { at: 46000, type: "dim",     text: "# STILL RUNNING — 46.0s and counting..." },
  { at: 48000, type: "dim",     text: "# STILL RUNNING — 48.0s and counting..." },
  { at: 50000, type: "dim",     text: "# STILL RUNNING — 50.0s and counting..." },
  { at: 52000, type: "dim",     text: "# STILL RUNNING — 52.0s and counting..." },
  { at: 54000, type: "dim",     text: "# STILL RUNNING — 54.0s and counting..." },
];

const DC_RIGHT: DL[] = [
  { at: 0,    type: "dim",     text: "# Task: where is session timeout set? what breaks if I change it?" },
  { at: 250,  type: "prompt",  text: '> m1nd.seek("session timeout configuration")' },
  { at: 600,  type: "dim",     text: "⠋ activating graph — 9,767 nodes in memory" },
  { at: 900,  type: "success", text: "✓ 0.18s — 4 nodes located" },
  { at: 1050, type: "output",  text: "  config/settings.py     SESSION_TIMEOUT=1800" },
  { at: 1160, type: "output",  text: "  session/manager.py     check_expiry()" },
  { at: 1270, type: "output",  text: "  middleware/auth.py      verify_session()" },
  { at: 1380, type: "output",  text: "  tests/test_session.py  timeout assertions" },
  { at: 1500, type: "blank",   text: "" },
  { at: 1560, type: "prompt",  text: '> m1nd.impact("file::config/settings.py")' },
  { at: 1820, type: "success", text: "✓ 0.001s — blast radius computed" },
  { at: 1950, type: "output",  text: "  direct:    3 callers" },
  { at: 2060, type: "output",  text: "  indirect:  7 downstream files" },
  { at: 2200, type: "warning", text: "  △ HIGH RISK: session invalidation cascade" },
  { at: 2320, type: "blank",   text: "" },
  { at: 2380, type: "prompt",  text: '> m1nd.surgical_context_v2("config/settings.py", radius=2)' },
  { at: 2640, type: "success", text: "✓ 0.12s — surgical context assembled" },
  { at: 2780, type: "output",  text: "  3 files loaded  ·  callers, callees, tests attached" },
  { at: 2900, type: "blank",   text: "" },
  { at: 2960, type: "success", text: "Finished in 0.30s  ·  3 tool calls  ·  84% token savings" },
];

const DC_RIGHT_DONE = 3050;
const DC_TOTAL      = 58000;

function DemoComparison() {
  const [elapsed, setElapsed] = useState(0);
  const [phase, setPhase] = useState<"idle" | "running" | "done">("idle");
  const rafRef = useRef<number>(0);
  const t0Ref  = useRef<number>(0);

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
      if (e < DC_TOTAL) {
        rafRef.current = requestAnimationFrame(tick);
      } else {
        setElapsed(DC_TOTAL);
        setPhase("done");
      }
    };
    rafRef.current = requestAnimationFrame(tick);
  }, [stop]);

  // auto-start on page load — no useInView needed, user is here intentionally
  useEffect(() => {
    const t = setTimeout(play, 600);
    return () => { clearTimeout(t); stop(); };
  }, []);

  const isDone    = phase === "done";
  const hasRun    = elapsed > 0;

  return (
    <div>
      <style>{`
        @keyframes dc-blink { 50% { opacity: 0; } }
        @keyframes dc-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
      `}</style>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3 lg:gap-4">
        <div className="order-2 md:order-1">
          <DemoTerminal
            title="Traditional Agent — grep/glob/cat"
            subtitle="codex-cli · bash tools"
            accent="#ff4444"
            lines={DC_LEFT}
            doneAt={null}
            elapsed={elapsed}
            isLeft
          />
        </div>
        <div className="order-1 md:order-2">
          <DemoTerminal
            title="Agent + m1nd — graph-first"
            subtitle="m1nd-mcp · Rust · in-memory"
            accent="#00f5ff"
            lines={DC_RIGHT}
            doneAt={DC_RIGHT_DONE}
            elapsed={elapsed}
          />
        </div>
      </div>

      <p className="text-center text-[10px] font-mono text-muted-foreground/30 mt-2 md:hidden">
        m1nd finishes first — scroll down to see the full run
      </p>

      {/* stats summary — fades in when both sides have their first result */}
      {hasRun && (
        <motion.div
          animate={{ opacity: isDone ? 1 : 0 }}
          transition={{ duration: 0.6 }}
          className="mt-8 pt-6 border-t border-border/20 grid grid-cols-1 sm:grid-cols-3 gap-3 text-center font-mono"
          style={{ pointerEvents: isDone ? "auto" : "none" }}
        >
          {[
            { label: "Time",             left: "~35 min",  right: "0.30s",    lc: "#ff4444", rc: "#00ff88" },
            { label: "Files opened",     left: "23+",       right: "0",       lc: "#ff4444", rc: "#00ff88" },
            { label: "Tokens consumed",  left: "~18,400",   right: "0",       lc: "#ff4444", rc: "#00ff88" },
          ].map(({ label, left, right, lc, rc }) => (
            <div key={label} className="p-4 rounded-lg border border-border/30 bg-background/50">
              <div className="text-xs text-muted-foreground/60 mb-2">{label}</div>
              <div className="flex items-center justify-center gap-3">
                <span className="text-sm font-bold" style={{ color: lc }}>{left}</span>
                <span className="text-muted-foreground/30 text-xs">vs</span>
                <span className="text-sm font-bold" style={{ color: rc }}>{right}</span>
              </div>
            </div>
          ))}
        </motion.div>
      )}

      {/* replay */}
      {isDone && (
        <div className="flex justify-center mt-6">
          <button
            onClick={play}
            className="px-6 py-2.5 font-mono text-xs border border-primary/30 text-primary rounded hover:bg-primary/10 transition-all"
          >
            ↺ replay
          </button>
        </div>
      )}
    </div>
  );
}

export default function Demo() {
  return (
    <main className="w-full min-h-screen bg-background">
      <SEO
        title="Live Demo — m1nd Graph Intelligence"
        description="Watch m1nd answer a real agent query in 0.30s with 3 tool calls — while grep opens 23 files and takes 35 minutes. Split-screen comparison + 100× slow-motion breakdown."
        canonicalPath="/demo"
      />
      <NavBar />

      <div className="pt-24 pb-6 border-b border-border/30 relative overflow-hidden">
        <div className="absolute inset-0 pointer-events-none" style={{ background: "radial-gradient(ellipse at 50% 0%, rgba(0,245,255,0.06), transparent 60%)" }} />
        <div className="container mx-auto px-6 relative z-10 text-center">
          <motion.div initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.7 }}>
            <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-6">
              live comparison
            </div>
            <h1 className="text-4xl md:text-6xl font-bold font-sans tracking-tight mb-4">
              The same task.
              <br />
              <span className="text-muted-foreground font-normal">Two different substrates.</span>
            </h1>
            <p className="text-lg text-muted-foreground max-w-2xl mx-auto font-mono">
              "Where is session timeout configured? What would break if I change it?"
            </p>
          </motion.div>
        </div>
      </div>

      <div className="py-12 border-b border-border/20">
        <div className="container mx-auto px-4 lg:px-6">
          <DemoComparison />
        </div>
      </div>

      <SlowMotionSection />

      <div className="py-20 text-center border-b border-border/20">
        <div className="container mx-auto px-6">
          <p className="text-2xl md:text-3xl font-bold font-sans tracking-tight mb-8">
            Ready to give your agent a graph?
          </p>
          <div className="bg-background/80 border border-border/40 rounded-lg p-4 inline-block font-mono text-sm text-primary/80 mb-8 shadow-[0_0_20px_rgba(0,245,255,0.05)]">
            git clone https://github.com/maxkle1nz/m1nd &amp;&amp; cargo build --release
          </div>
          <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
            <a href="https://m1nd.world/wiki/" target="_blank" rel="noreferrer">
              <button className="px-8 py-3 bg-primary text-primary-foreground font-bold rounded-md hover:bg-primary/90 transition-all shadow-[0_0_20px_rgba(0,245,255,0.2)]">
                Read the Docs
              </button>
            </a>
            <Link href="/use-cases" className="px-8 py-3 border border-primary/25 text-primary hover:bg-primary/10 transition-all rounded-md font-medium">
              See all use cases →
            </Link>
          </div>
        </div>
      </div>

      <Footer />
    </main>
  );
}
