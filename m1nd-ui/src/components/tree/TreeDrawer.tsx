/*
 * TreeDrawer — the node drawer, rung 1 (PRD §3.4). An evolution of DetailPanel.
 * Shows: path + symbols, the trust chip IN ACTION LANGUAGE, the anchored post-its
 * (as read-only cards), and the blast whisper (impact, floor language INV-08).
 * Read-only: no editing, no authoring. ESC ascends (handled by the container).
 */
import { useEffect, useState } from 'react';
import type { TreeRow } from '../../lib/tree';
import type { TrustBand } from '../../lib/softProof';
import { blastCountPhrase } from '../../lib/softProof';
import { api } from '../../api/client';
import type { ImpactOutput } from '../../api/toolTypes';
import PostItChip from '../soft/PostItChip';
import VerdictChip from '../soft/VerdictChip';

interface TreeDrawerProps {
  row: TreeRow | null;
  band: TrustBand;
  onClose: () => void;
  /** Open the Pre-Flight Card seeded with this node (§3.4, the hero entry). */
  onCheckBeforeEditing?: (row: TreeRow) => void;
}

/**
 * Map a trust band to the drawer verdict chip (action language, no epistemology).
 *
 * The band IS the learn-history verdict: `insufficient_evidence` means no agent
 * has ever confirmed or corrected an answer about this file; low/medium/high mean
 * feedback exists (agents confirmed it safe, flagged it mixed, or corrected it as
 * risky). See `feedbackLine` for the human wording of that history.
 */
function bandVerdict(band: TrustBand): { verdict: 'act' | 'reverify' | 'abstain'; label: string } {
  switch (band) {
    case 'low':
      return { verdict: 'act', label: 'looks safe to touch' };
    case 'medium':
      return { verdict: 'reverify', label: 'worth a second look' };
    case 'high':
      return { verdict: 'reverify', label: 'risky — check before editing' };
    case 'insufficient_evidence':
    default:
      // The old copy said "I haven't seen evidence either way yet" — but `evidence`
      // is L1GHT's word for MEMORY anchors (the panel below), and the collision
      // read as "no memories" and confused a live reader. This chip is about
      // FEEDBACK — whether agents confirmed/corrected answers about this file —
      // never about memory. It must never contain the word "evidence" (INV: the
      // memory/feedback glossary, PRD §4A.3.1).
      return { verdict: 'abstain', label: 'no feedback on this file yet' };
  }
}

/**
 * The learn-HISTORY line under the feedback chip: the human wording of whether
 * agents have confirmed or corrected answers about this file (PRD §4A.3.1
 * glossary — FEEDBACK, distinct from the MEMORY anchors panel below). The band is
 * the history verdict; this turns it into one honest sentence. Never says
 * "evidence" (that word belongs to memory).
 */
export function feedbackLine(band: TrustBand): string {
  switch (band) {
    case 'low':
      return 'agents have confirmed answers about this file';
    case 'high':
      return 'agents have corrected answers about this file';
    case 'medium':
      return 'agents have both confirmed and corrected answers here';
    case 'insufficient_evidence':
    default:
      return 'no feedback yet — no agent has confirmed or corrected answers about this file';
  }
}

export default function TreeDrawer({ row, band, onClose, onCheckBeforeEditing }: TreeDrawerProps) {
  const [impact, setImpact] = useState<ImpactOutput | null>(null);
  const [impactLoading, setImpactLoading] = useState(false);

  useEffect(() => {
    setImpact(null);
    if (!row || !row.externalId) return;
    let mounted = true;
    setImpactLoading(true);
    api
      .tool<ImpactOutput>('impact', { node_id: row.externalId })
      .then((r) => {
        if (mounted) setImpact(r);
      })
      .catch(() => {})
      .finally(() => {
        if (mounted) setImpactLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, [row?.externalId]);

  if (!row) {
    return (
      <aside className="w-80 border-l border-ink/10 bg-porcelain flex flex-col items-center justify-center text-ink-soft/70 text-xs shrink-0">
        <div className="text-center space-y-1">
          <div className="text-lg">◇</div>
          <div>Select a row</div>
        </div>
      </aside>
    );
  }

  const v = bandVerdict(band);

  return (
    <aside data-role="tree-drawer" className="w-80 border-l border-ink/10 bg-porcelain flex flex-col shrink-0 overflow-hidden">
      {/* Header */}
      <div className="px-4 py-3 border-b border-ink/10">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-0.5">{row.kind}</div>
            <div className="text-ink text-sm font-mono font-semibold break-all leading-snug">
              {row.name}
            </div>
            <div className="text-[11px] text-ink-soft mt-1 break-all">{row.path}</div>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="text-ink-soft hover:text-ink text-sm shrink-0"
          >
            ✕
          </button>
        </div>
        {/* The FEEDBACK chip (PRD §4A.3.1): whether agents confirmed/corrected
            answers about this file. Distinct from the MEMORY anchors below —
            this line never says "evidence" (that word is memory's). */}
        <div className="mt-2 space-y-1">
          <div className="flex items-center gap-2">
            <VerdictChip verdict={v.verdict} label={v.label} />
          </div>
          <div className="text-[10px] text-ink-soft" data-role="feedback-line">
            {feedbackLine(band)}
          </div>
        </div>
        {/* The hero entry (§3.4): open the Pre-Flight Card seeded with this node —
            "see what the agent verified vs. guessed before it touches your code." */}
        {onCheckBeforeEditing && (
          <button
            type="button"
            data-role="check-before-editing"
            onClick={() => onCheckBeforeEditing(row)}
            className="mt-2 w-full text-left px-3 py-1.5 rounded bg-bone text-ink border border-ink/20 hover:shadow-contact transition-shadow text-[12px]"
          >
            Check before editing
          </button>
        )}
      </div>

      {/* Blast whisper (floor language) */}
      <div className="px-4 py-2.5 border-b border-ink/10">
        <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-1">blast radius</div>
        {impactLoading && <div className="text-[11px] text-ink-soft/70">measuring…</div>}
        {impact && (
          <div className="text-[13px] text-ink" data-role="blast-line">
            touches{' '}
            <span className="font-mono font-medium">
              {blastCountPhrase(impact.total_blast_nodes, impact.truncated)}
            </span>
          </div>
        )}
        {!impactLoading && !impact && row.externalId && (
          <div className="text-[11px] text-ink-soft/70">no blast data</div>
        )}
      </div>

      {/* Anchored post-its */}
      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-2">
        <div className="text-[10px] uppercase tracking-wide text-ink-soft">
          memories anchored here ({row.postIts.length})
        </div>
        {row.postIts.length === 0 && (
          <div className="text-[11px] text-ink-soft/70 italic">
            No memories cite this yet — agents leave notes here as they work.
          </div>
        )}
        {row.postIts.map((p) => (
          <PostItChip
            key={p.nodeId}
            variant="card"
            claim={p.claim}
            sourceAgent={p.sourceAgent}
            ageMs={p.ageMs}
            state={p.state}
          />
        ))}

        {row.children.length > 0 && (
          <div className="pt-2">
            <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-1">
              contains ({row.children.length})
            </div>
            <div className="space-y-0.5">
              {row.children.slice(0, 40).map((c) => (
                <div key={c.id} className="text-[11px] text-ink-soft font-mono truncate">
                  {c.name}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </aside>
  );
}
