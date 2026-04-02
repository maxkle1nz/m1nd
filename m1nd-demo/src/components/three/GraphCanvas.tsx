import { Canvas } from "@react-three/fiber";
import { Preload } from "@react-three/drei";
import { ReactNode, Suspense } from "react";
import * as THREE from "three";
import { WebGLBoundary } from "./WebGLBoundary";

interface GraphCanvasProps {
  children: ReactNode;
  className?: string;
  cameraPos?: [number, number, number];
  fallback?: ReactNode;
}

function CSSFallback() {
  return (
    <div className="w-full h-full absolute inset-0 overflow-hidden">
      {Array.from({ length: 80 }).map((_, i) => {
        const colorPool = ["#00f5ff", "#00ff88", "#ffb700", "#ff00aa", "#c8d8ff"];
        const c = colorPool[i % colorPool.length];
        return (
          <div
            key={i}
            className="absolute rounded-full"
            style={{
              width: (Math.sin(i * 1.31) * 1.5 + 2) + "px",
              height: (Math.sin(i * 1.31) * 1.5 + 2) + "px",
              left: ((Math.sin(i * 0.37 + 1) * 0.5 + 0.5) * 100) + "%",
              top: ((Math.cos(i * 0.29 + 0.5) * 0.5 + 0.5) * 100) + "%",
              background: c,
              boxShadow: `0 0 6px ${c}`,
              opacity: 0.3 + (Math.sin(i * 0.71) * 0.5 + 0.5) * 0.5,
              animation: `pulse ${2.4 + (i % 7) * 0.4}s ease-in-out infinite alternate`,
              animationDelay: (i % 5) * 0.3 + "s",
            }}
          />
        );
      })}
    </div>
  );
}

/** Subtle ambient point that pulses gently — gives depth to the dark scene */
function SceneLighting() {
  return (
    <>
      <ambientLight intensity={0.12} color="#0a0a20" />
      <pointLight position={[0, 0, 0]} intensity={0.6} color="#1a2040" distance={20} decay={2} />
    </>
  );
}

export function GraphCanvas({
  children,
  className = "",
  cameraPos = [0, 0, 15],
  fallback,
}: GraphCanvasProps) {
  return (
    <div className={`w-full h-full absolute inset-0 ${className}`}>
      <WebGLBoundary fallback={fallback ?? <CSSFallback />}>
        <Canvas
          camera={{ position: cameraPos, fov: 44, near: 0.05, far: 500 }}
          gl={{
            antialias: true,
            alpha: true,
            powerPreference: "high-performance",
            toneMapping: THREE.ACESFilmicToneMapping,
            toneMappingExposure: 1.1,
            failIfMajorPerformanceCaveat: false,
          }}
          dpr={[1, 2]}
          flat={false}
          onCreated={({ gl }) => {
            gl.setPixelRatio(Math.min(window.devicePixelRatio, 2));
            gl.shadowMap.enabled = false;
          }}
        >
          <color attach="background" args={["#050510"]} />
          {/* Depth fog — far clips at 60 instead of 40 for more breathing room */}
          <fog attach="fog" args={["#050510", 15, 60]} />
          <SceneLighting />
          <Suspense fallback={null}>
            {children}
            <Preload all />
          </Suspense>
        </Canvas>
      </WebGLBoundary>
    </div>
  );
}
