/*
 * Wires — the typed connection layer under the cards (HUMAN-VIEW-V2-SCREENS §1.1;
 * PRD F3). Each output socket becomes a curve from the source card to the target
 * card (resolved by the socket's `to` block NAME), with a bead at the midpoint
 * (edge evidence is first-class — today the bead is empty: no edge receipt yet).
 * External sockets render as a short stub carrying the class/alias label. All
 * socket-blue, drawn BEHIND the cards (pointer-events: none) so cards stay
 * clickable. Read-only; no zoom, no drag (F1) — a stable projection of the
 * skeleton's declared sockets.
 */
import { CARD_H, CARD_W, blockIndexByName, type Point, type SystemBlockStore } from '../../lib/buildMap';

const STROKE = 1.5;
const BLUE = 'var(--socket-blue)';

/** Cubic-bezier point at t=0.5 (for the bead): (P0 + 3P1 + 3P2 + P3)/8. */
function mid(p0: Point, p1: Point, p2: Point, p3: Point): Point {
  return {
    x: (p0.x + 3 * p1.x + 3 * p2.x + p3.x) / 8,
    y: (p0.y + 3 * p1.y + 3 * p2.y + p3.y) / 8,
  };
}

export interface WiresProps {
  store: SystemBlockStore;
  positions: Point[];
  width: number;
  height: number;
}

export default function Wires({ store, positions, width, height }: WiresProps) {
  const indexByName = blockIndexByName(store);

  const curves: React.ReactNode[] = [];
  const stubs: React.ReactNode[] = [];

  store.blocks.forEach((block, i) => {
    const pos = positions[i];
    if (!pos) return;
    const outs = block.sockets.outputs.filter((s) => s.to != null);
    outs.forEach((s, k) => {
      const targetIdx = s.to != null ? indexByName.get(s.to) : undefined;
      if (targetIdx == null) return; // a dangling socket is surfaced in the rollup (broken), not drawn as a fake wire
      const tp = positions[targetIdx];
      if (!tp) return;
      // Fan multiple outputs off the source's right edge so they don't overlap.
      const spread = (k - (outs.length - 1) / 2) * 14;
      const p0: Point = { x: pos.x + CARD_W, y: pos.y + CARD_H / 2 + spread };
      const p3: Point = { x: tp.x, y: tp.y + CARD_H / 2 };
      const dx = Math.max(40, Math.abs(p3.x - p0.x) / 2);
      const c1: Point = { x: p0.x + dx, y: p0.y };
      const c2: Point = { x: p3.x - dx, y: p3.y };
      const m = mid(p0, c1, c2, p3);
      const key = `${block.block_id}->${s.to}#${k}`;
      curves.push(
        <path
          key={key}
          data-role="wire"
          d={`M ${p0.x} ${p0.y} C ${c1.x} ${c1.y}, ${c2.x} ${c2.y}, ${p3.x} ${p3.y}`}
          style={{ stroke: BLUE, fill: 'none', opacity: 0.55 }}
          strokeWidth={STROKE}
        />,
      );
      curves.push(
        <circle
          key={`${key}-bead`}
          data-role="wire-bead"
          cx={m.x}
          cy={m.y}
          r={3}
          style={{ fill: 'var(--warm-paper)', stroke: BLUE }}
          strokeWidth={STROKE}
        />,
      );
    });

    // External sockets — a short stub + the class/alias label (no target block).
    block.sockets.external.forEach((s, k) => {
      const label = s.alias ?? s.class ?? 'external';
      const y = pos.y + CARD_H / 2 + (k + 1) * 14;
      const x0 = pos.x + CARD_W;
      stubs.push(
        <g key={`${block.block_id}-ext-${k}`} data-role="external-stub">
          <path
            d={`M ${x0} ${y} L ${x0 + 34} ${y}`}
            style={{ stroke: BLUE, fill: 'none', opacity: 0.55 }}
            strokeWidth={STROKE}
            strokeDasharray="3 3"
          />
          <circle cx={x0 + 34} cy={y} r={2.5} style={{ fill: BLUE }} />
          <text x={x0 + 40} y={y + 3} className="fill-ink-soft" style={{ fontSize: 9 }} fontFamily="IBM Plex Mono, monospace">
            {label}
          </text>
        </g>,
      );
    });
  });

  return (
    <svg
      data-role="wires"
      width={width}
      height={height}
      className="absolute inset-0 pointer-events-none"
      aria-hidden
    >
      {curves}
      {stubs}
    </svg>
  );
}
