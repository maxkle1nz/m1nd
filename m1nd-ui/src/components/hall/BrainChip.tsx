/*
 * BrainChip — the reception echo, always in view (HUMAN-LAYER-PRD §4A.5).
 *
 * One chip in the top bar on EVERY surface: brain name · node count · liveness,
 * from the same envelope the surface itself rendered. The law: no graph pixel
 * without the owning brain's name in view — the project-d-class "which brain am I
 * talking to?" ambiguity is killed at the chrome level. On degraded binding /
 * reception mismatch the chip wears the honesty (brick text); §3.5's banner still
 * owns the repair. Click = the Hall. SOFT PROOF; no violet (not an abstain surface).
 */
import { Icon } from '../../lib/icons/registry';

interface BrainChipProps {
  /**
   * The bound brain's PROJECT name (§4A.5, Brain Chip law) — "m1nd", never the
   * `agent-memory` sidecar. Comes from the self envelope's server-resolved
   * `display_name`; null while unknown → honest placeholder.
   */
  displayName: string | null;
  /** The bound brain's project root — the chip's hover title (the real repo). */
  projectPath?: string | null;
  nodeCount: number | null;
  /** true = bound & healthy; false = degraded/reception mismatch (wears honesty). */
  healthy: boolean;
  onClick: () => void;
}

export default function BrainChip({
  displayName,
  projectPath,
  nodeCount,
  healthy,
  onClick,
}: BrainChipProps) {
  const name = displayName && displayName.trim().length > 0 ? displayName : 'no brain';
  const dot = healthy ? 'var(--verdict-act, #6fa287)' : 'var(--state-failure, #b0563b)';
  const title = projectPath ? `${projectPath} — open the Hall` : 'open the Hall';
  return (
    <button
      type="button"
      data-role="brain-chip"
      data-healthy={healthy}
      onClick={onClick}
      title={title}
      className={`flex items-center gap-1.5 px-2 py-1 rounded border transition-shadow hover:shadow-contact ${
        healthy ? 'border-ink/12 text-ink' : 'border-state-failure/40 text-state-failure'
      }`}
    >
      <span className="w-1.5 h-1.5 rounded-full inline-block" style={{ backgroundColor: dot }} />
      <span className="text-xs font-medium truncate max-w-[16ch]">{name}</span>
      {nodeCount != null && (
        <span className="inline-flex items-center gap-1 text-[10px] text-ink-soft">
          <Icon name="graph" size={14} decorative className="text-ink-soft/70" />
          <span className="font-mono tabular-nums">{nodeCount.toLocaleString()}</span> nodes
        </span>
      )}
    </button>
  );
}
