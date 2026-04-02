import { Html } from "@react-three/drei";
import { useRef, useMemo } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import * as THREE from "three";

const ADD = THREE.AdditiveBlending;

function hashPos(p: [number, number, number]): number {
  return Math.abs(Math.sin(p[0] * 127.1 + p[1] * 311.7 + p[2] * 74.7)) * Math.PI * 2;
}

interface PulseRingProps {
  color: string;
  period: number;
  maxR: number;
}

function PulseRingEmit({ color, period, maxR }: PulseRingProps) {
  const r1 = useRef<THREE.Mesh>(null);
  const r2 = useRef<THREE.Mesh>(null);
  const { camera } = useThree();

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    [r1, r2].forEach((ref, i) => {
      if (!ref.current) return;
      const phase = (t / period + i * 0.5) % 1;
      // Cubic ease-out expansion: rapid start, graceful fade
      const eased = 1 - Math.pow(1 - phase, 2.2);
      ref.current.scale.set(eased * maxR, eased * maxR, 1);
      // Opacity: full early, then sudden drop off at 60%
      const mat = ref.current.material as THREE.MeshBasicMaterial;
      mat.opacity = phase < 0.65 ? (1 - phase / 0.65) * 0.55 : 0;
      // Always face camera
      ref.current.lookAt(camera.position);
    });
  });

  return (
    <>
      <mesh ref={r1}>
        <ringGeometry args={[0.82, 1, 64]} />
        <meshBasicMaterial
          color={color}
          transparent
          opacity={0.55}
          side={THREE.DoubleSide}
          blending={ADD}
          depthWrite={false}
        />
      </mesh>
      <mesh ref={r2}>
        <ringGeometry args={[0.82, 1, 64]} />
        <meshBasicMaterial
          color={color}
          transparent
          opacity={0}
          side={THREE.DoubleSide}
          blending={ADD}
          depthWrite={false}
        />
      </mesh>
    </>
  );
}

export interface NamedNodeProps {
  position: [number, number, number];
  label: string;
  sublabel?: string;
  color?: string;
  activationTime?: number;
  cycleLength?: number;
  size?: number;
  emitPulse?: boolean;
  pulsePeriod?: number;
}

