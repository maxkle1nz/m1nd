import { FeatureSection } from "./FeatureSection";
import { GraphCanvas } from "./three/GraphCanvas";
import { CameraRig } from "./three/CameraRig";
import { Stars } from "@react-three/drei";
import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { NamedNode } from "./three/NamedNode";
import { FlowEdge } from "./three/FlowEdge";

const CYCLE = 10;
const ADD = THREE.AdditiveBlending;

const visitedNodes = [
  { pos: [4, 0, 0] as [number,number,number], label: "auth/index.ts", sub: "visited 2 days ago", color: "#ffb700", at: 0.5 },
  { pos: [2, 3.5, 1] as [number,number,number], label: "db/queries.ts", sub: "visited 2 days ago", color: "#ffb700", at: 1.1 },
  { pos: [-2, 3.5, -1] as [number,number,number], label: "api/middleware.ts", sub: "visited 1 day ago", color: "#ffb700", at: 1.7 },
  { pos: [-4, 0, 0.5] as [number,number,number], label: "models/User.ts", sub: "visited 1 day ago", color: "#ffb700", at: 2.3 },
  { pos: [-1.5, -3.5, 0] as [number,number,number], label: "hooks/useAuth.ts", sub: "visited today", color: "#00ff88", at: 2.9 },
  { pos: [1.5, -3.5, -1] as [number,number,number], label: "types/session.ts", sub: "visited today", color: "#00ff88", at: 3.5 },
];

const trailEdges = [
  { from: 0, to: 1 }, { from: 1, to: 2 }, { from: 2, to: 3 },
  { from: 3, to: 4 }, { from: 4, to: 5 }, { from: 5, to: 0 },
];

function OrbitalRing({ radius, tilt, speed, color, activationTime }: {
  radius: number; tilt: number; speed: number; color: string; activationTime: number;
}) {
  const groupRef = useRef<THREE.Group>(null);
  const matRef = useRef<THREE.MeshBasicMaterial>(null);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % CYCLE;
    const isActive = cycle >= activationTime;
    if (groupRef.current) groupRef.current.rotation.z += speed * 0.0018;
    if (matRef.current) {
      const tgt = isActive ? 0.18 + Math.sin(t * 0.4) * 0.06 : 0.01;
      matRef.current.opacity += (tgt - matRef.current.opacity) * 0.04;
    }
  });

  return (
    <group ref={groupRef} rotation={[tilt, 0, 0]}>
      <mesh>
        <torusGeometry args={[radius, 0.03, 8, 80]} />
        <meshBasicMaterial ref={matRef} color={color} transparent opacity={0.01} blending={ADD} depthWrite={false} />
      </mesh>
    </group>
  );
}

function MemoryScene() {
  return (
    <GraphCanvas cameraPos={[2, 8, 16]}>
      <Stars radius={80} depth={60} count={1500} factor={3.5} saturation={0} fade speed={0.6} />
      <CameraRig
        targets={[[2, 8, 16], [-5, 6, 14], [4, 4, 15]]}
        secondsPerWaypoint={8}
        spring={0.032}
        damping={0.85}
      />
      <OrbitalRing radius={5} tilt={0.4} speed={0.3} color="#ffb700" activationTime={0.5} />
      <OrbitalRing radius={5.8} tilt={-0.6} speed={-0.2} color="#ffb700" activationTime={1.0} />
      <OrbitalRing radius={4.5} tilt={0.8} speed={0.15} color="#00ff88" activationTime={2.5} />
      {visitedNodes.map((n, i) => (
        <NamedNode key={i} position={n.pos} label={n.label} sublabel={n.sub} color={n.color}
          activationTime={n.at} cycleLength={CYCLE} />
      ))}
      {trailEdges.map((e, i) => (
        <FlowEdge key={i} start={visitedNodes[e.from].pos} end={visitedNodes[e.to].pos}
          color="#ffb700" activationTime={visitedNodes[e.to].at} cycleLength={CYCLE}
          speed={0.18} particleCount={2} />
      ))}
    </GraphCanvas>
  );
}

export function MemoryFeature() {
  return (
    <FeatureSection
      title="Pick Up Exactly Where It Left Off"
      subtitle="Persistent Memory"
      description="When your agent returns to an investigation across sessions, m1nd restores the full trail — which nodes were visited, in what order, across how many hops. No re-reading files. No reconstructing context from scratch. Continued in milliseconds."
      align="left"
    >
      <MemoryScene />
    </FeatureSection>
  );
}
