import { NavBar } from "@/components/NavBar";
import { SEO } from "@/components/SEO";
import { Footer } from "@/components/Footer";
import { motion } from "framer-motion";

interface SN {
  id: number;
  x: number;
  y: number;
  label: string;
  sub?: string;
  color: string;
  at: number;
  pulse?: boolean;
  anchor?: "start" | "end" | "middle";
  dy?: number;
}
interface SE { f: number; t: number; color: string; at: number; }

const SCALE = 0.52;

function GraphScene({ nodes, edges }: { nodes: SN[]; edges: SE[] }) {
  return (
    <div className="absolute inset-0 overflow-hidden">
      <div
        className="absolute inset-0 pointer-events-none"
        style={{
          backgroundImage: "radial-gradient(circle, #ffffff07 1px, transparent 1px)",
          backgroundSize: "28px 28px",
        }}
      />
      <svg viewBox="0 0 480 360" className="absolute inset-0 w-full h-full">

        {/* Edges — always visible, draw-in on mount */}
        {edges.map((e, i) => {
          const fn = nodes.find((n) => n.id === e.f);
          const tn = nodes.find((n) => n.id === e.t);
          if (!fn || !tn) return null;
          return (
            <motion.path
              key={`e${i}`}
              d={`M ${fn.x} ${fn.y} L ${tn.x} ${tn.y}`}
              stroke={e.color}
              strokeWidth={1.5}
              fill="none"
              strokeOpacity={0.38}
              initial={{ pathLength: 0 }}
              animate={{ pathLength: 1 }}
              transition={{ delay: e.at * SCALE, duration: 0.9, ease: "easeOut" }}
            />
          );
        })}

        {/* Particles — travel along edges continuously */}
        {edges.map((e, i) => {
          const fn = nodes.find((n) => n.id === e.f);
          const tn = nodes.find((n) => n.id === e.t);
          if (!fn || !tn) return null;
          return (
            <motion.circle
              key={`p${i}`}
              r={2.5}
              fill={e.color}
              initial={{ cx: fn.x, cy: fn.y, opacity: 0 }}
              animate={{
                cx: [fn.x, tn.x],
                cy: [fn.y, tn.y],
                opacity: [0.9, 0],
              }}
              transition={{
                duration: 1.6,
                repeat: Infinity,
                repeatDelay: 2.8,
                delay: e.at * SCALE + 0.6,
                ease: "linear",
                opacity: { times: [0.05, 1] },
              }}
            />
          );
        })}

        {/* Nodes — always visible */}
        {nodes.map((n) => {
          const labelAnchor = n.anchor ?? "middle";
          const labelX = n.anchor === "start" ? n.x + 11 : n.anchor === "end" ? n.x - 11 : n.x;
          const labelY = n.dy != null ? n.y + n.dy : n.y < 200 ? n.y + 18 : n.y - 19;
          const subY = labelY + 10;

          return (
            <g key={n.id}>
              {/* Pulse ring for hub nodes */}
              {n.pulse && (
                <>
                  <motion.circle
                    cx={n.x} cy={n.y}
                    fill="none" stroke={n.color} strokeWidth={1.5}
                    initial={{ r: 7, opacity: 0.6 }}
                    animate={{ r: [7, 27], opacity: [0.6, 0] }}
                    transition={{ duration: 2.2, repeat: Infinity, ease: "easeOut" }}
                  />
                  <motion.circle
                    cx={n.x} cy={n.y}
                    fill="none" stroke={n.color} strokeWidth={1}
                    initial={{ r: 7, opacity: 0.38 }}
                    animate={{ r: [7, 20], opacity: [0.38, 0] }}
                    transition={{ duration: 2.2, repeat: Infinity, ease: "easeOut", delay: 0.75 }}
                  />
                </>
              )}

              {/* Glow halo */}
              <circle cx={n.x} cy={n.y} r={13} fill={n.color} fillOpacity={0.07} />

              {/* Node body */}
              <circle cx={n.x} cy={n.y} r={7} fill={n.color} fillOpacity={0.18} stroke={n.color} strokeWidth={1.5} />
              <circle cx={n.x} cy={n.y} r={2.8} fill={n.color} />

              {/* Label */}
              <text
                x={labelX} y={labelY}
                textAnchor={labelAnchor}
                fill={n.color} fontSize={8.5}
                fontFamily="Space Mono, monospace" fontWeight="bold"
              >
                {n.label.length > 17 ? n.label.slice(0, 16) + "…" : n.label}
              </text>
              {n.sub && (
                <text
                  x={labelX} y={subY}
                  textAnchor={labelAnchor}
                  fill={n.color} fillOpacity={0.52}
                  fontSize={7} fontFamily="Space Mono, monospace"
                >
                  {n.sub.length > 22 ? n.sub.slice(0, 21) + "…" : n.sub}
                </text>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

const SCENES: Record<string, { nodes: SN[]; edges: SE[] }> = {
  "01": {
    nodes: [
      { id: 0, x: 215, y: 195, label: "AuthService", sub: "token refresh", color: "#00f5ff", at: 0.4, pulse: true },
      { id: 1, x: 360, y: 170, label: "TokenRefresher", sub: "refresh(expiredToken)", color: "#00f5ff", at: 1.2 },
      { id: 2, x: 415, y: 105, label: "SessionStore", sub: "get(sessionId)", color: "#00ff88", at: 2.0 },
      { id: 3, x: 348, y: 72, label: "JWTValidator", sub: "verify(token,secret)", color: "#00ff88", at: 2.6 },
      { id: 4, x: 90, y: 135, label: "RefreshPolicy", sub: "shouldRefresh(exp)", color: "#ffb700", at: 3.2 },
      { id: 5, x: 118, y: 268, label: "SecurityConfig", sub: "JWT_EXPIRY, ALGO", color: "#ffb700", at: 3.8 },
    ],
    edges: [
      { f: 0, t: 1, color: "#00f5ff", at: 1.2 },
      { f: 1, t: 2, color: "#00ff88", at: 2.0 },
      { f: 1, t: 3, color: "#00ff88", at: 2.6 },
      { f: 0, t: 4, color: "#ffb700", at: 3.2 },
      { f: 4, t: 5, color: "#ffb700", at: 3.8 },
    ],
  },
  "02": {
    nodes: [
      { id: 0, x: 240, y: 182, label: "worker_pool.py", sub: "class WorkerPool", color: "#ff6b00", at: 0.4, pulse: true },
      { id: 1, x: 360, y: 177, label: "TaskScheduler", sub: "schedule(task)", color: "#ff6b00", at: 1.5, anchor: "start", dy: 18 },
      { id: 2, x: 240, y: 90, label: "QueueMgr", sub: "enqueue(job)", color: "#ff6b00", at: 1.9 },
      { id: 3, x: 118, y: 177, label: "MetricsReporter", sub: "emit(event)", color: "#ff6b00", at: 2.3, anchor: "end", dy: 18 },
      { id: 4, x: 240, y: 270, label: "WorkerMonitor", sub: "track(worker)", color: "#ff6b00", at: 2.7 },
      { id: 5, x: 420, y: 98, label: "HealthCheck", sub: "ping(workerId)", color: "#ffb700", at: 3.5 },
      { id: 6, x: 315, y: 44, label: "APIHandler", sub: "dispatch(req)", color: "#ffb700", at: 3.9 },
      { id: 7, x: 52, y: 115, label: "Logger", sub: "emit(msg)", color: "#ffb700", at: 4.3 },
      { id: 8, x: 315, y: 322, label: "AlertingMgr", sub: "alert(threshold)", color: "#ffb700", at: 4.7 },
    ],
    edges: [
      { f: 0, t: 1, color: "#ff6b00", at: 1.5 }, { f: 0, t: 2, color: "#ff6b00", at: 1.9 },
      { f: 0, t: 3, color: "#ff6b00", at: 2.3 }, { f: 0, t: 4, color: "#ff6b00", at: 2.7 },
      { f: 1, t: 5, color: "#ffb700", at: 3.5 }, { f: 2, t: 6, color: "#ffb700", at: 3.9 },
      { f: 3, t: 7, color: "#ffb700", at: 4.3 }, { f: 4, t: 8, color: "#ffb700", at: 4.7 },
    ],
  },
  "03": {
    nodes: [
      { id: 0, x: 240, y: 48, label: "UnhandledError", sub: "event loop crash", color: "#ff00aa", at: 0.3, pulse: true, anchor: "middle", dy: 18 },
      { id: 1, x: 240, y: 128, label: "PromiseChain", sub: "unhandledRejection", color: "#ff6b00", at: 1.2, anchor: "start", dy: 18 },
      { id: 2, x: 240, y: 205, label: "NetworkLayer", sub: "fetch timeout", color: "#ff6b00", at: 2.0, anchor: "end", dy: -20 },
      { id: 3, x: 240, y: 275, label: "ConfigLoader", sub: "missing env var", color: "#ffb700", at: 2.8, anchor: "start", dy: 18 },
      { id: 4, x: 240, y: 335, label: "EnvValidator", sub: "ROOT_CAUSE: undef", color: "#00ff88", at: 3.6, pulse: true, anchor: "end", dy: -20 },
    ],
    edges: [
      { f: 0, t: 1, color: "#ff6b00", at: 1.2 },
      { f: 1, t: 2, color: "#ff6b00", at: 2.0 },
      { f: 2, t: 3, color: "#ffb700", at: 2.8 },
      { f: 3, t: 4, color: "#00ff88", at: 3.6 },
    ],
  },
  "04": {
    nodes: [
      { id: 0, x: 240, y: 200, label: "Edit Plan", sub: "5 files · 9 changes", color: "#00f5ff", at: 0, pulse: true },
      { id: 1, x: 100, y: 118, label: "UserController", sub: "3 changes — ok", color: "#00ff88", at: 0.5 },
      { id: 2, x: 240, y: 72, label: "authMiddleware", sub: "1 change — ok", color: "#00ff88", at: 1.2 },
      { id: 3, x: 378, y: 118, label: "sessionStore", sub: "2 changes — conflict!", color: "#ff00aa", at: 1.9 },
      { id: 4, x: 318, y: 298, label: "tokenRefresh", sub: "1 change — ok", color: "#00ff88", at: 2.6 },
      { id: 5, x: 152, y: 298, label: "loginRoute", sub: "2 changes — ok", color: "#00ff88", at: 3.3 },
    ],
    edges: [
      { f: 0, t: 1, color: "#00ff88", at: 0.5 }, { f: 0, t: 2, color: "#00ff88", at: 1.2 },
      { f: 0, t: 3, color: "#ff00aa", at: 1.9 }, { f: 0, t: 4, color: "#00ff88", at: 2.6 },
      { f: 0, t: 5, color: "#00ff88", at: 3.3 },
    ],
  },
  "05": {
    nodes: [
      { id: 0, x: 385, y: 182, label: "auth/index.ts", sub: "3 sessions ago", color: "#ffb700", at: 0.6, anchor: "end", dy: -20 },
      { id: 1, x: 308, y: 78, label: "db/queries.ts", sub: "2 sessions ago", color: "#ffb700", at: 1.2 },
      { id: 2, x: 158, y: 62, label: "api/middleware.ts", sub: "last session", color: "#00ff88", at: 1.8 },
      { id: 3, x: 72, y: 182, label: "models/User.ts", sub: "last session", color: "#00ff88", at: 2.4, anchor: "start", dy: -20 },
      { id: 4, x: 155, y: 302, label: "hooks/useAuth.ts", sub: "last session", color: "#00ff88", at: 3.0 },
      { id: 5, x: 308, y: 302, label: "types/session.ts", sub: "open trail", color: "#00f5ff", at: 3.6 },
      { id: 6, x: 240, y: 182, label: "store/authSlice", sub: "open trail", color: "#00f5ff", at: 4.2, pulse: true },
    ],
    edges: [
      { f: 0, t: 1, color: "#ffb700", at: 1.2 }, { f: 1, t: 2, color: "#ffb700", at: 1.8 },
      { f: 2, t: 3, color: "#00ff88", at: 2.4 }, { f: 3, t: 4, color: "#00ff88", at: 3.0 },
      { f: 4, t: 5, color: "#00f5ff", at: 3.6 }, { f: 5, t: 6, color: "#00f5ff", at: 4.2 },
    ],
  },
  "06": {
    nodes: [
      { id: 6, x: 240, y: 182, label: "m1nd.query", sub: "unified graph", color: "#00ff88", at: 0.2, pulse: true },
      { id: 0, x: 112, y: 132, label: "jwt_handler.py", sub: "sign(payload,secret)", color: "#00f5ff", at: 0.6, anchor: "end", dy: -20 },
      { id: 1, x: 88, y: 220, label: "auth.py", sub: "verify_token(req)", color: "#00f5ff", at: 1.3, anchor: "end", dy: 18 },
      { id: 2, x: 148, y: 308, label: "session_store.py", sub: "persist(session_id)", color: "#00f5ff", at: 2.0, anchor: "end", dy: -20 },
      { id: 3, x: 368, y: 108, label: "RFC 6749", sub: "OAuth 2.0 Framework", color: "#ffb700", at: 1.0 },
      { id: 4, x: 392, y: 195, label: "RFC 7519", sub: "JSON Web Token", color: "#ffb700", at: 1.7, anchor: "start", dy: -20 },
      { id: 5, x: 335, y: 295, label: "OWASP Auth", sub: "Session security", color: "#ffb700", at: 2.4 },
    ],
    edges: [
      { f: 6, t: 0, color: "#00f5ff", at: 0.6 }, { f: 6, t: 1, color: "#00f5ff", at: 1.3 },
      { f: 6, t: 2, color: "#00f5ff", at: 2.0 }, { f: 6, t: 3, color: "#ffb700", at: 1.0 },
      { f: 6, t: 4, color: "#ffb700", at: 1.7 }, { f: 6, t: 5, color: "#ffb700", at: 2.4 },
    ],
  },
  "07": {
    nodes: [
      { id: 0, x: 240, y: 182, label: "m1nd graph", sub: "live knowledge", color: "#00f5ff", at: 0, pulse: true },
      { id: 1, x: 378, y: 108, label: "auth.query", sub: "confirmed ×23", color: "#00ff88", at: 0.8 },
      { id: 2, x: 382, y: 258, label: "blast_radius", sub: "confirmed ×18", color: "#00ff88", at: 1.4 },
      { id: 3, x: 98, y: 108, label: "trace_error", sub: "confirmed ×11", color: "#00f5ff", at: 2.0 },
      { id: 4, x: 98, y: 258, label: "validate_plan", sub: "confirmed ×7", color: "#7b61ff", at: 2.6 },
      { id: 5, x: 240, y: 328, label: "Hebbian LTP", sub: "edges reinforce on use", color: "#00ff88", at: 3.2, pulse: true },
    ],
    edges: [
      { f: 0, t: 1, color: "#00ff88", at: 0.8 }, { f: 0, t: 2, color: "#00ff88", at: 1.4 },
      { f: 0, t: 3, color: "#00f5ff", at: 2.0 }, { f: 0, t: 4, color: "#7b61ff", at: 2.6 },
      { f: 0, t: 5, color: "#00ff88", at: 3.2 },
    ],
  },
  "08": {
    nodes: [
      { id: 0, x: 240, y: 72, label: "antibody: null-deref", sub: "severity: HIGH", color: "#ff00aa", at: 0, pulse: true },
      { id: 1, x: 240, y: 178, label: "m1nd.antibody_scan", sub: "checking 43 files", color: "#00f5ff", at: 0.8 },
      { id: 2, x: 108, y: 278, label: "session_pool.py", sub: "MATCH — same shape", color: "#ff00aa", at: 2.0 },
      { id: 3, x: 372, y: 278, label: "worker_pool.py", sub: "MATCH — same shape", color: "#ff00aa", at: 2.8 },
      { id: 4, x: 240, y: 338, label: "2 bugs contained", sub: "before production", color: "#00ff88", at: 3.8, pulse: true },
    ],
    edges: [
      { f: 0, t: 1, color: "#ff00aa", at: 0.8 },
      { f: 1, t: 2, color: "#ff00aa", at: 2.0 }, { f: 1, t: 3, color: "#ff00aa", at: 2.8 },
      { f: 2, t: 4, color: "#00ff88", at: 3.8 }, { f: 3, t: 4, color: "#00ff88", at: 3.8 },
    ],
  },
  "09": {
    nodes: [
      { id: 0, x: 240, y: 188, label: "m1nd.missing", sub: "structural gaps", color: "#7b61ff", at: 0, pulse: true },
      { id: 1, x: 92, y: 112, label: "error_handler.py", sub: "missing: timeout", color: "#7b61ff", at: 0.8 },
      { id: 2, x: 148, y: 292, label: "retry_policy.py", sub: "missing: cleanup", color: "#7b61ff", at: 1.4 },
      { id: 3, x: 388, y: 112, label: "db_connector.py", sub: "missing: retry", color: "#7b61ff", at: 2.0 },
      { id: 4, x: 338, y: 292, label: "cache_layer.py", sub: "missing: fallback", color: "#7b61ff", at: 2.6 },
      { id: 5, x: 240, y: 48, label: "api_gateway.py", sub: "missing: rate limiter", color: "#7b61ff", at: 3.2 },
    ],
    edges: [
      { f: 0, t: 1, color: "#7b61ff", at: 0.8 }, { f: 0, t: 2, color: "#7b61ff", at: 1.4 },
      { f: 0, t: 3, color: "#7b61ff", at: 2.0 }, { f: 0, t: 4, color: "#7b61ff", at: 2.6 },
      { f: 0, t: 5, color: "#7b61ff", at: 3.2 },
    ],
  },
  "10": {
    nodes: [
      { id: 0, x: 72, y: 100, label: "worker_1", sub: "flow thread 1", color: "#ff6b00", at: 0.5, anchor: "start", dy: 18 },
      { id: 1, x: 72, y: 195, label: "worker_2", sub: "flow thread 2", color: "#ff6b00", at: 0.8, anchor: "start", dy: 18 },
      { id: 2, x: 72, y: 290, label: "worker_3", sub: "flow thread 3", color: "#ff6b00", at: 1.1, anchor: "start", dy: 18 },
      { id: 3, x: 252, y: 193, label: "shared_state.py", sub: "4 writers · no guard", color: "#ff4444", at: 2.0, pulse: true },
      { id: 4, x: 405, y: 128, label: "counter.py", sub: "dirty write", color: "#ffb700", at: 3.2 },
      { id: 5, x: 405, y: 258, label: "cache_state.py", sub: "dirty write", color: "#ffb700", at: 3.8 },
    ],
    edges: [
      { f: 0, t: 3, color: "#ff6b00", at: 2.0 }, { f: 1, t: 3, color: "#ff6b00", at: 2.0 },
      { f: 2, t: 3, color: "#ff6b00", at: 2.0 },
      { f: 3, t: 4, color: "#ffb700", at: 3.2 }, { f: 3, t: 5, color: "#ffb700", at: 3.8 },
    ],
  },
  "11": {
    nodes: [
      { id: 0, x: 240, y: 68, label: "m1nd.hypothesize", sub: '"settings crash"', color: "#7b61ff", at: 0, pulse: true },
      { id: 1, x: 110, y: 175, label: "settings_routes", sub: "entry point", color: "#7b61ff", at: 0.8 },
      { id: 2, x: 240, y: 212, label: "config_provider", sub: "no validation gate", color: "#ff4444", at: 1.6 },
      { id: 3, x: 375, y: 175, label: "boot_init.py", sub: "consumes unvalidated", color: "#ff4444", at: 2.4 },
      { id: 4, x: 110, y: 315, label: "crash path #1", sub: "confirmed", color: "#ff4444", at: 3.0, pulse: true },
      { id: 5, x: 375, y: 315, label: "crash path #2", sub: "confirmed", color: "#ff4444", at: 3.4, pulse: true },
    ],
    edges: [
      { f: 0, t: 1, color: "#7b61ff", at: 0.8 }, { f: 1, t: 2, color: "#ff4444", at: 1.6 },
      { f: 2, t: 3, color: "#ff4444", at: 2.4 },
      { f: 1, t: 4, color: "#ff4444", at: 3.0 }, { f: 3, t: 5, color: "#ff4444", at: 3.4 },
    ],
  },
  "12": {
    nodes: [
      { id: 0, x: 240, y: 182, label: "auth.py", sub: "INFECTED — origin", color: "#ff4444", at: 0, pulse: true },
      { id: 1, x: 370, y: 132, label: "session_pool.py", sub: "INFECTED — direct", color: "#ff4444", at: 1.8 },
      { id: 2, x: 112, y: 132, label: "middleware.py", sub: "INFECTED — chain", color: "#ff4444", at: 2.5 },
      { id: 3, x: 240, y: 312, label: "api_handler.py", sub: "INFECTED — call", color: "#ff4444", at: 3.2 },
      { id: 4, x: 432, y: 235, label: "worker_pool.py", sub: "susceptible", color: "#ffb700", at: 4.2, anchor: "end", dy: -20 },
      { id: 5, x: 48, y: 235, label: "cache.py", sub: "susceptible", color: "#ffb700", at: 4.8, anchor: "start", dy: -20 },
      { id: 6, x: 372, y: 312, label: "config.py", sub: "immune", color: "#00ff88", at: 0.4 },
      { id: 7, x: 108, y: 312, label: "logger.py", sub: "immune", color: "#00f5ff", at: 0.4 },
    ],
    edges: [
      { f: 0, t: 1, color: "#ff4444", at: 1.8 }, { f: 0, t: 2, color: "#ff4444", at: 2.5 },
      { f: 0, t: 3, color: "#ff4444", at: 3.2 },
      { f: 1, t: 4, color: "#ffb700", at: 4.2 }, { f: 2, t: 5, color: "#ffb700", at: 4.8 },
    ],
  },
  "13": {
    nodes: [
      { id: 0, x: 240, y: 108, label: "session_pool.py", sub: "CRITICAL — 4.7× accel", color: "#ff4444", at: 0.2, pulse: true },
      { id: 1, x: 108, y: 218, label: "worker_pool.py", sub: "WARNING — 2.3× accel", color: "#ffb700", at: 0.5, pulse: true, anchor: "end", dy: -20 },
      { id: 2, x: 378, y: 205, label: "auth.py", sub: "WATCH — vel rising", color: "#ffb700", at: 0.8, anchor: "start", dy: -20 },
      { id: 3, x: 112, y: 308, label: "api_handler.py", sub: "stable — 0.4× rate", color: "#00ff88", at: 1.0 },
      { id: 4, x: 382, y: 308, label: "utils.py", sub: "stable — 18d no change", color: "#00f5ff", at: 1.2 },
    ],
    edges: [],
  },
};

const useCases = [
  {
    number: "01",
    title: "Find the Auth Refresh Flow",
    subtitle: "Orientation",
    description: "An AI agent needs to understand how token refresh works. Instead of reading 8 files and burning 4,000 tokens, it fires a single m1nd query and gets the complete subgraph back in one response.",
    steps: [
      "m1nd activates from AuthService and propagates through the import graph in 1.36µs",
      "TokenRefresher, JWTValidator, and SessionStore returned with function signatures attached",
      "5 nodes, 4 edges, 0 files opened — the complete auth flow in one tool call",
      "Callers and dependencies already included — no follow-up queries needed",
      "From question to complete map: 0.18s and 3 tool calls total",
    ],
    color: "#00f5ff",
    align: "left",
  },
  {
    number: "02",
    title: "Blast Radius Before Refactoring",
    subtitle: "Impact Analysis",
    description: "Before touching worker_pool.py, the agent needs to know exactly what will break. m1nd computes the full impact cone before a single line changes — sorted by coupling risk.",
    steps: [
      "WorkerPool identified as the epicenter — full impact analysis begins immediately",
      "4 direct callers returned in the first ring with call frequency attached",
      "4 indirect consumers expanded into the second ring, sorted by blast distance",
      "Each dependency flagged: stable, fragile, or critical — actionable, not just informational",
      "The agent has a safe refactor plan before writing a single line of code",
    ],
    color: "#ff6b00",
    align: "right",
  },
  {
    number: "03",
    title: "Trace a Runtime Error to Root Cause",
    subtitle: "Error Tracing",
    description: "A production crash surfaces at UnhandledError. The agent traces the call graph backwards in one pass — no log scanning, no trial and error, no hallucinated fixes.",
    steps: [
      "UnhandledError is the entry point — m1nd begins a backwards traversal immediately",
      "PromiseChain → NetworkLayer → ConfigLoader traced in one graph walk",
      "Missing environment variable identified at EnvValidator — the actual root cause",
      "Full ancestry returned: 4 nodes, 3 edges, 1 confirmed root cause",
      "From crash report to fix location: one backward traversal, zero file reads",
    ],
    color: "#ff00aa",
    align: "left",
  },
  {
    number: "04",
    title: "Validate a Multi-File Edit Plan",
    subtitle: "Pre-flight Check",
    description: "Before applying 9 changes across 5 files, the agent validates the plan against the live graph — catching structural conflicts before any code runs.",
    steps: [
      "All 5 target files evaluated simultaneously against the current graph state",
      "4 files confirmed structurally valid — their changes will not cascade unexpectedly",
      "Circular import conflict detected in sessionStore.ts before the first edit runs",
      "Confidence score returned: 94% safe, 1 conflict to resolve first",
      "Your agent ships a clean PR instead of discovering the problem in production",
    ],
    color: "#00ff88",
    align: "right",
  },
  {
    number: "05",
    title: "Resume a Saved Investigation",
    subtitle: "Persistent Memory",
    description: "Three sessions later, the agent returns to an unfinished auth investigation. m1nd restores the full context in milliseconds — the agent resumes the work, not the orientation.",
    steps: [
      "7 previously-visited nodes restored with their original traversal sequence",
      "Nodes from older sessions separated from today's findings — temporal context preserved",
      "Open trails marked — the agent knows exactly where it stopped and why",
      "Zero tokens spent on reconstruction — the graph remembers what mattered",
      "The agent continues the investigation exactly where it left off",
    ],
    color: "#ffb700",
    align: "left",
  },
  {
    number: "06",
    title: "Connect Research to Code",
    subtitle: "Knowledge Graph",
    description: "m1nd ingests more than code. RFCs, specs, and documentation live in the same graph. One query returns the implementation and the standard that defined it — simultaneously.",
    steps: [
      "Code nodes and document nodes share the same graph — no separate search required",
      "Querying jwt_handler.py also returns RFC 7519 as a directly connected node",
      "Implementation and specification linked by typed edges, not keyword matches",
      "Your agent understands the why behind the code, not just the what",
      "Useful for compliance work, security reviews, and onboarding new agents to a codebase",
    ],
    color: "#ffb700",
    align: "right",
  },
  {
    number: "07",
    title: "The Graph That Gets Smarter",
    subtitle: "Hebbian Learning",
    description: "m1nd was designed by LLMs, tested by LLMs, and it learns from them too. Every confirmed result strengthens its edges via Hebbian LTP — each session makes the next one more accurate.",
    steps: [
      "Each result the agent confirms automatically strengthens its graph edges",
      "Paths that proved useful become higher-ranked in future queries on the same codebase",
      "After 10 sessions, query accuracy measurably improves — no manual tuning",
      "The graph was built for agents — and it gets better the more agents use it",
      "Your agent isn't just using a codebase map. It's improving it.",
    ],
    color: "#00f5ff",
    align: "left",
  },
  {
    number: "08",
    title: "Never Let the Same Bug Bite Twice",
    subtitle: "Antibody Immunity",
    description: "When a bug is fixed, m1nd lets the agent encode it as a structural pattern — an antibody. Every future scan checks for the same shape across the entire codebase automatically.",
    steps: [
      "Agent encodes a fixed bug as a named structural pattern via m1nd.antibody_create",
      "Pattern stored with severity level and description — not just the file, the topology",
      "m1nd.antibody_scan checks the pattern against all changed files on every future session",
      "Two high-severity matches found: session_pool.py and worker_pool.py — before they ship",
      "Bug patterns the team fixed once never reach production again — the graph remembers",
    ],
    color: "#ff00aa",
    align: "right",
  },
  {
    number: "09",
    title: "Find the Code That Doesn't Exist Yet",
    subtitle: "Structural Gap Detection",
    description: "The hardest bugs to find are the ones that aren't there. m1nd.missing detects structural holes — missing guards, missing retries, missing timeouts — by comparing your graph to expected patterns.",
    steps: [
      "Agent asks: 'Where is timeout cleanup guard retry missing from the error path?'",
      "m1nd walks the graph looking for nodes that exist without their expected neighbors",
      "5 files identified with absent structural guards — no grep, no file reading required",
      "Ghost filaments show exactly where the missing nodes should connect",
      "Your agent fixes absences, not just presence — defensive code, not reactive",
    ],
    color: "#7b61ff",
    align: "left",
  },
  {
    number: "10",
    title: "Hunt Race Conditions Before Production",
    subtitle: "Concurrent Hazard Mapping",
    description: "m1nd.flow_simulate runs concurrent particle streams through the dependency graph. Where multiple flows converge on shared mutable state, turbulence scores spike — before a single thread runs.",
    steps: [
      "4 concurrent workers tracked through their dependency paths simultaneously",
      "shared_state.py identified as a turbulence node — 4 concurrent writers, no guard",
      "Race condition confirmed: counter.py and cache_state.py get dirty writes",
      "Structural hazard surfaced in 0.3s — before any test was written or thread started",
      "Your agent finds the race in the architecture, not in the production logs",
    ],
    color: "#ff6b00",
    align: "right",
  },
  {
    number: "11",
    title: "Test a Structural Claim Before It Crashes",
    subtitle: "Hypothesis Testing",
    description: "m1nd.hypothesize takes a natural-language structural claim and tests it against the graph. The agent doesn't guess — it triangulates. Evidence paths either harden into confirmed risk or break cleanly.",
    steps: [
      "Agent writes a plain claim: 'settings_routes can save invalid config that crashes on boot'",
      "m1nd maps the claim to graph topology: settings_routes → config_provider → boot_init",
      "boot_init.py confirmed: no validation gate before it consumes provider output",
      "3 confirmed crash paths returned with node-level evidence — zero guesswork",
      "The agent fixes the structural gap before any user hits the boot-time crash",
    ],
    color: "#7b61ff",
    align: "left",
  },
  {
    number: "12",
    title: "Map How Far a Bug Spreads",
    subtitle: "Epidemic Modeling",
    description: "m1nd.epidemic runs an SIR-style simulation from any infected node. Infected, susceptible, and immune modules are color-coded across the graph — so the agent knows exactly how wide to cast the fix.",
    steps: [
      "auth.py confirmed infected — SIR propagation begins from that node immediately",
      "First wave: session_pool.py and middleware.py — direct import path, now infected",
      "Second wave: worker_pool.py, cache.py, scheduler.py enter the susceptible zone",
      "logger.py and config.py confirmed immune — no dependency path to auth.py",
      "The agent contains the infection before shipping a fix that only patches half the blast",
    ],
    color: "#ff4444",
    align: "right",
  },
  {
    number: "13",
    title: "Catch the Next Bug Before It's Written",
    subtitle: "Predictive Breakage",
    description: "m1nd.tremor detects accelerating change frequency across the graph. When a file's edit velocity spikes past a threshold, it raises a seismic alert — the codebase is about to fracture at that node.",
    steps: [
      "m1nd.tremor scans the last 30 days: change rate per file, sorted by acceleration",
      "session_pool.py: 14 changes in 30 days, up from 3 in the prior period — 4.7× acceleration",
      "worker_pool.py: velocity still rising — 2 open investigations, no antibody coverage",
      "Trust scores confirm: both files are historically fragile, defect correlation matches",
      "Agent flags the tremor zone before anyone files a bug report — structural prediction, not reaction",
    ],
    color: "#ffb700",
    align: "left",
  },
];

function UseCaseSection({ uc }: { uc: (typeof useCases)[0] }) {
  const isRight = uc.align === "right";
  const scene = SCENES[uc.number];

  return (
    <section className="relative w-full py-24 border-t border-border/30 overflow-hidden">
      <div
        className="absolute inset-0 pointer-events-none"
        style={{
          background: `radial-gradient(ellipse at ${isRight ? "80%" : "20%"} 50%, ${uc.color}06, transparent 60%)`,
        }}
      />

      <div className="container mx-auto px-6 relative z-10">
        <div className={`flex flex-col ${isRight ? "lg:flex-row-reverse" : "lg:flex-row"} items-center gap-12 lg:gap-20`}>
          <motion.div
            className="w-full lg:w-1/2"
            initial={{ opacity: 0, x: isRight ? 30 : -30 }}
            whileInView={{ opacity: 1, x: 0 }}
            viewport={{ once: true, margin: "-80px" }}
            transition={{ duration: 0.7 }}
          >
            <div
              className="text-[80px] md:text-[120px] font-bold font-mono leading-none mb-4 select-none"
              style={{ color: `${uc.color}12`, WebkitTextStroke: `1px ${uc.color}20` }}
            >
              {uc.number}
            </div>

            <h3 className="font-mono text-xs tracking-widest uppercase mb-3" style={{ color: uc.color }}>
              {uc.subtitle}
            </h3>
            <h2 className="text-2xl md:text-4xl font-bold font-sans tracking-tight mb-4">
              {uc.title}
            </h2>
            <p className="text-lg text-muted-foreground mb-8 leading-relaxed">
              {uc.description}
            </p>

            <ol className="space-y-3">
              {uc.steps.map((step, i) => (
                <motion.li
                  key={i}
                  className="flex items-start gap-3 text-sm"
                  initial={{ opacity: 0, x: -10 }}
                  whileInView={{ opacity: 1, x: 0 }}
                  viewport={{ once: true }}
                  transition={{ duration: 0.4, delay: i * 0.08 }}
                >
                  <span
                    className="w-5 h-5 rounded-full flex-shrink-0 flex items-center justify-center text-[10px] font-mono font-bold mt-0.5"
                    style={{
                      background: `${uc.color}18`,
                      color: uc.color,
                      border: `1px solid ${uc.color}44`,
                    }}
                  >
                    {i + 1}
                  </span>
                  <span className="text-muted-foreground leading-relaxed">{step}</span>
                </motion.li>
              ))}
            </ol>
          </motion.div>

          <motion.div
            className="w-full lg:w-1/2 h-[420px] lg:h-[580px] relative rounded-xl border overflow-hidden"
            style={{
              borderColor: `${uc.color}25`,
              background: "rgba(5,5,16,0.85)",
              boxShadow: `0 0 40px ${uc.color}10`,
            }}
            initial={{ opacity: 0, scale: 0.97 }}
            whileInView={{ opacity: 1, scale: 1 }}
            viewport={{ once: true, margin: "-50px" }}
            transition={{ duration: 0.7, delay: 0.1 }}
          >
            {scene && <GraphScene nodes={scene.nodes} edges={scene.edges} />}

            <div
              className="absolute bottom-3 right-4 font-mono text-[9px] tracking-widest uppercase"
              style={{ color: `${uc.color}40` }}
            >
              m1nd graph · {scene?.nodes.length ?? 0} nodes · {scene?.edges.length ?? 0} edges
            </div>
          </motion.div>
        </div>
      </div>
    </section>
  );
}

export default function UseCases() {
  return (
    <main className="w-full min-h-screen bg-background">
      <SEO
        title="Use Cases — m1nd Graph Intelligence"
        description="13 real scenarios where m1nd gives AI agents surgical precision: impact analysis, race conditions, antibody immunity, hypothesis testing, epidemic modeling, predictive breakage, and more. All in under 0.30s."
        canonicalPath="/use-cases"
      />
      <NavBar />
      <div className="pt-24 pb-12 text-center relative overflow-hidden border-b border-border/30">
        <div
          className="absolute inset-0 pointer-events-none"
          style={{ background: "radial-gradient(ellipse at 50% 0%, rgba(0,245,255,0.08), transparent 60%)" }}
        />
        <motion.div
          className="relative z-10 container mx-auto px-6"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.8 }}
        >
          <div className="inline-block font-mono text-xs text-primary/60 tracking-widest uppercase border border-primary/20 rounded px-3 py-1 mb-6">
            real scenarios
          </div>
          <h1 className="text-4xl md:text-6xl font-bold font-sans tracking-tight mb-6">
            What Your Agent Can Do
          </h1>
          <p className="text-xl text-muted-foreground max-w-2xl mx-auto">
            Thirteen real scenarios. Each one is something your agent struggles with today — and solves in under 0.30s with m1nd.
          </p>
        </motion.div>
      </div>

      {useCases.map((uc) => (
        <UseCaseSection key={uc.number} uc={uc} />
      ))}

      <Footer />
    </main>
  );
}
