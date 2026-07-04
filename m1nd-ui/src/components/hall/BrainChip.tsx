/*
 * BrainChip — the reception echo, always in view (HUMAN-LAYER-PRD §4A.5).
 *
 * One chip in the top bar on EVERY surface: brain name · node count · liveness,
 * from the same envelope the surface itself rendered. The law: no graph pixel
 * without the owning brain's name in view — the almus-class "which brain am I
 * talking to?" ambiguity is killed at the chrome level. On degraded binding /
 * reception mismatch the chip wears the honesty (brick text); §3.5's banner still
 * owns the repair. Click = the Hall. SOFT PROOF; no violet (not an abstain surface).
 */
import { repoBasename } from '../../lib/hallSemantics';

interface BrainChipProps {
  /** Brain name (repo basename); null while unknown → honest placeholder. */
  workspaceRoot: string | null;
  nodeCount: number | null;
  /** true = bound & healthy; false = degraded/reception mismatch (wears honesty). */
  healthy: boolean;
  onClick: () => void;
}

export default function BrainChip({ workspaceRoot, nodeCount, healthy, onClick }: BrainChipProps) {
  const name = workspaceRoot ? repoBasename(workspaceRoot) : 'no brain';
  const dot = healthy ? 'var(--verdict-act, #6fa287)' : 'var(--state-failure, #b0563b)';
  return (
    <button
      type="button"
      data-role="brain-chip"
      data-healthy={healthy}
      onClick={onClick}
      title={workspaceRoot ? `${workspaceRoot} — open the Hall` : 'open the Hall'}
      className={`flex items-center gap-1.5 px-2 py-1 rounded border transition-shadow hover:shadow-contact ${
        healthy ? 'border-ink/12 text-ink' : 'border-state-failure/40 text-state-failure'
      }`}
    >
      <span className="w-1.5 h-1.5 rounded-full inline-block" style={{ backgroundColor: dot }} />
      <span className="text-xs font-medium truncate max-w-[16ch]">{name}</span>
      {nodeCount != null && (
        <span className="text-[10px] font-mono text-ink-soft">{nodeCount} nodes</span>
      )}
    </button>
  );
}
