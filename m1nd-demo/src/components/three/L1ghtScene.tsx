import { useRef, useMemo } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import { OrbitControls, Stars, Html } from "@react-three/drei";
import * as THREE from "three";
import { WebGLBoundary } from "./WebGLBoundary";

const AMBER = "#ffb700";
const VIOLET = "#7b61ff";
const PINK = "#ff00aa";
const CYAN = "#00f5ff";

/* ── Document node definitions ─────────────────────────────── */
const DOC_NODES = [
  { label: "arXiv:2312.04117", kind: "Paper",        pos: [ 2.5,  1.2,  0.8] as [number,number,number], r: 0.28, color: AMBER  },
  { label: "US Pat. 11,847,992", kind: "Patent",     pos: [-2.8,  0.5,  1.5] as [number,number,number], r: 0.22, color: VIOLET },
  { label: "Voita et al. 2019",  kind: "Paper",      pos: [ 0.8, -2.5,  1.0] as [number,number,number], r: 0.25, color: AMBER  },
  { label: "Slack · #research",  kind: "Memory",     pos: [-1.2,  2.0, -1.5] as [number,number,number], r: 0.18, color: PINK   },
  { label: "RFC 9110 · HTTP",    kind: "RFC",        pos: [ 3.0, -0.8, -1.2] as [number,number,number], r: 0.20, color: CYAN   },
  { label: "Nature · Vol 623",   kind: "Journal",    pos: [-0.5, -1.5,  2.8] as [number,number,number], r: 0.30, color: AMBER  },
  { label: "EP3912057A1",        kind: "Patent",     pos: [ 1.5,  2.8, -0.5] as [number,number,number], r: 0.21, color: VIOLET },
  { label: "Meeting · Mar 18",   kind: "Memory",     pos: [-3.0, -0.8, -1.0] as [number,number,number], r: 0.17, color: PINK   },
  { label: "Fed. Learning",      kind: "Paper",      pos: [ 0.2,  0.8, -3.2] as [number,number,number], r: 0.26, color: AMBER  },
  { label: "Transformer.pdf",    kind: "PDF",        pos: [-1.8, -2.8,  0.5] as [number,number,number], r: 0.23, color: "#ff8c00"},
  { label: "Conversation Jan 14",kind: "Memory",     pos: [ 2.2, -0.5,  2.5] as [number,number,number], r: 0.17, color: PINK   },
  { label: "arXiv:2401.12065",   kind: "Paper",      pos: [-0.8,  3.2,  0.8] as [number,number,number], r: 0.24, color: AMBER  },
  { label: "10-K Filing 2024",   kind: "Report",     pos: [ 0.5, -3.0, -1.8] as [number,number,number], r: 0.20, color: "#10b981"},
  { label: "Jupyter · exp-42",   kind: "Notebook",   pos: [-2.0,  1.5,  2.2] as [number,number,number], r: 0.19, color: "#f37626"},
];

/* edges: pairs of node indices */
const EDGES: [number, number][] = [
  [0, 2], [0, 6], [0, 9], [0, 8],
  [1, 6], [1, 4],
  [2, 9], [2, 11], [2, 5],
  [3, 7], [3, 10],
  [4, 8], [4, 12],
  [5, 11], [5, 13],
  [6, 11], [7, 3],
  [8, 12], [9, 13],
];

const PULSE_INTERVAL = 5.0; // seconds per activation cycle

/* ── Static edge mesh ───────────────────────────────────────── */
function DocEdges({ pulseProgress }: { pulseProgress: React.RefObject<number> }) {
  const lineRef = useRef<THREE.LineSegments>(null);

  const positions = useMemo(() => {
    const pts: number[] = [];
    EDGES.forEach(([a, b]) => {
      pts.push(...DOC_NODES[a].pos, ...DOC_NODES[b].pos);
    });
    return new Float32Array(pts);
  }, []);

  useFrame(() => {
    if (!lineRef.current) return;
    const p = pulseProgress.current ?? 0;
    const maxDist = 5;
    const waveFront = p * maxDist;
    // Pulse brightens edges near wave front
    const baseOpacity = 0.15;
    const boost = Math.max(0, Math.sin(p * Math.PI)) * 0.25;
    (lineRef.current.material as THREE.LineBasicMaterial).opacity = baseOpacity + boost;
  });

  return (
    <lineSegments ref={lineRef}>
      <bufferGeometry>
        <bufferAttribute
          args={[positions, 3]}
          attach="attributes-position"
        />
      </bufferGeometry>
      <lineBasicMaterial color={AMBER} transparent opacity={0.15} />
    </lineSegments>
  );
}

