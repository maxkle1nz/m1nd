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
  entryBaseUrl,
  canOpenBrainInPlace,
} from '../../lib/hallSemantics';
import { Icon } from '../../lib/icons/registry';
import BrainCardGold from './BrainCardGold';
import type { FreshnessG1, CalibrationG2, CompoundingG3, AlivenessG4 } from '../../lib/cardV2';

export interface BrainCardProps {
  entry: InstanceRegistryEntry;
  isSelf: boolean;
  /**
   * §4A.9.5 capability stamp: the owner advertises the REST brain selector, so a
   * hosted project brain can be Opened in the tree. Absent/false → hosted Open
   * stays disabled (the honest pre-2H posture; INV-11). Defaults to false so a
   * card rendered without the stamp never over-promises.
   */
  restSelector?: boolean;
  /** Real counts when known (self graph_state or a live sibling's polled stats). */
  knownNodeCount?: number | null;
  knownEdgeCount?: number | null;
  selected: boolean;
  /**
   * True for the ONE card the tab is currently viewing (§4A.8). Renders the
   * `Eye` viewing chip — a STATE, not a severity, from the same envelope as the
   * Brain Chip. Exactly one card in the Hall wears it (HallView enforces).
   */
  viewing?: boolean;
  /**
   * Card-v2 GOLD fields (§4A.3.1) — rendered ONLY for the OPEN/bound brain (the
   * TODAY column). A hosted brain passes none and shows them absent-honest.
   */
  gold?: { g1: FreshnessG1 | null; g2: CalibrationG2 | null; g3: CompoundingG3 | null; g4: AlivenessG4 | null } | null;
  onReread?: () => void;
  onCalibrate?: () => void;
  onSelect: (entry: InstanceRegistryEntry) => void;
  onOpen: (entry: InstanceRegistryEntry) => void;
  /**
   * Open THIS brain's mailbox (§4A.11) — the D3 "N open" entry. Absent → the D3
   * field renders nothing (the affordance needs a handler to be honest).
   */
  onOpenMailbox?: (entry: InstanceRegistryEntry) => void;
}

export default function BrainCard({
  entry,
  isSelf,
  restSelector = false,
  knownNodeCount,
  knownEdgeCount,
  selected,
  viewing = false,
  gold = null,
  onReread,
  onCalibrate,
  onSelect,
  onOpen,
  onOpenMailbox,
}: BrainCardProps) {
  const band = livenessBand(entry);
  const dot = LIVENESS_STYLE[band];
  // D3 — the Mailbox entry (§4A.11). "N open" = the box's wet_ink + in_flight
  // (external never counted). Absent-honest: the count is server-enriched ONLY
  // when the repo has a box file, so null → the entry does not render (never a
  // fabricated zero — INV-10). A handler is required for the affordance to exist.
  const openCount = entry.mailbox_open_count;
  const showMailbox = openCount != null && onOpenMailbox != null;
  // A project brain carries its own recorded counts on the entry; self/siblings
  // use the polled known counts. Freshness + conflicts also follow project
  // semantics (no process status, no lock).
  const counts = resolvedBrainCounts(entry, { nodeCount: knownNodeCount, edgeCount: knownEdgeCount });
  const isProject = isProjectBrain(entry);
  const freshnessMs = brainFreshnessMs(entry);
  const conflicts = visibleConflicts(entry);
  const name = brainDisplayName(entry);
  const projectPath = brainProjectPath(entry);
  // §4A.9: a hosted project brain opens IN THE TREE when the owner advertises the
  // REST selector; the bound/self brain always does. A live sibling (own port)
  // opens in a NEW tab. Open is enabled if any of those paths exist.
  const opensInTree = canOpenBrainInPlace(entry, isSelf, restSelector);
  const opensInTab = !opensInTree && entryBaseUrl(entry) != null;
  const canOpen = opensInTree || opensInTab;

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
      {/* Header: dot + name + viewing chip · freshness.
          NO kind badge — implementation class never labels a card face (INV-14);
          it lives in the receipt's `binding:` line. The one distinction a human
          needs here is "which am I looking at?" — the viewing chip (§4A.8). */}
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
            {viewing && (
              <span
                data-role="viewing-chip"
                className="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded-full border border-ink/15 bg-bone text-ink-soft shrink-0"
                title="the brain this tab is viewing"
              >
                <Icon name="viewing" size={14} decorative className="text-ink-soft/80" />
                viewing
              </span>
            )}
          </div>
          <div className="text-[11px] text-ink-soft font-mono break-all" title={projectPath}>
            {shortPath(projectPath)}
          </div>
        </div>
        <div className="flex flex-col items-end gap-1 shrink-0">
          <div className="text-[10px] text-ink-soft font-mono text-right">
            {lastSeenPhrase(freshnessMs)}
          </div>
          {/* D3 — the Mailbox entry (§4A.11). Opens THIS brain's caixinha. */}
          {showMailbox && (
            <button
              type="button"
              data-role="mailbox-entry"
              data-open-count={openCount}
              onClick={(e) => {
                e.stopPropagation();
                onOpenMailbox?.(entry);
              }}
              className="inline-flex items-center gap-1 text-[10px] font-mono px-1.5 py-0.5 rounded-full border border-ink/15 bg-bone text-ink-soft hover:text-ink hover:shadow-contact transition-shadow"
              title="Open this brain's field-report mailbox"
            >
              <Icon name="inbox" size={14} decorative className="text-ink-soft/80" />
              {openCount} open
            </button>
          )}
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

      {/* Card-v2 GOLD (§4A.3.1) — the OPEN/bound brain only (TODAY column) */}
      {gold && <BrainCardGold g1={gold.g1} g2={gold.g2} g3={gold.g3} g4={gold.g4} onReread={onReread} onCalibrate={onCalibrate} />}

      {/* Open — the one action on the face; the rest live in the drawer.
          §4A.9 (2H): the "REST brain routing not built yet" residue is RETIRED —
          a hosted brain now opens in the tree when the owner advertises the
          selector (INV-11 exit criterion). It stays disabled only against a
          pre-2H owner with no stamp (feature-detected, never assumed). */}
      <div className="mt-3 flex items-center gap-2">
        <button
          type="button"
          data-role="open-brain"
          onClick={(e) => {
            e.stopPropagation();
            if (canOpen) onOpen(entry);
          }}
          disabled={!canOpen}
          title={
            opensInTree
              ? isSelf
                ? 'Open this brain (the tree)'
                : 'Open this brain in the tree'
              : opensInTab
                ? 'Open this brain in its own tab'
                : 'This owner can’t open a hosted brain yet — update it to browse hosted brains.'
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
