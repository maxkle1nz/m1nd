import { FeatureSection } from "./FeatureSection";
import { GraphCanvas } from "./three/GraphCanvas";
import { CameraRig } from "./three/CameraRig";
import { Stars } from "@react-three/drei";
import { NamedNode } from "./three/NamedNode";
import { FlowEdge } from "./three/FlowEdge";

const CYCLE = 9;

const nodes = [
  { pos: [0, 0, 0] as [number,number,number], label: "auth/service.ts", sub: "export class AuthService", color: "#00f5ff", at: 0.3, pulse: true },
  { pos: [3.5, 0.5, 0.5] as [number,number,number], label: "refresh.ts", sub: "refreshAccessToken()", color: "#00f5ff", at: 1.4 },
  { pos: [5.5, 2.5, 0] as [number,number,number], label: "TokenStore.ts", sub: "get(key: string)", color: "#00ff88", at: 2.4 },
  { pos: [3.5, 3.5, -1.5] as [number,number,number], label: "JWTValidator.ts", sub: "verify(token, secret)", color: "#00ff88", at: 2.9 },
  { pos: [-3.5, 1.5, 1] as [number,number,number], label: "SessionMgr.ts", sub: "resume(sessionId)", color: "#ffb700", at: 3.5 },
  { pos: [-2.5, -2.5, 0.5] as [number,number,number], label: "config.ts", sub: "JWT_SECRET, EXPIRY", color: "#ffb700", at: 4.2 },
];

const edges = [
  { from: 0, to: 1, color: "#00f5ff", at: 1.4 },
  { from: 1, to: 2, color: "#00ff88", at: 2.4 },
  { from: 1, to: 3, color: "#00ff88", at: 2.9 },
  { from: 0, to: 4, color: "#ffb700", at: 3.5 },
  { from: 4, to: 5, color: "#ffb700", at: 4.2 },
];

function OrientationScene() {
  return (
    <GraphCanvas cameraPos={[0, 6, 18]}>
      <Stars radius={80} depth={60} count={2200} factor={3.5} saturation={0} fade speed={0.7} />
      <CameraRig
        targets={[[0, 6, 18], [5, 3, 16], [-3, 4.5, 15], [2, 2, 14]]}
        secondsPerWaypoint={7}
        spring={0.035}
        damping={0.82}
      />
      {nodes.map((n, i) => (
        <NamedNode
          key={i}
          position={n.pos}
          label={n.label}
          sublabel={n.sub}
          color={n.color}
          activationTime={n.at}
          cycleLength={CYCLE}
          emitPulse={n.pulse}
          pulsePeriod={2}
        />
      ))}
      {edges.map((e, i) => (
        <FlowEdge
          key={i}
          start={nodes[e.from].pos}
          end={nodes[e.to].pos}
          color={e.color}
          activationTime={e.at}
          cycleLength={CYCLE}
          speed={0.3}
        />
      ))}
    </GraphCanvas>
  );
}

export function OrientationFeature() {
  return (
    <FeatureSection
      id="features"
      title="Your Agent Starts in the Right Place"
      subtitle="Orientation"
      description="Ask m1nd where token refresh happens. In 1.36µs it returns the exact subgraph — callers, callees, function signatures — already assembled. Your agent begins with a complete map, not a stack of grep matches it has to make sense of."
      align="left"
    >
      <OrientationScene />
    </FeatureSection>
  );
}
