import { GraphCanvas } from "./GraphCanvas";
import { OrbitControls, Stars } from "@react-three/drei";
import { PulseRing } from "./PulseRing";
import { EdgeBeam } from "./EdgeBeam";
import { useRef, useMemo } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";

function Nodes({ count = 50 }) {
  const mesh = useRef<THREE.InstancedMesh>(null);
  const dummy = useMemo(() => new THREE.Object3D(), []);
  
  const particles = useMemo(() => {
    const temp = [];
    for (let i = 0; i < count; i++) {
      const x = (Math.random() - 0.5) * 20;
      const y = (Math.random() - 0.5) * 20;
      const z = (Math.random() - 0.5) * 20;
      temp.push({ x, y, z });
    }
    return temp;
  }, [count]);
  
  useFrame(() => {
    if (!mesh.current) return;
    particles.forEach((particle, i) => {
      dummy.position.set(particle.x, particle.y, particle.z);
      dummy.updateMatrix();
      mesh.current!.setMatrixAt(i, dummy.matrix);
    });
    mesh.current.instanceMatrix.needsUpdate = true;
  });
  
  return (
    <instancedMesh ref={mesh} args={[undefined, undefined, count]}>
      <sphereGeometry args={[0.15, 16, 16]} />
      <meshBasicMaterial color="#00f5ff" />
    </instancedMesh>
  );
}

export function GraphScene() {
  return (
    <GraphCanvas cameraPos={[0, 0, 15]}>
      <OrbitControls autoRotate autoRotateSpeed={0.5} enableZoom={false} enablePan={false} />
      <Stars radius={50} depth={50} count={2000} factor={4} saturation={0} fade speed={1} />
      <Nodes count={100} />
      <PulseRing position={[0, 0, 0]} maxRadius={8} speed={1} color="#00f5ff" />
      <EdgeBeam start={[0, 0, 0]} end={[2, 3, -1]} />
      <EdgeBeam start={[0, 0, 0]} end={[-3, -2, 2]} />
      <EdgeBeam start={[0, 0, 0]} end={[4, -1, -3]} />
    </GraphCanvas>
  );
}