/* ── Single document node ───────────────────────────────────── */
function DocNode({ node, index, pulseProgress }: {
  node: typeof DOC_NODES[0];
  index: number;
  pulseProgress: React.RefObject<number>;
}) {
  const coreRef = useRef<THREE.Mesh>(null);
  const glowRef = useRef<THREE.Mesh>(null);
  const nodeDist = Math.sqrt(node.pos[0] ** 2 + node.pos[1] ** 2 + node.pos[2] ** 2);
  const phaseOffset = index * 0.7;

  useFrame((state) => {
    if (!coreRef.current) return;
    const t = state.clock.getElapsedTime();
    const p = pulseProgress.current ?? 0;

    // Idle float
    coreRef.current.position.y = node.pos[1] + Math.sin(t * 0.5 + phaseOffset) * 0.12;

    // Spreading activation: wave at distance from origin expands 0→5 over cycle
    const waveFront = p * 5;
    const waveWidth = 0.9;
    const activation = Math.max(0, 1 - Math.abs(waveFront - nodeDist) / waveWidth);

    const scale = 1 + activation * 0.7;
    coreRef.current.scale.setScalar(scale);

    if (glowRef.current) {
      glowRef.current.scale.setScalar(1 + activation * 3.0);
      (glowRef.current.material as THREE.MeshBasicMaterial).opacity = 0.05 + activation * 0.28;
    }
  });

  const showLabel = ["Paper", "Patent", "Journal", "RFC", "Report"].includes(node.kind);

  return (
    <group position={node.pos}>
      {/* Glow halo */}
      <mesh ref={glowRef}>
        <sphereGeometry args={[node.r * 2.2, 12, 12]} />
        <meshBasicMaterial color={node.color} transparent opacity={0.05} depthWrite={false} />
      </mesh>
      {/* Core sphere */}
      <mesh ref={coreRef}>
        <sphereGeometry args={[node.r, 22, 22]} />
        <meshBasicMaterial color={node.color} />
      </mesh>
      {/* HTML label */}
      {showLabel && (
        <Html distanceFactor={9} center>
          <div
            style={{
              background: `${node.color}14`,
              border: `1px solid ${node.color}35`,
              borderRadius: 4,
              padding: "2px 8px",
              fontFamily: "monospace",
              fontSize: 10,
              color: node.color,
              whiteSpace: "nowrap",
              backdropFilter: "blur(6px)",
              marginTop: node.r * 55 + 4,
              pointerEvents: "none",
              userSelect: "none",
            }}
          >
            <span style={{ opacity: 0.5, marginRight: 4 }}>{node.kind}</span>
            {node.label}
          </div>
        </Html>
      )}
    </group>
  );
}

/* ── Expanding query pulse ring ─────────────────────────────── */
function QueryPulse({ pulseProgress }: { pulseProgress: React.RefObject<number> }) {
  const ring1 = useRef<THREE.Mesh>(null);
  const ring2 = useRef<THREE.Mesh>(null);

  useFrame(() => {
    const p = pulseProgress.current ?? 0;
    const r = p * 5.5;

    if (ring1.current) {
      ring1.current.scale.setScalar(r + 0.001);
      (ring1.current.material as THREE.MeshBasicMaterial).opacity = (1 - p) * 0.35;
    }
    // Second ring slightly behind
    const p2 = Math.max(0, p - 0.12);
    const r2 = p2 * 5.5;
    if (ring2.current) {
      ring2.current.scale.setScalar(r2 + 0.001);
      (ring2.current.material as THREE.MeshBasicMaterial).opacity = (1 - p2) * 0.15;
    }
  });

  return (
    <>
      <mesh ref={ring1} rotation={[Math.PI / 2, 0, 0]}>
        <ringGeometry args={[0.9, 1, 64]} />
        <meshBasicMaterial color={AMBER} transparent opacity={0.35} side={THREE.DoubleSide} depthWrite={false} />
      </mesh>
      <mesh ref={ring2} rotation={[Math.PI / 2, 0, 0]}>
        <ringGeometry args={[0.9, 1, 64]} />
        <meshBasicMaterial color={AMBER} transparent opacity={0.15} side={THREE.DoubleSide} depthWrite={false} />
      </mesh>
    </>
  );
}

