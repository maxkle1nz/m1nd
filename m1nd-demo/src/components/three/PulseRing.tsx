import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";

export function PulseRing({ position = [0, 0, 0], color = "#00f5ff", maxRadius = 10, speed = 2 }: any) {
  const ref = useRef<THREE.Mesh>(null);
  
  useFrame(({ clock }) => {
    if (ref.current) {
      const t = (clock.getElapsedTime() * speed) % 1;
      const radius = t * maxRadius;
      const scale = radius;
      ref.current.scale.set(scale, scale, 1);
      
      const material = ref.current.material as THREE.MeshBasicMaterial;
      material.opacity = (1 - t) * 0.8;
    }
  });

  return (
    <mesh position={position} ref={ref}>
      <ringGeometry args={[0.9, 1, 64]} />
      <meshBasicMaterial color={color} transparent opacity={0.8} side={THREE.DoubleSide} />
    </mesh>
  );
}
