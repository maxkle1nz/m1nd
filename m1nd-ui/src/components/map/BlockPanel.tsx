/*
 * BlockPanel — the selected block's read-only detail drawer
 * (HUMAN-VIEW-V2-SCREENS §1.1 right panel; PRD F1/F2).
 *
 * Name, purpose, `Receipts M/N` against the block's OWN declared contract (the
 * auditable denominator — never a vibe), each required receipt with its honest
 * per-item state (`— not earned yet`), membership by role, sockets, and the
 * versioned metadata. Show Code / Ask agent are HONEST placeholders: disabled,
 * with a tooltip naming F2 — an affordance without a surface is never a live but
 * dead button (INV-11). Copy law holds: no "proven/done/correct".
 */
import type { BlockRollup, Receipt, SystemBlock } from '../../lib/buildMap';
import { STATE_LABEL, domainTag as toDomainTag, membershipByRole, receiptFreshness } from '../../lib/buildMap';
import { Icon } from '../../lib/icons/registry';

/** The honest per-receipt freshness marker (F3b §C). A boundary-stale receipt names
 *  both versions ("boundary v1 < v2") so the drift is legible; other stale reasons
 *  read plainly; fresh is a calm "fresh". Copy law: never "proven/done/correct". */
function freshnessLabel(receipt: Receipt, block: SystemBlock): { stale: boolean; text: string } {
  const f = receiptFreshness(receipt, block);
  if (f.fresh) return { stale: false, text: 'fresh' };
  if (f.reason === 'boundary') {
    return { stale: true, text: `— stale (boundary v${receipt.scope.boundary_version} < v${block.boundary_version})` };
  }
  return { stale: true, text: `— stale (${f.reason})` };
}

export interface BlockPanelProps {
  block: SystemBlock;
  rollup: BlockRollup;
  repoId: string | null;
  /** Open the Show Code modal for this block (F2). */
  onShowCode?: () => void;
  /** Open the packet compositor for this block (F2). */
  onAskAgent?: () => void;
}

