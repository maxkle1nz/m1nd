/*
 * BrainCard — one brain the owner holds (HUMAN-LAYER-PRD §4A.3).
 *
 * Card anatomy: name + root, a matte liveness dot, node·edges (absent-honest for
 * dormant/hosted brains — INV-10), freshness, kind badge, conflict chips, and the
 * actions (§4A.4). Heat scarcity (§3.2): only a stale/conflicted/failed brain
 * earns a non-sage dot. At most ~five facts on the face; the receipt lives in the
 * drawer. SOFT PROOF only — porcelain/bone/ink, NO violet (the Hall is not an
 * abstain surface; violet-lint enforces it).
 */
import type { InstanceRegistryEntry } from '../../types';
import {
  livenessBand,
  LIVENESS_STYLE,
  lastSeenPhrase,
  brainDisplayName,
  brainProjectPath,
  shortPath,
  resolvedBrainCounts,
  brainFreshnessMs,
  visibleConflicts,
  isProjectBrain,
  brainKindBadge,
  entryBaseUrl,
  type BrainKindBadge,
} from '../../lib/hallSemantics';

const KIND_LABEL: Record<BrainKindBadge, string> = {
  bound: 'this brain',
  project: 'project',
  sibling: 'sibling',
};

export interface BrainCardProps {
  entry: InstanceRegistryEntry;
  isSelf: boolean;
  /** Real counts when known (self graph_state or a live sibling's polled stats). */
  knownNodeCount?: number | null;
  knownEdgeCount?: number | null;
  selected: boolean;
  onSelect: (entry: InstanceRegistryEntry) => void;
  onOpen: (entry: InstanceRegistryEntry) => void;
}

export default function BrainCard({
  entry,
  isSelf,
  knownNodeCount,
  knownEdgeCount,
  selected,
  onSelect,
  onOpen,
}: BrainCardProps) {
  const band = livenessBand(entry);
  const dot = LIVENESS_STYLE[band];
  const kind = brainKindBadge(entry, isSelf);
  // A project brain carries its own recorded counts on the entry; self/siblings
  // use the polled known counts. Freshness + conflicts also follow project
  // semantics (no process status, no lock).
  const counts = resolvedBrainCounts(entry, { nodeCount: knownNodeCount, edgeCount: knownEdgeCount });
  const isProject = isProjectBrain(entry);
  const freshnessMs = brainFreshnessMs(entry);
  const conflicts = visibleConflicts(entry);
  const name = brainDisplayName(entry);
  const projectPath = brainProjectPath(entry);
  const canOpenInPlace = isSelf || entryBaseUrl(entry) != null;

  return (
    <div
      role="listitem"
      tabIndex={0}
      data-brain-card={entry.instance_id}
      data-liveness={band}
      aria-selected={selected}
      onClick={() => onSelect(entry)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          onSelect(entry);
        }
      }}
      className={`rounded-xl border bg-bone/60 p-4 cursor-pointer transition-shadow outline-none focus:ring-1 focus:ring-ink/20 ${
        selected ? 'border-ink/30 shadow-card' : 'border-ink/10 hover:shadow-contact'
      }`}
    >
      {/* Header: dot + name + kind badge · freshness */}
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2 mb-0.5">
            <span
              className="w-2 h-2 rounded-full inline-block shrink-0"
              style={{ backgroundColor: dot.color }}
              title={dot.label}
              aria-label={`liveness: ${dot.label}`}
            />
            <span className="text-sm text-ink font-semibold truncate">{name}</span>
            <span className="text-[10px] font-mono px-1.5 py-0.5 rounded border border-ink/15 text-ink-soft shrink-0">
              {KIND_LABEL[kind]}
            </span>
          </div>
          <div className="text-[11px] text-ink-soft font-mono break-all" title={projectPath}>
            {shortPath(projectPath)}
          </div>
        </div>
        <div className="text-[10px] text-ink-soft font-mono text-right shrink-0">
          {lastSeenPhrase(freshnessMs)}
        </div>
      </div>

      {/* Facts row: counts (absent-honest) — §4A.3, INV-10 */}
      <div className="mt-3 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-ink-soft font-mono">
        {counts.nodeCount != null && counts.edgeCount != null ? (
          <>
            <span data-role="node-count">{counts.nodeCount} nodes</span>
            <span data-role="edge-count">{counts.edgeCount} edges</span>
          </>
        ) : (
          // "not running" is INSTANCE language — never for a project brain, which
          // lives in-process and has no process state. Absent project counts read
          // "counts not recorded yet" (a fresh store before its first persist).
          <span data-role="counts-absent" className="italic text-ink-soft/70">
            {isProject ? 'counts not recorded yet' : 'counts unknown — not running'}
          </span>
        )}
      </div>

      {/* Conflict chips — calm, not warnings (§4A.3); lock/instance conflicts are
          filtered out for a project brain (it owns no lock) */}
      {conflicts.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {conflicts.map((c) => (
            <span
              key={c}
              data-role="conflict-chip"
              className="text-[10px] font-mono px-1.5 py-0.5 rounded border border-ink/15 bg-porcelain text-ink-soft"
            >
              {c.replace(/_/g, ' ')}
            </span>
          ))}
        </div>
      )}

      {/* Open — the one action on the face; the rest live in the drawer */}
      <div className="mt-3 flex items-center gap-2">
        <button
          type="button"
          data-role="open-brain"
          onClick={(e) => {
            e.stopPropagation();
            if (canOpenInPlace) onOpen(entry);
          }}
          disabled={!canOpenInPlace}
          title={
            canOpenInPlace
              ? isSelf
                ? 'Open this brain (the tree)'
                : 'Open this brain in its own tab'
              : 'Opening a hosted project brain in place needs REST brain routing (not built yet)'
          }
          className="px-3 py-1 text-xs bg-porcelain text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-45 disabled:cursor-not-allowed"
        >
          Open
        </button>
        <span className="text-[10px] text-ink-soft/60">more in the receipt →</span>
      </div>
    </div>
  );
}
