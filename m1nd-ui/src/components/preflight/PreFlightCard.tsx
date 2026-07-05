/*
 * PreFlightCard — the north packet rendered for a HUMAN (HUMAN-LAYER §4.2,
 * ORGANISM §C1 reader-2: "one packet, N readers"). The hero moment: "see what the
 * agent verified vs. guessed, before it touches your code."
 *
 * SOFT PROOF: calm, matte, nothing glows; violet is quarantined to the abstain
 * verdict + the honest-gaps card; icons come from the registry only. It renders a
 * REAL north packet (from `north`, or a captured fixture) — never invents (INV-01).
 * A field with no packet field behind it does not render (§C1: renders, never
 * widens). Read-only.
 *
 * The beats, top to bottom (§4.2):
 *   1. BINDING            — which brain · trust mode · freshness (+ reception rider)
 *   2. the VERDICT        — act / reverify / abstain (abstain wears iris violet)
 *   3. ANCHORS            — the task's focus regions, a strip (not a graph)
 *   4. WHAT AGENTS KNOW   — the memory beat: claims + author + age + tier (R7)
 *   5. HONEST GAPS        — what m1nd does NOT know, first-class, violet, still
 *   + the primary next-move button (the one suggested step).
 */
import { Icon } from '../../lib/icons/registry';
import {
  bindingView,
  receptionView,
  anchorChips,
  memoryRows,
  emptyMemoryLine,
  honestGaps,
  preflightVerdict,
  nextMove,
  seedBand,
} from '../../lib/preflight';
import type { NorthPacket } from '../../api/toolTypes';
import type { ImpactOutput } from '../../api/toolTypes';
import type { TrustBand } from '../../lib/softProof';
import { blastCountPhrase } from '../../lib/softProof';
import TrustDot from '../soft/TrustDot';
import VerdictChip from '../soft/VerdictChip';
import PostItChip from '../soft/PostItChip';
import GapCard from '../soft/GapCard';
import FreshnessBanner from '../soft/FreshnessBanner';
import { postItState } from '../../lib/softProof';

export interface PreFlightCardProps {
  /** The REAL north packet for the task (from `north` or a captured fixture). */
  packet: NorthPacket;
  /** A short label for what's about to be touched (the seeded focus node name). */
  target?: string | null;
  /** The seeded focus node's trust band (from the tree), for the header dot. */
  targetBand?: TrustBand | null;
  /** The blast line, from a separate `impact` call on the focus node (floor
   *  language). Absent → the blast beat simply does not render (INV-01). */
  impact?: ImpactOutput | null;
  /** Fire the packet's next_move (the primary button). */
  onNextMove?: (move: string) => void;
  /** Close the card (ESC ascends). */
  onClose?: () => void;
}

/** A section shell — a quiet label above its beat. Matte, no glow. */
function Beat({
  label,
  icon,
  children,
}: {
  label: string;
  icon: React.ComponentProps<typeof Icon>['name'];
  children: React.ReactNode;
}) {
  return (
    <section className="px-5 py-3 border-b border-ink/10">
      <div className="flex items-center gap-1.5 mb-2 text-[10px] uppercase tracking-wide text-ink-soft">
        <Icon name={icon} size={14} decorative className="shrink-0" />
        <span>{label}</span>
      </div>
      {children}
    </section>
  );
}