function SocketList({ label, sockets }: { label: string; sockets: Array<{ to?: string; type?: string; alias?: string; class?: string }> }) {
  if (sockets.length === 0) return null;
  return (
    <div className="mt-2">
      <div className="text-[10px] uppercase tracking-wide text-ink-soft">{label}</div>
      <ul className="mt-1 space-y-0.5">
        {sockets.map((s, i) => {
          const target = s.to ?? s.alias ?? s.class ?? '—';
          const kind = s.type ?? s.class ?? '';
          return (
            <li key={i} className="text-[11px] font-mono text-ink">
              <span className="text-socket-blue">→ </span>
              {target}
              {kind ? <span className="text-ink-soft"> · {kind}</span> : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export default function BlockPanel({ block, rollup, repoId, onShowCode, onAskAgent }: BlockPanelProps) {
  const tag = toDomainTag(block.block_id, repoId);
  const roles = membershipByRole(block);

  return (
    <aside
      data-role="block-panel"
      data-block-panel={block.block_id}
      className="w-72 shrink-0 border-l border-ink/10 bg-porcelain overflow-y-auto p-4"
    >
      {/* Header */}
      <div className="text-[11px] font-semibold tracking-wide uppercase text-ink-soft">{tag}</div>
      <h2 className="text-base font-semibold text-ink leading-tight mt-0.5">{block.name}</h2>
      <p className="text-xs text-ink-soft mt-1">{block.purpose}</p>

      <div className="my-3 border-t border-ink/10" />

      {/* Receipts against the block's OWN contract (auditable denominator). */}
      <div className="text-[10px] uppercase tracking-wide text-ink-soft">Receipts</div>
      <div className="text-sm font-mono text-ink mt-0.5" data-role="panel-receipts">
        Receipts {rollup.receiptsEarned}/{rollup.receiptsRequired}
        <span className="text-ink-soft"> · {STATE_LABEL[rollup.state]}</span>
      </div>
      <ul className="mt-1.5 space-y-1">
        {block.receipt_contract.required.map((req) => {
          const earned = rollup.earnedTypes.includes(req.type);
          return (
            <li key={req.type} className="text-[11px] font-mono flex items-center justify-between" data-role="required-receipt">
              <span className="text-ink">{req.type}</span>
              <span className={earned ? 'text-verdict-act' : 'text-ink-soft'}>
                {earned ? 'earned · fresh' : '— not earned yet'}
              </span>
            </li>
          );
        })}
        {block.receipt_contract.required.length === 0 && (
          <li className="text-[11px] font-mono text-ink-soft">no required contract — not scanned yet</li>
        )}
      </ul>
      {block.receipt_contract.optional.length > 0 && (
        <p className="text-[10px] text-ink-soft mt-1.5">
          optional (never counted): {block.receipt_contract.optional.map((o) => o.type).join(' · ')}
        </p>
      )}

      {/* The evidence itself (F3b §C) — the actual receipts, each with its honest
          freshness. When the boundary moved, a note above the list says so, and each
          receipt earned against an older boundary is marked stale with the reason. */}
      {block.receipts.length > 0 && (
        <div className="mt-2.5" data-role="evidence">
          {rollup.boundaryStale && (
            <p className="text-[10px] text-verdict-reverify mb-1" data-role="boundary-stale-note">
              boundary changed — evidence below predates the current membership
            </p>
          )}
          <ul className="space-y-0.5">
            {block.receipts.map((r, i) => {
              const f = freshnessLabel(r, block);
              return (
                <li
                  key={i}
                  data-role="evidence-receipt"
                  data-stale={f.stale}
                  className="text-[11px] font-mono flex items-center justify-between gap-2"
                >
                  <span className="text-ink">{r.type}</span>
                  <span className={f.stale ? 'text-verdict-reverify' : 'text-verdict-act'}>{f.text}</span>
                </li>
              );
            })}
          </ul>
        </div>
      )}

      <div className="my-3 border-t border-ink/10" />

      {/* Membership by role. */}
      <div className="text-[10px] uppercase tracking-wide text-ink-soft">Membership</div>
      <ul className="mt-1 flex flex-wrap gap-1.5" data-role="membership-roles">
        {roles.map((r) => (
          <li key={r.role} className="text-[11px] font-mono bg-bone rounded px-1.5 py-0.5 text-ink">
            {r.role} <span className="text-ink-soft tabular-nums">{r.count}</span>
          </li>
        ))}
      </ul>

      {/* Sockets. */}
      <SocketList label="Inputs" sockets={block.sockets.inputs} />
      <SocketList label="Outputs" sockets={block.sockets.outputs} />
      <SocketList label="External" sockets={block.sockets.external} />

      <div className="my-3 border-t border-ink/10" />

      {/* Versioned metadata (mono, internal surface). */}
      <div className="text-[10px] font-mono text-ink-soft break-all">
        {block.block_id} · boundary v{block.boundary_version} · contract v{block.contract_version}
      </div>

      {/* F2 — Show Code + Ask agent are live (they open read-only surfaces). */}
      <div className="mt-3 space-y-1.5">
        <button
          type="button"
          data-role="show-code"
          onClick={onShowCode}
          disabled={!onShowCode}
          title="Show the block's files, receipts and connections"
          className="w-full flex items-center gap-1.5 px-2.5 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-not-allowed"
        >
          <Icon name="blocks" size={14} decorative />
          Show code
        </button>
        <button
          type="button"
          data-role="ask-agent"
          onClick={onAskAgent}
          disabled={!onAskAgent}
          title="Compose a packet to point an agent at this block"
          className="w-full flex items-center gap-1.5 px-2.5 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-not-allowed"
        >
          <Icon name="agents" size={14} decorative />
          Ask agent
        </button>
      </div>
    </aside>
  );
}
