import { FeatureSection } from "./FeatureSection";
import { GraphCanvas } from "./three/GraphCanvas";
import { CameraRig } from "./three/CameraRig";
import { Stars, Html } from "@react-three/drei";
import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { NamedNode } from "./three/NamedNode";
import { FlowEdge } from "./three/FlowEdge";

const CYCLE = 10;
const ADD = THREE.AdditiveBlending;

const target = { pos: [0, 0, 0] as [number,number,number], label: "handleAuth()", sub: "src/auth/handler.ts:44", color: "#00f5ff", at: 0 };
const callers = [
  { pos: [-4, 2.5, 0] as [number,number,number], label: "loginRoute()", sub: "calls · 3 times", color: "#ffb700", at: 0.8 },
  { pos: [-4, -2.5, 0] as [number,number,number], label: "refreshRoute()", sub: "calls · 1 time", color: "#ffb700", at: 1.3 },
];
const callees = [
  { pos: [4, 2.5, 0] as [number,number,number], label: "validateJWT()", sub: "called by target", color: "#00ff88", at: 1.8 },
  { pos: [4, -2.5, 0.5] as [number,number,number], label: "updateSession()", sub: "called by target", color: "#00ff88", at: 2.3 },
];

function ContainmentSphere() {
  const wireRef = useRef<THREE.MeshBasicMaterial>(null);
  const glowRef = useRef<THREE.MeshBasicMaterial>(null);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const breathe = 0.03 + Math.sin(t * 1.2) * 0.015;
    if (wireRef.current) wireRef.current.opacity = breathe;
    if (glowRef.current) glowRef.current.opacity = breathe * 0.4;
  });

  return (
    <group>
      <mesh>
        <sphereGeometry args={[2.3, 32, 32]} />
        <meshBasicMaterial ref={wireRef} color="#00f5ff" transparent opacity={0.03} wireframe blending={ADD} depthWrite={false} />
      </mesh>
      <mesh>
        <sphereGeometry args={[2.5, 12, 12]} />
        <meshBasicMaterial ref={glowRef} color="#00f5ff" transparent opacity={0.012} blending={ADD} depthWrite={false} />
      </mesh>
    </group>
  );
}

function LanceMesh({ activationTime }: { activationTime: number }) {
  const meshRef = useRef<THREE.Mesh>(null);
  const coreRef = useRef<THREE.MeshBasicMaterial>(null);
  const glowRef = useRef<THREE.MeshBasicMaterial>(null);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % CYCLE;
    const isActive = cycle >= activationTime;
    const ramp = isActive ? Math.min(1, (cycle - activationTime) * 4) : 0;

    if (meshRef.current) {
      const targetX = isActive ? 0 : -8;
      meshRef.current.position.x += (targetX - meshRef.current.position.x) * 0.06;
    }
    if (coreRef.current) {
      coreRef.current.opacity += (ramp * 0.95 - coreRef.current.opacity) * 0.08;
    }
    if (glowRef.current) {
      const glow = isActive ? 0.35 + Math.sin(t * 8) * 0.1 : 0;
      glowRef.current.opacity += (glow * ramp - glowRef.current.opacity) * 0.08;
    }
  });

  return (
    <group ref={meshRef} position={[-8, 0, 0]}>
      {/* Bright core lance */}
      <mesh rotation={[0, 0, Math.PI / 2]}>
        <cylinderGeometry args={[0.018, 0.004, 7, 8]} />
        <meshBasicMaterial ref={coreRef} color="#ffffff" transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
      {/* Colored bloom halo */}
      <mesh rotation={[0, 0, Math.PI / 2]}>
        <cylinderGeometry args={[0.09, 0.02, 7, 8]} />
        <meshBasicMaterial ref={glowRef} color="#00ff88" transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
    </group>
  );
}

function EditResult() {
  const ref = useRef<HTMLDivElement>(null);
  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % CYCLE;
    if (ref.current) {
      const cur = parseFloat(ref.current.style.opacity || "0");
      ref.current.style.opacity = String(cur + ((cycle >= 3.5 ? 1 : 0) - cur) * 0.07);
    }
  });
  return (
    <Html position={[0, -2.9, 0]} center distanceFactor={12} zIndexRange={[2, 0]} style={{ pointerEvents: "none" }}>
      <div ref={ref} style={{
        background: "rgba(5,5,22,0.95)",
        border: "1px solid #00ff8840",
        borderRadius: "6px",
        padding: "5px 10px",
        fontSize: "12px",
        lineHeight: "1.6",
        color: "#00ff88",
        fontFamily: '"Space Mono", monospace',
        whiteSpace: "nowrap",
        opacity: 0,
        pointerEvents: "none",
        boxShadow: "0 0 14px #00ff8828",
      }}>
        edit applied · 1 node modified
        <div style={{ color: "#4a6080", fontSize: "10px", marginTop: "1.5px" }}>
          re-indexing 2 edges
        </div>
      </div>
    </Html>
  );
}

function SurgicalScene() {
  return (
    <GraphCanvas cameraPos={[0, 3, 14]}>
      <Stars radius={60} depth={50} count={1200} factor={3} saturation={0} fade speed={0.5} />
      <CameraRig
        targets={[[0, 3, 14], [5, 2, 10], [-3, 4, 12], [0, 1, 11]]}
        secondsPerWaypoint={7}
        spring={0.04}
        damping={0.83}
      />
      <ContainmentSphere />
      <NamedNode position={target.pos} label={target.label} sublabel={target.sub}
        color={target.color} activationTime={target.at} cycleLength={CYCLE} emitPulse pulsePeriod={2.2} />
      {callers.map((n, i) => (
        <NamedNode key={`c${i}`} position={n.pos} label={n.label} sublabel={n.sub}
          color={n.color} activationTime={n.at} cycleLength={CYCLE} />
      ))}
      {callees.map((n, i) => (
        <NamedNode key={`e${i}`} position={n.pos} label={n.label} sublabel={n.sub}
          color={n.color} activationTime={n.at} cycleLength={CYCLE} />
      ))}
      {callers.map((n, i) => (
        <FlowEdge key={`cf${i}`} start={n.pos} end={target.pos} color="#ffb700"
          activationTime={n.at} cycleLength={CYCLE} speed={0.28} />
      ))}
      {callees.map((n, i) => (
        <FlowEdge key={`ef${i}`} start={target.pos} end={n.pos} color="#00ff88"
          activationTime={n.at} cycleLength={CYCLE} speed={0.28} />
      ))}
      <LanceMesh activationTime={3.2} />
      <EditResult />
    </GraphCanvas>
  );
}

export function SurgicalFeature() {
  return (
    <FeatureSection
      title="Edit One Node. Touch Nothing Else."
      subtitle="Surgical Edit"
      description="When your agent edits handleAuth(), m1nd provides exactly what it needs: the function itself, its 2 callers, its 3 callees, and its tests. Not the full file. Not adjacent functions. A precise context slice that prevents hallucination and eliminates token waste."
      align="left"
    >
      <SurgicalScene />
    </FeatureSection>
  );
}
