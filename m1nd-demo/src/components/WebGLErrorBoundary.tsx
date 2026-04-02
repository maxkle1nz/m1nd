import { Component, ReactNode } from "react";

interface Props {
  children: ReactNode;
  color?: string;
}

interface State {
  hasError: boolean;
}

function GraphFallback({ color = "#00f5ff" }: { color?: string }) {
  const nodes = [
    { cx: 400, cy: 300, r: 14, c: color },
    { cx: 200, cy: 180, r: 10, c: "#00ff88" },
    { cx: 600, cy: 150, r: 10, c: "#7b61ff" },
    { cx: 680, cy: 380, r: 8,  c: color },
    { cx: 150, cy: 380, r: 8,  c: "#00ff88" },
    { cx: 450, cy: 460, r: 9,  c: "#ff6b00" },
    { cx: 300, cy: 460, r: 7,  c: "#7b61ff" },
    { cx: 560, cy: 490, r: 7,  c: color },
    { cx: 100, cy: 260, r: 6,  c: "#7b61ff" },
    { cx: 750, cy: 260, r: 6,  c: "#00ff88" },
  ];
  const edges = [
    [0,1],[0,2],[0,3],[0,4],[0,5],[1,4],[1,8],[2,3],[2,9],[5,6],[5,7],[3,9],[4,6],
  ];
  return (
    <div className="absolute inset-0 overflow-hidden">
      <svg
        viewBox="0 0 800 600"
        className="w-full h-full"
        preserveAspectRatio="xMidYMid slice"
        aria-hidden="true"
      >
        <defs>
          <radialGradient id="bgGrad" cx="50%" cy="50%" r="60%">
            <stop offset="0%" stopColor={color} stopOpacity="0.06" />
            <stop offset="100%" stopColor="transparent" stopOpacity="0" />
          </radialGradient>
          {nodes.map((n, i) => (
            <radialGradient key={i} id={`ng${i}`} cx="50%" cy="50%" r="50%">
              <stop offset="0%" stopColor={n.c} stopOpacity="0.9" />
              <stop offset="100%" stopColor={n.c} stopOpacity="0.3" />
            </radialGradient>
          ))}
        </defs>
        <rect width="800" height="600" fill="url(#bgGrad)" />
        {edges.map(([a, b], i) => (
          <line
            key={i}
            x1={nodes[a].cx} y1={nodes[a].cy}
            x2={nodes[b].cx} y2={nodes[b].cy}
            stroke={nodes[a].c}
            strokeWidth="1"
            strokeOpacity="0.18"
          />
        ))}
        {nodes.map((n, i) => (
          <g key={i}>
            <circle cx={n.cx} cy={n.cy} r={n.r * 2.5} fill={n.c} fillOpacity="0.05" />
            <circle cx={n.cx} cy={n.cy} r={n.r} fill={`url(#ng${i})`} />
          </g>
        ))}
      </svg>
    </div>
  );
}

export class WebGLErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError() {
    return { hasError: true };
  }

  render() {
    if (this.state.hasError) {
      return <GraphFallback color={this.props.color} />;
    }
    return this.props.children;
  }
}
