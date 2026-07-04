/*
 * BrainReceiptDrawer — the per-brain receipt (HUMAN-LAYER-PRD §4A.3, R6).
 *
 * A read-only receipt, NOT a dashboard: binding fingerprint (runtime/workspace
 * roots, pid), conflicts, persist age, and the actions ladder (§4A.4) with honest
 * disabled states. The calm two-step delete (ForgetRuntimeFlow) is hosted here as
 * an inline reveal — never a slam modal. SOFT PROOF only; the only violet on the
 * whole surface is nowhere (the Hall is not abstain).
 */
import type { InstanceRegistryEntry, InstanceSelfResponse } from '../../types';
import {
  livenessBand,
  LIVENESS_STYLE,
  repoBasename,
  shortPath,
  bornPhrase,
  lastSeenPhrase,
  persistedPhrase,
  entryBaseUrl,
  brainCounts,
} from '../../lib/hallSemantics';
import ForgetRuntimeFlow from './ForgetRuntimeFlow';

interface BrainReceiptDrawerProps {
  entry: InstanceRegistryEntry | null;
  isSelf: boolean;
  self: InstanceSelfResponse | null;
  onClose: () => void;
  onOpen: (entry: InstanceRegistryEntry) => void;
  onSave: (entry: InstanceRegistryEntry) => void;
  saving: boolean;
  /** Called after a successful delete so the list can drop the card. */
  onDeleted: (entry: InstanceRegistryEntry) => void;
}

/** A read-only receipt line. */
function Fact({ label, value, mono = true }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-1 border-b border-ink/5 last:border-0">
      <span className="text-[10px] uppercase tracking-wide text-ink-soft/70 shrink-0">{label}</span>
      <span className={`text-[11px] text-ink text-right break-all ${mono ? 'font-mono' : ''}`}>{value}</span>
    </div>
  );
}

/** A disabled-with-tooltip rung (§4A.4 needs-backend items — INV-11). */
function DisabledRung({ label, residue, cli }: { label: string; residue: string; cli?: string }) {
  return (
    <div className="flex items-center justify-between gap-2" data-role="disabled-rung">
      <button
        type="button"
        disabled
        title={residue}
        className="px-3 py-1 text-xs bg-porcelain text-ink border border-ink/12 rounded opacity-45 cursor-not-allowed"
      >
        {label}
      </button>
      <span className="text-[10px] text-ink-soft/70 font-mono truncate" title={residue}>
        {cli ? <code className="select-all">{cli}</code> : residue}
      </span>
    </div>
  );
}

