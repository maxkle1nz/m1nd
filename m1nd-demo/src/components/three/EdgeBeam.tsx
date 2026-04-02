import { useRef, useLayoutEffect } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";

interface EdgeBeamProps {
  start: [number, number, number];
  end: [number, number, number];
  color?: string;
  thickness?: number;
  opacity?: number;
  animated?: boolean;
}

export function EdgeBeam({
  start,
  end,
  color = "#00f5ff",
  thickness = 0.05,
  opacity = 0.6,
  animated = true,
}: EdgeBeamProps) {
  const meshRef = useRef<THREE.Mesh>(null);

  const startVec = new THREE.Vector3(...start);
  const endVec = new THREE.Vector3(...end);
  const distance = startVec.distanceTo(endVec);
  const midpoint = startVec.clone().lerp(endVec, 0.5).toArray() as [number, number, number];

  useLayoutEffect(() => {
    if (!meshRef.current) return;
    const dir = endVec.clone().sub(startVec).normalize();
    const up = new THREE.Vector3(0, 1, 0);
    const quaternion = new THREE.Quaternion().setFromUnitVectors(up, dir);
    meshRef.current.setRotationFromQuaternion(quaternion);
  });

  useFrame(({ clock }) => {
    if (!meshRef.current || !animated) return;
    const mat = meshRef.current.material as THREE.MeshBasicMaterial;
    const t = Math.sin(clock.getElapsedTime() * 1.5) * 0.5 + 0.5;
    mat.opacity = opacity * (0.5 + t * 0.5);
  });

  return (
    <mesh ref={meshRef} position={midpoint}>
      <cylinderGeometry args={[thickness, thickness, distance, 6]} />
      <meshBasicMaterial color={color} transparent opacity={opacity} />
    </mesh>
  );
}