export function NamedNode({
  position,
  label,
  sublabel,
  color = "#00f5ff",
  activationTime = 0,
  cycleLength = 9,
  size = 0.22,
  emitPulse = false,
  pulsePeriod = 2.5,
}: NamedNodeProps) {
  const driftGroupRef = useRef<THREE.Group>(null);
  const coreRef = useRef<THREE.MeshBasicMaterial>(null);
  const corona1Ref = useRef<THREE.MeshBasicMaterial>(null);
  const corona2Ref = useRef<THREE.MeshBasicMaterial>(null);
  const ambientRef = useRef<THREE.MeshBasicMaterial>(null);
  const labelRef = useRef<HTMLDivElement>(null);

  const colorObj = useMemo(() => new THREE.Color(color), [color]);
  const coreColor = useMemo(
    () => new THREE.Color(color).lerp(new THREE.Color("#ffffff"), 0.5),
    [color]
  );
  const dim = useMemo(() => new THREE.Color("#05050e"), []);

  // Unique drift phase per node — so all nodes breathe differently
  const driftPhase = useMemo(() => hashPos(position), []);
  const wasActive = useRef(false);
  const flash = useRef(0);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    const cycle = t % cycleLength;
    const isActive = cycle >= activationTime;

    // Detect rising edge → activation flash
    if (isActive && !wasActive.current) {
      wasActive.current = true;
      flash.current = 5.0;
    }
    if (!isActive) wasActive.current = false;
    flash.current = Math.max(1.0, flash.current * 0.86);

    // Organic position drift on inner group
    if (driftGroupRef.current) {
      driftGroupRef.current.position.x = Math.sin(t * 0.73 + driftPhase) * 0.05;
      driftGroupRef.current.position.y = Math.cos(t * 0.51 + driftPhase * 1.31) * 0.05;
      driftGroupRef.current.position.z = Math.sin(t * 0.61 + driftPhase * 0.71) * 0.03;
    }

    const fi = flash.current;
    const breathe = 0.85 + Math.sin(t * 1.4 + driftPhase) * 0.15;

    // Layer 1 — bright core (white-ish, small)
    if (coreRef.current) {
      coreRef.current.color.lerpColors(dim, coreColor, isActive ? 1 : 0.04);
      const tgt = isActive ? Math.min(fi, 1.0) : 0.04;
      coreRef.current.opacity += (tgt - coreRef.current.opacity) * 0.14;
    }

    // Layer 2 — inner corona (full color, 3× size)
    if (corona1Ref.current) {
      corona1Ref.current.color.lerpColors(dim, colorObj, isActive ? 1 : 0);
      const tgt = isActive ? Math.min(fi * 0.48, 0.72) * breathe : 0.0;
      corona1Ref.current.opacity += (tgt - corona1Ref.current.opacity) * 0.1;
    }

    // Layer 3 — mid glow (5× size, pulsing)
    if (corona2Ref.current) {
      const tgt = isActive ? (0.18 + Math.sin(t * 1.7 + driftPhase) * 0.06) * breathe : 0.0;
      corona2Ref.current.opacity += (tgt - corona2Ref.current.opacity) * 0.07;
    }

    // Layer 4 — outer ambient (10× size, ultra-faint)
    if (ambientRef.current) {
      const tgt = isActive ? (0.055 + Math.sin(t * 0.9 + driftPhase) * 0.018) : 0.0;
      ambientRef.current.opacity += (tgt - ambientRef.current.opacity) * 0.05;
    }

    // Label fade
    if (labelRef.current) {
      const cur = parseFloat(labelRef.current.style.opacity || "0");
      labelRef.current.style.opacity = String(cur + ((isActive ? 1 : 0) - cur) * 0.1);
    }
  });

  return (
    <group position={position}>
      <group ref={driftGroupRef}>
        {/* Core — tiny white-ish center */}
        <mesh>
          <sphereGeometry args={[size, 24, 24]} />
          <meshBasicMaterial
            ref={coreRef}
            color="#05050e"
            transparent
            opacity={0.04}
            blending={ADD}
            depthWrite={false}
          />
        </mesh>

        {/* Inner corona */}
        <mesh>
          <sphereGeometry args={[size * 2.8, 16, 16]} />
          <meshBasicMaterial
            ref={corona1Ref}
            color={color}
            transparent
            opacity={0.0}
            blending={ADD}
            depthWrite={false}
          />
        </mesh>

        {/* Mid glow */}
        <mesh>
          <sphereGeometry args={[size * 5.5, 12, 12]} />
          <meshBasicMaterial
            ref={corona2Ref}
            color={color}
            transparent
            opacity={0.0}
            blending={ADD}
            depthWrite={false}
          />
        </mesh>

        {/* Outer ambient */}
        <mesh>
          <sphereGeometry args={[size * 10, 8, 8]} />
          <meshBasicMaterial
            ref={ambientRef}
            color={color}
            transparent
            opacity={0.0}
            blending={ADD}
            depthWrite={false}
          />
        </mesh>

        {emitPulse && <PulseRingEmit color={color} period={pulsePeriod} maxR={5} />}

        <Html center distanceFactor={12} zIndexRange={[2, 0]} style={{ pointerEvents: "none" }}>
          <div
            ref={labelRef}
            style={{
              background: "rgba(5,5,18,0.92)",
              border: `1px solid ${color}40`,
              borderRadius: "4px",
              padding: "3px 8px",
              fontSize: "12px",
              lineHeight: "1.55",
              color,
              fontFamily: '"Space Mono", "Courier New", monospace',
              whiteSpace: "nowrap",
              transform: "translateY(-30px)",
              opacity: 0,
              userSelect: "none",
              pointerEvents: "none",
              boxShadow: `0 0 12px ${color}30, inset 0 0 8px ${color}08`,
              letterSpacing: "0.02em",
            }}
          >
            {label}
            {sublabel && (
              <div
                style={{
                  color: "#4a6080",
                  fontSize: "10px",
                  marginTop: "1.5px",
                  letterSpacing: "0.01em",
                }}
              >
                {sublabel}
              </div>
            )}
          </div>
        </Html>
      </group>
    </group>
  );
}