export default function BrainReceiptDrawer({
  entry,
  isSelf,
  self,
  onClose,
  onOpen,
  onSave,
  saving,
  onDeleted,
}: BrainReceiptDrawerProps) {
  if (!entry) return null;
  const band = livenessBand(entry);
  const dot = LIVENESS_STYLE[band];
  const name = repoBasename(entry.workspace_root);
  const baseUrl = entryBaseUrl(entry);
  const canOpenInPlace = isSelf || baseUrl != null;
  const isLive = entry.owner_live === true;

  // Counts are known only for self (graph_state) here; a live sibling would need a
  // polled /api/graph/stats — deferred, so dormant/hosted shows absent-honest.
  const counts = brainCounts(
    isSelf && self
      ? { nodeCount: self.graph_state.node_count, edgeCount: self.graph_state.edge_count }
      : { nodeCount: null, edgeCount: null },
  );

  return (
    <aside
      role="complementary"
      aria-label={`receipt for ${name}`}
      className="w-[380px] max-w-full h-full border-l border-ink/10 bg-porcelain flex flex-col shrink-0"
    >
      {/* Header */}
      <div className="px-5 py-4 border-b border-ink/10 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <span className="w-2 h-2 rounded-full inline-block" style={{ backgroundColor: dot.color }} title={dot.label} />
            <span className="text-base text-ink font-semibold truncate">{name}</span>
          </div>
          <div className="text-[11px] text-ink-soft font-mono break-all" title={entry.workspace_root}>
            {entry.workspace_root}
          </div>
        </div>
        <button onClick={onClose} className="text-ink-soft hover:text-ink text-sm font-mono shrink-0" aria-label="close receipt">
          esc
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5">
        {/* The receipt — read-only facts */}
        <section>
          <div className="text-[10px] uppercase tracking-widest text-ink-soft/70 mb-2">the binding</div>
          <div className="rounded-lg border border-ink/10 bg-bone/50 px-3 py-2">
            <Fact label="status" value={dot.label} mono={false} />
            {counts.nodeCount != null && counts.edgeCount != null ? (
              <Fact label="graph" value={`${counts.nodeCount} nodes · ${counts.edgeCount} edges`} />
            ) : (
              <Fact label="graph" value="counts unknown — not running" mono={false} />
            )}
            <Fact label="runtime root" value={shortPath(entry.runtime_root)} />
            <Fact label="workspace" value={shortPath(entry.workspace_root)} />
            <Fact label="pid" value={String(entry.pid)} />
            {bornPhrase(entry.started_at_ms) && <Fact label="born" value={bornPhrase(entry.started_at_ms)!} mono={false} />}
            <Fact label="last seen" value={lastSeenPhrase(entry.last_heartbeat_ms)} mono={false} />
            {isSelf && self && persistedPhrase(self.last_persist_secs_ago) && (
              <Fact label="persisted" value={persistedPhrase(self.last_persist_secs_ago)!} mono={false} />
            )}
            {isSelf && self ? (
              <>
                <Fact label="attached" value={`${self.active_agent_sessions} agent sessions`} mono={false} />
                <Fact label="activity" value={`${self.queries_processed} queries this session`} mono={false} />
              </>
            ) : (
              // §4A.3 / §9.5.1: per-listed-brain calibration + attached_sessions are
              // [needs-backend] — the owner knows, no surface reports them. Named
              // absent-honest, never zero, never guessed (INV-10/INV-11).
              <div
                data-role="needs-backend-fields"
                className="pt-1 text-[10px] text-ink-soft/70 italic"
                title="Per-brain calibration and attached-session counts need the §9.5.1 registry fields (not built yet)"
              >
                attached agents &amp; calibration — not reported for this brain yet
              </div>
            )}
          </div>
          {entry.conflicts.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1.5">
              {entry.conflicts.map((c) => (
                <span key={c} className="text-[10px] font-mono px-1.5 py-0.5 rounded border border-ink/15 bg-porcelain text-ink-soft">
                  {c.replace(/_/g, ' ')}
                </span>
              ))}
            </div>
          )}
        </section>

        {/* Actions (§4A.4) */}
        <section>
          <div className="text-[10px] uppercase tracking-widest text-ink-soft/70 mb-2">actions</div>
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <button
                type="button"
                data-role="open-brain"
                onClick={() => canOpenInPlace && onOpen(entry)}
                disabled={!canOpenInPlace}
                title={
                  canOpenInPlace
                    ? isSelf
                      ? 'Open this brain (the tree)'
                      : 'Open this brain in its own tab'
                    : 'Opening a hosted project brain in place needs REST brain routing (not built yet)'
                }
                className="px-3 py-1 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-45 disabled:cursor-not-allowed"
              >
                Open
              </button>
              {isSelf && (
                <button
                  type="button"
                  data-role="save-brain"
                  onClick={() => onSave(entry)}
                  disabled={saving}
                  title="Persist this brain's state now"
                  className="px-3 py-1 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-50"
                >
                  {saving ? 'Saving…' : 'Save state'}
                </button>
              )}
            </div>

            {/* The removal ladder (§4A.4, R5): stop → clean → eject. Only clean is live. */}
            <DisabledRung
              label="Stop"
              residue="Stopping a live brain from the UI isn't built yet — two-tier Slice 2."
              cli={`m1nd brain stop ${shortPath(entry.workspace_root)}`}
            />

            {/* Clean = the real guarded delete (the calm two-step). Live brains are refused. */}
            <ForgetRuntimeFlow entry={entry} isLive={isLive} onDeleted={() => onDeleted(entry)} />

            <DisabledRung
              label="Eject"
              residue="Eject (delete committed memory) is reserved for V2 — git still carries your mind."
            />
          </div>
        </section>
      </div>
    </aside>
  );
}
