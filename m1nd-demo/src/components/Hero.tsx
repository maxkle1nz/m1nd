import { motion } from "framer-motion";
import { GraphCanvas } from "./three/GraphCanvas";
import { StarField } from "./three/StarField";
import { OrbitControls, Stars } from "@react-three/drei";
import { WebGLErrorBoundary } from "./WebGLErrorBoundary";
import { Link } from "wouter";

function HeroScene() {
  return (
    <GraphCanvas cameraPos={[0, 2, 20]}>
      <OrbitControls autoRotate autoRotateSpeed={0.5} enableZoom={false} enablePan={false} maxPolarAngle={Math.PI / 2} minPolarAngle={Math.PI / 3} />
      <StarField count={3000} radius={30} />
      <Stars radius={50} depth={50} count={5000} factor={4} saturation={0} fade speed={1} />
      <mesh position={[0, 0, 0]}>
        <sphereGeometry args={[0.5, 32, 32]} />
        <meshBasicMaterial color="#00f5ff" />
      </mesh>
      <mesh position={[0, 0, 0]}>
        <sphereGeometry args={[1, 32, 32]} />
        <meshBasicMaterial color="#00f5ff" transparent opacity={0.2} />
      </mesh>
    </GraphCanvas>
  );
}

function HeroGraphSVG() {
  const nodes = [
    { cx: 50,  cy: 30,  r: 2.2, c: "#00f5ff" },
    { cx: 20,  cy: 55,  r: 1.6, c: "#00ff88" },
    { cx: 78,  cy: 22,  r: 1.8, c: "#7b61ff" },
    { cx: 85,  cy: 58,  r: 1.4, c: "#00f5ff" },
    { cx: 15,  cy: 75,  r: 1.4, c: "#00ff88" },
    { cx: 55,  cy: 72,  r: 1.6, c: "#ff6b00" },
    { cx: 35,  cy: 80,  r: 1.2, c: "#7b61ff" },
    { cx: 70,  cy: 80,  r: 1.2, c: "#00f5ff" },
    { cx: 8,   cy: 42,  r: 1.0, c: "#7b61ff" },
    { cx: 93,  cy: 40,  r: 1.0, c: "#00ff88" },
    { cx: 62,  cy: 14,  r: 1.2, c: "#00f5ff" },
    { cx: 30,  cy: 18,  r: 1.0, c: "#7b61ff" },
  ];
  const edges = [
    [0,1],[0,2],[0,3],[0,4],[0,5],[1,4],[1,8],[2,3],[2,9],[2,10],[5,6],[5,7],[3,9],[4,6],[10,11],[0,11],
  ];
  return (
    <svg
      viewBox="0 0 100 100"
      className="absolute inset-0 w-full h-full"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
    >
      <defs>
        <radialGradient id="heroGlow" cx="50%" cy="30%" r="60%">
          <stop offset="0%" stopColor="#00f5ff" stopOpacity="0.07" />
          <stop offset="100%" stopColor="transparent" stopOpacity="0" />
        </radialGradient>
      </defs>
      <rect width="100" height="100" fill="url(#heroGlow)" />
      {edges.map(([a, b], i) => (
        <line
          key={i}
          x1={nodes[a].cx} y1={nodes[a].cy}
          x2={nodes[b].cx} y2={nodes[b].cy}
          stroke={nodes[a].c}
          strokeWidth="0.18"
          strokeOpacity="0.22"
        />
      ))}
      {nodes.map((n, i) => (
        <g key={i}>
          <circle cx={n.cx} cy={n.cy} r={n.r * 3} fill={n.c} fillOpacity="0.04" />
          <circle cx={n.cx} cy={n.cy} r={n.r} fill={n.c} fillOpacity="0.75" />
        </g>
      ))}
    </svg>
  );
}

