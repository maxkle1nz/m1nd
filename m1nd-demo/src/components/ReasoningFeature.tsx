import { FeatureSection } from "./FeatureSection";
import { GraphCanvas } from "./three/GraphCanvas";
import { CameraRig } from "./three/CameraRig";
import { Stars, Html, Line } from "@react-three/drei";
import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { NamedNode } from "./three/NamedNode";
import { FlowEdge } from "./three/FlowEdge";

const CYCLE = 10;
const ADD = THREE.AdditiveBlending;

const hypothesisNodes = [
  { pos: [-3.5, 2.5, 0] as [number,number,number], label: "handleAuth()", sub: "src/auth/handler.ts:44", color: "#00f5ff", at: 0.4 },
  { pos: [3.5, 2.5, 0] as [number,number,number], label: "validateToken()", sub: "src/auth/token.ts:12", color: "#00f5ff", at: 1.0 },
  { pos: [0, -3.5, 0.5] as [number,number,number], label: "checkScope()", sub: "src/auth/scope.ts:88", color: "#00f5ff", at: 1.6 },
];

const gapPos: [number,number,number] = [0, 2.5, 0];

const beams = [
  { from: 0, to: 1 }, { from: 1, to: 2 }, { from: 2, to: 0 },
];

function GapIndicator() {
  const coreRef = useRef<THREE.MeshBasicMaterial>(null);
  const glowRef = useRef<THREE.MeshBasicMaterial>(null);
  const labelRef = useRef<HTMLDivElement>(null);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % CYCLE;
    const isActive = cycle >= 2.5;

    if (coreRef.current) {
      const tgt = isActive ? 0.4 + Math.sin(t * 5) * 0.15 : 0;
      coreRef.current.opacity += (tgt - coreRef.current.opacity) * 0.07;
    }
    if (glowRef.current) {
      const tgt = isActive ? 0.08 + Math.sin(t * 3) * 0.04 : 0;
      glowRef.current.opacity += (tgt - glowRef.current.opacity) * 0.05;
    }
    if (labelRef.current) {
      const cur = parseFloat(labelRef.current.style.opacity || "0");
      labelRef.current.style.opacity = String(cur + ((isActive ? 1 : 0) - cur) * 0.07);
    }
  });

  return (
    <group position={gapPos}>
      <mesh>
        <sphereGeometry args={[0.22, 24, 24]} />
        <meshBasicMaterial ref={coreRef} color="#ff00aa" transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
      <mesh>
        <sphereGeometry args={[0.7, 16, 16]} />
        <meshBasicMaterial ref={glowRef} color="#ff00aa" transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
      <Html center distanceFactor={12} zIndexRange={[2, 0]} style={{ pointerEvents: "none" }}>
        <div ref={labelRef} style={{
          background: "rgba(5,5,22,0.93)",
          border: "1px solid #ff00aa44",
          borderRadius: "4px",
          padding: "3px 8px",
          fontSize: "12px",
          lineHeight: "1.55",
          color: "#ff00aa",
          fontFamily: '"Space Mono", monospace',
          whiteSpace: "nowrap",
          transform: "translateY(-30px)",
          opacity: 0,
          pointerEvents: "none",
          boxShadow: "0 0 12px #ff00aa30",
        }}>
          ! structural gap detected
          <div style={{ color: "#5577aa", fontSize: "10px", marginTop: "1.5px" }}>
            missing: verifyExpiry()
          </div>
        </div>
      </Html>
    </group>
  );
}

function TriangulationBeam({ from, to, color }: { from: [number,number,number]; to: [number,number,number]; color: string }) {
  const bloomRef = useRef<any>(null);
  const coreRef = useRef<any>(null);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % CYCLE;
    const isActive = cycle >= 1.8;
    const tgt = isActive ? 0.3 + Math.sin(t * 2.2 + from[0]) * 0.08 : 0.02;

    const bloomMaterial = bloomRef.current?.material as THREE.Material & { opacity?: number } | undefined;
    if (bloomMaterial && typeof bloomMaterial.opacity === "number") {
      bloomMaterial.opacity += (tgt - bloomMaterial.opacity) * 0.06;
    }
    const coreMaterial = coreRef.current?.material as THREE.Material & { opacity?: number } | undefined;
    if (coreMaterial && typeof coreMaterial.opacity === "number") {
      const coreTgt = isActive ? tgt * 1.6 : 0;
      coreMaterial.opacity += (coreTgt - coreMaterial.opacity) * 0.08;
    }
  });

  return (
    <>
      <Line ref={bloomRef} points={[from, to]} color={color} lineWidth={3.5}
        transparent opacity={0.02} blending={ADD} depthWrite={false} />
      <Line ref={coreRef} points={[from, to]} color="#ffffff" lineWidth={0.8}
        transparent opacity={0.0} blending={ADD} depthWrite={false} />
    </>
  );
}

function ReasoningScene() {
  return (
    <GraphCanvas cameraPos={[0, 6, 16]}>
      <Stars radius={80} depth={60} count={1600} factor={3.5} saturation={0} fade speed={0.6} />
      <CameraRig
        targets={[[0, 6, 16], [5, 4, 12], [-4, 5, 14], [0, 2, 15]]}
        secondsPerWaypoint={7}
        spring={0.036}
        damping={0.84}
      />
      {hypothesisNodes.map((n, i) => (
        <NamedNode key={i} position={n.pos} label={n.label} sublabel={n.sub} color={n.color}
          activationTime={n.at} cycleLength={CYCLE} />
      ))}
      {beams.map((b, i) => (
        <TriangulationBeam key={i} from={hypothesisNodes[b.from].pos}
          to={hypothesisNodes[b.to].pos} color="#00ff88" />
      ))}
      {beams.map((b, i) => (
        <FlowEdge key={i} start={hypothesisNodes[b.from].pos} end={hypothesisNodes[b.to].pos}
          color="#00ff88" activationTime={1.8} cycleLength={CYCLE} speed={0.22} particleCount={2} />
      ))}
      <GapIndicator />
    </GraphCanvas>
  );
}

export function ReasoningFeature() {
  return (
    <FeatureSection
      title="Verify a Claim in One Query"
      subtitle="Structural Reasoning"
      description="Your agent can ask: 'does every auth call pass through scope validation?' m1nd checks it against the live graph and returns the answer — plus any structural gaps that break the assumption. A formal verification in under 1ms."
      align="right"
    >
      <ReasoningScene />
    </FeatureSection>
  );
}
