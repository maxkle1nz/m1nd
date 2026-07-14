/*
 * UniverseView — the L0 landing surface (HUMAN-VIEW-V2 F30). The owner's home: a
 * per-world PANORAMA of every existing project brain, rendered from REAL measures
 * only. Calm-tech, paper/observatory, ZERO neon: worlds are textured paper discs
 * sized ∝ node_count (log), lit ∝ freshness (with the age shown honestly beside
 * them), orbited by live-presence satellites, ringed amber-dashed when a human
 * gesture waits. The L0 header is a client-composed serif sentence of universe
 * FACTS — never a cross-brain pulse. Clicking a world opens its existing room (the
 * map). The Landing rides the right column. State-zoom only (no URL router, v1).
 */
import { useMemo } from 'react';
import {
  layoutWorlds,
  universeHeadline,
  worldAgeLabel,
  type UniverseResponse,
  type WorldPlacement,
} from '../../lib/universe';
import TheLanding from './TheLanding';

const VIEW_W = 1000;
const VIEW_H = 640;

interface UniverseViewProps {
  universe: UniverseResponse;
  /** Open a world's room — its Build Map (the tray + ratify live there). */
  onOpenWorld: (root: string) => void;
  /** Open the owner room (the Hall) for owner-scope detail (alerts). */
  onOpenOwner: () => void;
  /** A discreet read-failure note ("read failed — retrying") when a poll blipped but
   *  last-good is on screen — the sky stays lit, the note tells the truth. */
  note?: string | null;
  /** Test seam: freeze "now" so freshness/light is deterministic. */
  nowMs?: number;
}

/** One world disc + its satellites + label. A pure render over a precomputed
 *  placement (radius/light/pending already resolved by `layoutWorlds`). */
function World({ p, nowMs, onOpen }: { p: WorldPlacement; nowMs: number; onOpen: () => void }) {
  const { world, cx, cy, r, light, pending } = p;
  const presences = world.presences ?? [];
  const orbit = r + 12;
  const satN = Math.max(presences.length, 1);
  const age = worldAgeLabel(world.updated_ms, nowMs);

  return (
    <g
      data-role="universe-world"
      data-world-name={world.name}
      data-world-awake={world.awake ? 'true' : 'false'}
      data-world-pending={pending}
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onOpen();
        }
      }}
      className="cursor-pointer group focus:outline-none"
    >
      <title>{`${world.name} — ${world.node_count ?? '?'} nodes · updated ${age}${
        pending > 0 ? ` · ${pending} await your hand` : ''
      }`}</title>

      {/* The pending ring — amber dashed, only when a gesture waits. */}
      {pending > 0 && (
        <circle
          cx={cx}
          cy={cy}
          r={r + 6}
          fill="none"
          stroke="var(--verdict-reverify, #c89b3c)"
          strokeWidth={1.5}
          strokeDasharray="3 4"
          opacity={0.9}
        />
      )}

      {/* The world disc — a paper planet, lit by its freshness. */}
      <circle
        cx={cx}
        cy={cy}
        r={r}
        fill="url(#world-paper)"
        stroke="var(--hairline, #d8d1c6)"
        strokeWidth={1}
        opacity={light}
        className="transition-[r] duration-200 group-hover:brightness-105"
      />
      {/* A hover halo (contact shadow, not glow). */}
      <circle
        cx={cx}
        cy={cy}
        r={r + 3}
        fill="none"
        stroke="var(--ink, #2b2836)"
        strokeWidth={1}
        className="opacity-0 group-hover:opacity-15 group-focus:opacity-25 transition-opacity"
      />

      {/* Satellites — live presences orbiting (verdict-act = awake). */}
      {presences.map((pr, j) => {
        const a = (j / satN) * Math.PI * 2 - Math.PI / 2;
        const sx = cx + orbit * Math.cos(a);
        const sy = cy + orbit * Math.sin(a);
        const mutating = pr.mutation?.observed_at_ms != null || !!pr.mutation?.declared_intent;
        return (
          <g key={pr.agent_id} data-role="universe-satellite">
            <circle cx={sx} cy={sy} r={3} fill="var(--verdict-act, #6fa287)" />
            {mutating && (
              <circle
                cx={sx}
                cy={sy}
                r={5}
                fill="none"
                stroke="var(--verdict-act, #6fa287)"
                strokeWidth={0.75}
                opacity={0.6}
              />
            )}
          </g>
        );
      })}

      {/* Label — mono, below the disc. Name + honest freshness age. */}
      <text
        x={cx}
        y={cy + r + 16}
        textAnchor="middle"
        className="fill-ink font-mono"
        style={{ fontSize: 12 }}
      >
        {world.name}
      </text>
      <text
        x={cx}
        y={cy + r + 30}
        textAnchor="middle"
        className="fill-ink-soft font-mono"
        style={{ fontSize: 9.5 }}
      >
        {age}
      </text>
    </g>
  );
}

export default function UniverseView({
  universe,
  onOpenWorld,
  onOpenOwner,
  note = null,
  nowMs = Date.now(),
}: UniverseViewProps) {
  const placements = useMemo(
    () => layoutWorlds(universe.worlds, { width: VIEW_W, height: VIEW_H, nowMs }),
    [universe.worlds, nowMs],
  );
  const headline = universeHeadline(universe.totals);

  return (
    <div data-role="universe" className="flex flex-1 overflow-hidden bg-porcelain">
      <div className="flex flex-col flex-1 min-w-0">
        {/* L0 header — the client-composed serif sentence of universe FACTS. */}
        <div className="px-6 py-4 border-b border-hairline shrink-0">
          <h1
            data-role="universe-headline"
            className="font-serif text-ink text-xl md:text-2xl tracking-tight"
          >
            {headline}
          </h1>
          <p className="text-xs text-ink-soft mt-1 font-mono">
            the universe · every world this owner holds
            {note && (
              <span data-role="universe-note" className="ml-2 text-verdict-reverify">
                · {note}
              </span>
            )}
          </p>
        </div>

        {/* The panorama. */}
        {universe.worlds.length === 0 ? (
          <div
            data-role="universe-empty"
            className="flex-1 flex items-center justify-center text-center px-8"
          >
            <div className="max-w-sm space-y-2">
              <div className="font-serif text-ink text-lg">An empty sky.</div>
              <p className="text-sm text-ink-soft leading-relaxed">
                No worlds yet — read a repo into its own brain and it lights up here.
              </p>
            </div>
          </div>
        ) : (
          <svg
            data-role="universe-canvas"
            viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
            preserveAspectRatio="xMidYMid meet"
            className="flex-1 w-full h-full"
            role="group"
            aria-label="the universe of worlds"
          >
            <defs>
              {/* A paper planet: warm centre falling to bone at the rim — texture
                  without a single glow. */}
              <radialGradient id="world-paper" cx="38%" cy="34%" r="72%">
                <stop offset="0%" stopColor="var(--warm-paper, #fcfaf6)" />
                <stop offset="70%" stopColor="var(--bone, #efeae2)" />
                <stop offset="100%" stopColor="var(--hairline, #d8d1c6)" />
              </radialGradient>
            </defs>
            {placements.map((p) => (
              <World
                key={p.world.key}
                p={p}
                nowMs={nowMs}
                onOpen={() => onOpenWorld(p.world.root)}
              />
            ))}
          </svg>
        )}
      </div>

      <TheLanding universe={universe} onOpenWorld={onOpenWorld} onOpenOwner={onOpenOwner} />
    </div>
  );
}
