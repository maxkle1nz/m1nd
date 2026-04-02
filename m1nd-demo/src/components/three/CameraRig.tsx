import { useRef, useMemo } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import * as THREE from "three";

export interface CameraRigProps {
  /** World-space camera positions to visit, in order */
  targets: [number, number, number][];
  /** Point the camera looks toward — default [0,0,0] */
  lookAt?: [number, number, number];
  /** Seconds between waypoint advances */
  secondsPerWaypoint?: number;
  /** Spring constant — higher = faster response */
  spring?: number;
  /** Velocity damping factor — lower = more overshoot */
  damping?: number;
  /** Amplitude of micro-noise for organic drift */
  noiseAmp?: number;
}

/**
 * Spring-physics camera that visits a list of waypoints.
 * Uses velocity accumulation + damping for natural ease-in / ease-out.
 * Adds prime-frequency micro-noise to prevent "locked on rails" feel.
 */
export function CameraRig({
  targets,
  lookAt = [0, 0, 0],
  secondsPerWaypoint = 8,
  spring = 0.038,
  damping = 0.84,
  noiseAmp = 0.06,
}: CameraRigProps) {
  const { camera } = useThree();
  const vecs = useMemo(() => targets.map((t) => new THREE.Vector3(...t)), []);
  const lookAtVec = useMemo(() => new THREE.Vector3(...lookAt), []);

  const vel = useRef(new THREE.Vector3());
  const smoothLookAt = useRef(new THREE.Vector3(...lookAt));
  const waypointIdx = useRef(0);
  const nextChange = useRef(secondsPerWaypoint);
  const delta3 = useRef(new THREE.Vector3());

  useFrame(({ clock, camera: cam }) => {
    const t = clock.getElapsedTime();

    // Advance waypoint on schedule
    if (t >= nextChange.current) {
      waypointIdx.current = (waypointIdx.current + 1) % vecs.length;
      nextChange.current = t + secondsPerWaypoint;
    }

    // Spring force toward current target
    delta3.current.subVectors(vecs[waypointIdx.current], cam.position);
    vel.current.addScaledVector(delta3.current, spring);
    vel.current.multiplyScalar(damping);
    cam.position.add(vel.current);

    // Prime-frequency micro-noise — feels organic, never repeats
    if (noiseAmp > 0) {
      cam.position.x += Math.sin(t * 0.19 + 1.3) * noiseAmp * 0.4;
      cam.position.y += Math.sin(t * 0.13 + 0.7) * noiseAmp * 0.25;
      cam.position.z += Math.sin(t * 0.11 + 2.1) * noiseAmp * 0.15;
    }

    // Smooth lookAt
    smoothLookAt.current.lerp(lookAtVec, 0.04);
    cam.lookAt(smoothLookAt.current);
  });

  return null;
}