export function Hero() {
  return (
    <section className="relative w-full min-h-[100dvh] flex items-start md:items-center justify-center overflow-hidden bg-background pt-24 pb-12 md:pt-0 md:pb-0">
      <div className="absolute inset-0 z-0">
        <HeroGraphSVG />
        <WebGLErrorBoundary>
          <HeroScene />
        </WebGLErrorBoundary>
        <div className="absolute inset-0 bg-gradient-to-b from-transparent via-background/50 to-background" />
      </div>

      <div className="relative z-10 container px-6 mx-auto flex flex-col items-center text-center">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.8, ease: "easeOut" }}
          className="max-w-4xl"
        >
          <div className="font-mono text-[11px] sm:text-xs mb-8 flex flex-wrap items-center justify-center gap-x-2 gap-y-1">
            <span style={{ color: "#00f5ffb0" }}>north("harden the auth flow")</span>
            <span style={{ color: "#00f5ff40" }}>→</span>
            <span style={{ color: "#ffffff50" }}>trust: full_trust · memory: prior session · honest_gaps: 0</span>
          </div>

          <h1 className="text-4xl sm:text-5xl md:text-7xl font-bold tracking-tight text-foreground mb-6 font-sans leading-[1.05]">
            Memory that knows
            <br />
            <span className="text-transparent bg-clip-text bg-gradient-to-r from-primary to-secondary">
              when it's wrong.
            </span>
          </h1>

          <p className="text-xl md:text-2xl text-muted-foreground mb-10 max-w-2xl mx-auto leading-relaxed">
            m1nd gives your coding agent a live graph of your codebase, on your machine. Every answer carries a verdict: <strong className="text-foreground/90 font-semibold">act</strong>, <strong className="text-foreground/90 font-semibold">reverify</strong>, or <strong className="text-foreground/90 font-semibold">abstain</strong>, because <span className="text-primary/90">"insufficient evidence" is a real answer</span>. Memory is anchored to the code it cites, so when the code moves on, the memory flags itself instead of quietly lying.
          </p>

          <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
            <a href="https://github.com/maxkle1nz/m1nd" target="_blank" rel="noreferrer" className="w-full sm:w-auto">
              <button className="w-full sm:w-auto px-8 py-4 bg-primary text-primary-foreground font-bold rounded-md hover:bg-primary/90 transition-all shadow-[0_0_20px_rgba(0,245,255,0.3)] hover:shadow-[0_0_30px_rgba(0,245,255,0.5)]">
                Install the MCP server
              </button>
            </a>
            <a
              href="https://github.com/maxkle1nz/m1nd#when-not-to-use-m1nd"
              target="_blank"
              rel="noreferrer"
              className="w-full sm:w-auto px-8 py-4 border border-primary/30 text-primary hover:bg-primary/10 transition-all rounded-md font-medium"
            >
              Read the limits first →
            </a>
          </div>

          {/* ── Trust strip ── */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 1.1, duration: 0.9 }}
            className="mt-6 flex flex-wrap items-center justify-center gap-x-5 gap-y-2 text-[11px] font-mono"
          >
            <span className="flex items-center gap-1.5" style={{ color: "#00ff88cc" }}>
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <circle cx="5" cy="5" r="4" stroke="#00ff88" strokeWidth="1.2"/>
                <path d="M3 5l1.3 1.3L7 3.5" stroke="#00ff88" strokeWidth="1.2" strokeLinecap="round"/>
              </svg>
              MIT License
            </span>
            <span style={{ color: "#ffffff18" }}>·</span>
            <span style={{ color: "#ffffff45" }}>{`v${__M1ND_VERSION__}`}</span>
            <span style={{ color: "#ffffff18" }}>·</span>
            <span style={{ color: "#ffffff45" }}>Local-first</span>
            <span style={{ color: "#ffffff18" }}>·</span>
            <span style={{ color: "#ffffff45" }}>MCP-native</span>
            <span style={{ color: "#ffffff18" }}>·</span>
            <a
              href="https://github.com/maxkle1nz/m1nd"
              target="_blank"
              rel="noreferrer"
              className="transition-colors"
              style={{ color: "#00f5ff66" }}
              onMouseEnter={e => (e.currentTarget.style.color = "#00f5ffaa")}
              onMouseLeave={e => (e.currentTarget.style.color = "#00f5ff66")}
            >
              GitHub ↗
            </a>
          </motion.div>

          {/* ── Proof points ── */}
          <motion.div
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 1.4, duration: 0.8 }}
            className="mt-10 flex flex-wrap justify-center gap-8 sm:gap-12"
          >
            {[
              { val: "Official",  label: "MCP Registry entry", color: "#00f5ff" },
              { val: "Calibrated", label: "conformal verdicts", color: "#00ff88" },
              { val: "0",         label: "external telemetry",  color: "#7b61ff" },
              { val: "MIT · Rust", label: "open, local-first",  color: "#00f5ff" },
            ].map(({ val, label, color }) => (
              <div key={label} className="text-center">
                <div className="text-2xl md:text-3xl font-bold font-mono" style={{ color }}>{val}</div>
                <div className="text-[11px] font-mono mt-1" style={{ color: "#ffffff35" }}>{label}</div>
              </div>
            ))}
          </motion.div>
        </motion.div>
      </div>
    </section>
  );
}