export default function PreFlightCard({
  packet,
  target,
  targetBand,
  impact,
  onNextMove,
  onClose,
}: PreFlightCardProps) {
  const binding = bindingView(packet);
  const reception = receptionView(packet);
  const anchors = anchorChips(packet);
  const memory = memoryRows(packet);
  const gaps = honestGaps(packet);
  const verdict = preflightVerdict(packet);
  const move = nextMove(packet);
  const band = seedBand(targetBand);

  return (
    <div
      data-role="preflight-card"
      role="dialog"
      aria-label="Pre-flight check"
      className="w-full max-w-lg bg-porcelain border border-ink/15 rounded-lg shadow-card overflow-hidden flex flex-col"
    >
      {/* Headline — "Before we touch <target>" + the verdict glance (§4.2 #1) */}
      <header className="px-5 py-4 border-b border-ink/10 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-[11px] uppercase tracking-wide text-ink-soft mb-1">
            Before we touch
          </div>
          <div className="flex items-center gap-2 min-w-0">
            <TrustDot band={band} title={`trust: ${band}`} />
            <span
              data-role="preflight-target"
              className="text-ink font-mono font-semibold text-sm break-all leading-snug"
            >
              {target ?? packet.task ?? 'this code'}
            </span>
          </div>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="text-ink-soft hover:text-ink text-sm shrink-0"
          >
            ✕
          </button>
        )}
      </header>

      <div className="flex-1 overflow-y-auto">
        {/* 1. BINDING — which brain, trust mode, freshness (§4.2 header) */}
        <Beat label="binding" icon="viewing">
          <div className="space-y-1">
            <div className="text-[13px] text-ink" data-role="binding-line">
              {binding.trustLine}
            </div>
            <div className="flex items-center gap-3 text-[11px] text-ink-soft font-mono tabular-nums">
              {binding.nodeCount != null && (
                <span data-role="binding-nodes">
                  {binding.nodeCount.toLocaleString()} nodes
                </span>
              )}
              {binding.edgeCount != null && (
                <span data-role="binding-edges">
                  {binding.edgeCount.toLocaleString()} connections
                </span>
              )}
              {binding.binaryVersion && <span aria-hidden>·</span>}
              {binding.binaryVersion && <span>v{binding.binaryVersion}</span>}
            </div>
            {/* The reception rider (§C1.4 / JOINT-I): the bound graph may not cover
                the caller's repo — surfaced honestly through the abstain-class
                FreshnessBanner (violet, on the §6.2 allow-list), verbatim, never
                hidden behind a confident header. */}
            {reception.mismatch && reception.honest && (
              <div data-role="reception-rider" className="mt-1">
                <FreshnessBanner
                  message={`${reception.honest} — I'll verify against your files.`}
                  tone="unknown"
                />
              </div>
            )}
          </div>
        </Beat>

        {/* 2. THE VERDICT — act / reverify / abstain (§4.2 the calm decision) */}
        <Beat label="the read" icon="calibration">
          <div className="flex items-center gap-2">
            <VerdictChip verdict={verdict} />
            {/* The blast line rides here when impact is known — floor language
                (INV-08): "≥ N mapped files" over a truncated/incomplete subgraph. */}
            {impact && (
              <span className="text-[12px] text-ink-soft" data-role="blast-line">
                touches{' '}
                <span className="font-mono tabular-nums text-ink">
                  {blastCountPhrase(impact.total_blast_nodes, impact.truncated)}
                </span>
              </span>
            )}
          </div>
        </Beat>

        {/* 3. ANCHORS — the task's focus regions, a strip not a graph (§4.2 #2) */}
        {anchors.length > 0 && (
          <Beat label="the neighborhood" icon="graph">
            <div className="flex flex-wrap gap-1.5" data-role="anchor-strip">
              {anchors.map((a) => (
                <span
                  key={a.nodeId}
                  data-role="anchor-chip"
                  title={a.nodeId}
                  className="inline-flex items-center px-2 py-0.5 rounded-full bg-bone border border-ink/15 text-[11px] font-mono text-ink max-w-[13rem] truncate"
                >
                  {a.label}
                </span>
              ))}
            </div>
          </Beat>
        )}

        {/* 4. WHAT AGENTS KNOW — the memory beat (§4.2 #4, R7 tier+origin rows) */}
        <Beat label="what agents proved here" icon="memory">
          {memory.length === 0 ? (
            <div className="text-[12px] text-ink-soft/80 italic" data-role="memory-empty">
              {emptyMemoryLine(packet)}
            </div>
          ) : (
            <div className="space-y-2" data-role="memory-strip">
              {memory.map((m) => (
                <div key={m.nodeId} className="space-y-0.5">
                  <PostItChip
                    variant="card"
                    claim={m.claim}
                    sourceAgent={m.sourceAgent}
                    ageMs={m.ageMs}
                    state={postItState({
                      ageMs: m.ageMs,
                      sourceAgent: m.sourceAgent,
                      evidenceStale: m.stale,
                    })}
                  />
                  {/* R7 provenance: tier + origin brain, absent-honest (INV-04). */}
                  {(m.tier || m.originBrain) && (
                    <div
                      className="flex items-center gap-2 pl-3 font-mono text-[10px] text-ink-soft"
                      data-role="memory-provenance"
                    >
                      {m.tier && <span data-role="memory-tier">{m.tier}</span>}
                      {m.tier && m.originBrain && <span aria-hidden>·</span>}
                      {m.originBrain && (
                        <span data-role="memory-origin">from {m.originBrain}</span>
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </Beat>

        {/* 5. HONEST GAPS — what m1nd does NOT know, first-class (§4.2 #5) */}
        {gaps.length > 0 && (
          <Beat label="what I don't know yet" icon="freshness">
            <div className="space-y-2" data-role="honest-gaps">
              {gaps.map((g, i) => (
                <GapCard key={i} gap={g} onAction={(a) => onNextMove?.(a)} />
              ))}
            </div>
          </Beat>
        )}
      </div>

      {/* The primary next-move button — the one suggested step (§4.2 headline). */}
      {move && (
        <footer className="px-5 py-3 border-t border-ink/10 bg-bone/40">
          <button
            type="button"
            data-role="next-move"
            onClick={() => onNextMove?.(move)}
            title={move}
            className="w-full text-left flex items-center gap-2 px-3 py-2 rounded bg-bone text-ink border border-ink/20 hover:shadow-contact transition-shadow text-[13px]"
          >
            <Icon name="ingest" size={16} decorative className="shrink-0 text-ink-soft" />
            <span className="truncate">{move}</span>
          </button>
        </footer>
      )}
    </div>
  );
}
