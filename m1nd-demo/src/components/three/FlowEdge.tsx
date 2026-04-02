import { useRef, useMemo } from "react";
import { useFrame } from "@react-three/fiber";
import { Line } from "@react-three/drei";
import * as THREE from "three";

const ADD = THREE.AdditiveBlending;

// Comet trail: [position lag from main, opacity fraction, scale fraction]
const TRAIL_CFG = [
  [0.000, 1.00, 1.00],
  [0.032, 0.55, 0.78],
  [0.062, 0.28, 0.56],
  [0.092, 0.11, 0.36],
] as const;

interface CometParticleProps {
  start: THREE.Vector3;
  end: THREE.Vector3;
  phase: number;
  speed: number;
  color: string;
  activationTime: number;
  cycleLength: number;
  baseSize: number;
}

function CometParticle({
  start,
  end,
  phase,
  speed,
  color,
  activationTime,
  cycleLength,
  baseSize,
}: CometParticleProps) {
  // Explicit refs — never put useRef() inside an array
  const ref0 = useRef<THREE.Mesh>(null);
  const ref1 = useRef<THREE.Mesh>(null);
  const ref2 = useRef<THREE.Mesh>(null);
  const ref3 = useRef<THREE.Mesh>(null);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % cycleLength;
    const isActive = cycle >= activationTime;
    const ramp = isActive ? Math.min(1, (cycle - activationTime) * 2.5) : 0;
    const baseT = ((t * speed + phase) % 1 + 1) % 1;

    const refList = [ref0, ref1, ref2, ref3] as const;

    TRAIL_CFG.forEach(([lag, opMult, scaleMult], i) => {
      const ref = refList[i];
      if (!ref.current) return;
      const mat = ref.current.material as THREE.MeshBasicMaterial;

      if (isActive) {
        const flowT = ((baseT - lag) % 1 + 1) % 1;
        ref.current.position.lerpVectors(start, end, flowT);
        // Sinusoidal size envelope — largest at midpoint of path
        const sizeEnv = 0.5 + 0.5 * Math.sin(Math.PI * flowT);
        ref.current.scale.setScalar(sizeEnv * (scaleMult as number));
        mat.opacity = ramp * (opMult as number);
      } else {
        mat.opacity = Math.max(0, mat.opacity - 0.07);
        ref.current.scale.setScalar(1);
      }
    });
  });

  return (
    <>
      <mesh ref={ref0} position={start.toArray() as [number, number, number]}>
        <sphereGeometry args={[baseSize, 8, 8]} />
        <meshBasicMaterial color={color} transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
      <mesh ref={ref1} position={start.toArray() as [number, number, number]}>
        <sphereGeometry args={[baseSize * 0.88, 7, 7]} />
        <meshBasicMaterial color={color} transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
      <mesh ref={ref2} position={start.toArray() as [number, number, number]}>
        <sphereGeometry args={[baseSize * 0.72, 6, 6]} />
        <meshBasicMaterial color={color} transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
      <mesh ref={ref3} position={start.toArray() as [number, number, number]}>
        <sphereGeometry args={[baseSize * 0.56, 5, 5]} />
        <meshBasicMaterial color={color} transparent opacity={0} blending={ADD} depthWrite={false} />
      </mesh>
    </>
  );
}

export interface FlowEdgeProps {
  start: [number, number, number];
  end: [number, number, number];
  color?: string;
  activationTime?: number;
  cycleLength?: number;
  speed?: number;
  particleCount?: number;
  lineWidth?: number;
}

export function FlowEdge({
  start,
  end,
  color = "#00f5ff",
  activationTime = 0,
  cycleLength = 9,
  speed = 0.35,
  particleCount = 3,
  lineWidth = 1,
}: FlowEdgeProps) {
  const bloomRef = useRef<any>(null);
  const coreRef = useRef<any>(null);

  const startVec = useMemo(
    () => new THREE.Vector3(...start),
    [start[0], start[1], start[2]]
  );
  const endVec = useMemo(
    () => new THREE.Vector3(...end),
    [end[0], end[1], end[2]]
  );

  const particleSize = useMemo(() => {
    const d = startVec.distanceTo(endVec);
    return Math.max(0.045, Math.min(0.085, d * 0.015));
  }, [startVec, endVec]);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % cycleLength;
    const isActive = cycle >= activationTime;
    const ramp = isActive ? Math.min(1, (cycle - activationTime) * 2.0) : 0;

    const bloomMaterial = bloomRef.current?.material as THREE.Material & { opacity?: number } | undefined;
    if (bloomMaterial && typeof bloomMaterial.opacity === "number") {
      const tgt = (0.25 + Math.sin(t * 1.1 + start[0]) * 0.04) * ramp;
      bloomMaterial.opacity += (tgt - bloomMaterial.opacity) * 0.07;
    }
    const coreMaterial = coreRef.current?.material as THREE.Material & { opacity?: number } | undefined;
    if (coreMaterial && typeof coreMaterial.opacity === "number") {
      const tgt = (0.52 + Math.sin(t * 2.3 + end[0] + 0.7) * 0.07) * ramp;
      coreMaterial.opacity += (tgt - coreMaterial.opacity) * 0.09;
    }
  });

  return (
    <>
      {/* Colored bloom halo */}
      <Line
        ref={bloomRef}
        points={[start, end]}
        color={color}
        lineWidth={lineWidth * 3.5}
        transparent
        opacity={0.0}
        blending={ADD}
        depthWrite={false}
      />
      {/* White laser core */}
      <Line
        ref={coreRef}
        points={[start, end]}
        color="#ffffff"
        lineWidth={lineWidth * 0.85}
        transparent
        opacity={0.0}
        blending={ADD}
        depthWrite={false}
      />
      {/* Comet trail particles */}
      {Array.from({ length: particleCount }).map((_, i) => (
        <CometParticle
          key={i}
          start={startVec}
          end={endVec}
          phase={i / particleCount}
          speed={speed}
          color={color}
          activationTime={activationTime}
          cycleLength={cycleLength}
          baseSize={particleSize}
        />
      ))}
    </>
  );
}