/* ── Central query origin glyph ─────────────────────────────── */
function QueryOrigin({ pulseProgress }: { pulseProgress: React.RefObject<number> }) {
  const meshRef = useRef<THREE.Mesh>(null);

  useFrame((state) => {
    if (!meshRef.current) return;
    const t = state.clock.getElapsedTime();
    const p = pulseProgress.current ?? 0;
    const pulse = 1 + Math.sin(p * Math.PI) * 0.5;
    meshRef.current.scale.setScalar(pulse);
    meshRef.current.rotation.y = t * 0.5;
    meshRef.current.rotation.x = t * 0.3;
  });

  return (
    <group>
      <mesh ref={meshRef}>
        <octahedronGeometry args={[0.18, 0]} />
        <meshBasicMaterial color={AMBER} />
      </mesh>
      {/* Static glow at origin */}
      <mesh>
        <sphereGeometry args={[0.45, 12, 12]} />
        <meshBasicMaterial color={AMBER} transparent opacity={0.06} depthWrite={false} />
      </mesh>
      <Html center distanceFactor={8}>
        <div style={{
          fontFamily: "monospace", fontSize: 9, color: `${AMBER}99`,
          marginTop: 28, whiteSpace: "nowrap", pointerEvents: "none",
        }}>
          ❯ query origin
        </div>
      </Html>
    </group>
  );
}

/* ── Main scene: ties everything together + drives pulse ref ── */
function DocumentNebula() {
  const pulseProgress = useRef(0);

  useFrame((state) => {
    const t = state.clock.getElapsedTime();
    pulseProgress.current = (t % PULSE_INTERVAL) / PULSE_INTERVAL;
  });

  return (
    <>
      <Stars radius={60} depth={50} count={1800} factor={3} saturation={0} fade speed={0.5} />
      <OrbitControls
        autoRotate
        autoRotateSpeed={0.4}
        enableZoom={false}
        enablePan={false}
        minPolarAngle={Math.PI * 0.25}
        maxPolarAngle={Math.PI * 0.75}
      />
      <DocEdges pulseProgress={pulseProgress} />
      {DOC_NODES.map((node, i) => (
        <DocNode key={i} node={node} index={i} pulseProgress={pulseProgress} />
      ))}
      <QueryPulse pulseProgress={pulseProgress} />
      <QueryOrigin pulseProgress={pulseProgress} />
    </>
  );
}

/* ── CSS fallback (amber dots) for no-WebGL environments ─────── */
function AmberFallback() {
  return (
    <div className="w-full h-full absolute inset-0 overflow-hidden">
      {Array.from({ length: 60 }).map((_, i) => {
        const colors = [AMBER, "#ff8c00", VIOLET, PINK, CYAN];
        const c = colors[i % colors.length];
        return (
          <div
            key={i}
            className="absolute rounded-full"
            style={{
              width: (Math.sin(i * 1.31) * 1.8 + 2.5) + "px",
              height: (Math.sin(i * 1.31) * 1.8 + 2.5) + "px",
              left: ((Math.sin(i * 0.37 + 1) * 0.5 + 0.5) * 100) + "%",
              top: ((Math.cos(i * 0.29 + 0.5) * 0.5 + 0.5) * 100) + "%",
              background: c,
              boxShadow: `0 0 8px ${c}`,
              opacity: 0.25 + (Math.sin(i * 0.71) * 0.5 + 0.5) * 0.45,
              animation: `pulse ${2.4 + (i % 7) * 0.4}s ease-in-out infinite alternate`,
              animationDelay: (i % 5) * 0.3 + "s",
            }}
          />
        );
      })}
    </div>
  );
}

/* ── Exported component ─────────────────────────────────────── */
export function L1ghtScene() {
  return (
    <div className="w-full h-full absolute inset-0">
      <WebGLBoundary fallback={<AmberFallback />}>
        <Canvas
          camera={{ position: [0, 0, 9], fov: 50, near: 0.05, far: 500 }}
          gl={{
            antialias: true,
            alpha: true,
            powerPreference: "high-performance",
            failIfMajorPerformanceCaveat: false,
          }}
          dpr={[1, 2]}
          onCreated={({ gl }) => {
            gl.setPixelRatio(Math.min(window.devicePixelRatio, 2));
          }}
        >
          <color attach="background" args={["#050510"]} />
          <fog attach="fog" args={["#050510", 12, 55]} />
          <ambientLight intensity={0.1} color="#1a0a00" />
          <pointLight position={[0, 0, 0]} intensity={0.4} color="#ffb70030" distance={15} decay={2} />
          <DocumentNebula />
        </Canvas>
      </WebGLBoundary>
    </div>
  );
}
