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

const epicenter = { pos: [0, 0, 0] as [number,number,number], label: "worker_pool.py", sub: "class WorkerPool", color: "#ff6b00", at: 0.3 };
const ring1 = [
  { pos: [3.5, 0, 0] as [number,number,number], label: "TaskScheduler.py", sub: "schedule(task)", color: "#ff6b00", at: 1.5 },
  { pos: [0, 3.5, 0] as [number,number,number], label: "QueueMgr.py", sub: "enqueue(job)", color: "#ff6b00", at: 1.9 },
  { pos: [-3.5, 0, 0] as [number,number,number], label: "Metrics.py", sub: "record(event)", color: "#ff6b00", at: 2.3 },
];
const ring2 = [
  { pos: [5.5, 2.5, 0.5] as [number,number,number], label: "HealthCheck.py", sub: "ping(worker_id)", color: "#ffb700", at: 3.5 },
  { pos: [2, 5.5, 0] as [number,number,number], label: "APIHandler.py", sub: "dispatch(req)", color: "#ffb700", at: 3.9 },
  { pos: [-5, 2, 0] as [number,number,number], label: "Logger.py", sub: "emit(level, msg)", color: "#ffb700", at: 4.3 },
];

function SupernovaRing({ delay, color, maxR, speed = 4 }: { delay: number; color: string; maxR: number; speed?: number }) {
  const ref = useRef<THREE.Mesh>(null);
  useFrame(({ clock }) => {
    if (!ref.current) return;
    const t = ((clock.getElapsedTime() + delay) % speed) / speed;
    // Cubic ease-out — expands fast then slows
    const eased = 1 - Math.pow(1 - t, 2.4);
    ref.current.scale.set(eased * maxR, eased * maxR, 1);
    // Opacity: bright in first half, fades quickly after
    const mat = ref.current.material as THREE.MeshBasicMaterial;
    mat.opacity = t < 0.5 ? (1 - t * 2) * 0.5 : 0;
  });
  return (
    <mesh ref={ref}>
      <ringGeometry args={[0.88, 1, 72]} />
      <meshBasicMaterial color={color} transparent opacity={0} side={THREE.DoubleSide} blending={ADD} depthWrite={false} />
    </mesh>
  );
}

function ImpactScene() {
  const allNodes = [epicenter, ...ring1, ...ring2];
  return (
    <GraphCanvas cameraPos={[0, 12, 18]}>
      <Stars radius={80} depth={60} count={1800} factor={3.5} saturation={0} fade speed={0.6} />
      <CameraRig
        targets={[[0, 12, 18], [7, 8, 15], [-5, 10, 16]]}
        secondsPerWaypoint={8}
        spring={0.03}
        damping={0.86}
      />
      <SupernovaRing delay={0} color="#ff6b00" maxR={10} speed={3.5} />
      <SupernovaRing delay={1.1} color="#ff6b00" maxR={10} speed={3.5} />
      <SupernovaRing delay={2.2} color="#ffb700" maxR={16} speed={4.5} />
      <SupernovaRing delay={3.3} color="#ffb700" maxR={16} speed={4.5} />
      {allNodes.map((n, i) => (
        <NamedNode
          key={i}
          position={n.pos}
          label={n.label}
          sublabel={n.sub}
          color={n.color}
          activationTime={n.at}
          cycleLength={CYCLE}
          emitPulse={i === 0}
          pulsePeriod={2.2}
        />
      ))}
      {ring1.map((n, i) => (
        <FlowEdge key={`r1-${i}`} start={epicenter.pos} end={n.pos} color="#ff6b00"
          activationTime={n.at} cycleLength={CYCLE} speed={0.25} />
      ))}
      {ring2.map((n, i) => (
        <FlowEdge key={`r2-${i}`} start={ring1[i % ring1.length].pos} end={n.pos} color="#ffb700"
          activationTime={n.at} cycleLength={CYCLE} speed={0.2} />
      ))}
    </GraphCanvas>
  );
}

export function ImpactFeature() {
  return (
    <FeatureSection
      title="Know What Will Break Before You Touch It"
      subtitle="Blast Radius"
      description="Before your agent edits a single line in worker_pool.py, m1nd returns a pre-computed impact cone: 4 direct callers, 4 indirect consumers, sorted by coupling risk. Your agent plans a safe refactor instead of discovering regressions in CI."
      align="right"
    >
      <ImpactScene />
    </FeatureSection>
  );
}
