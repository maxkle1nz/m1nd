/*
 * ShowCode — the per-block Show Code modal (HUMAN-VIEW-V2-SCREENS §4, PRD F8–F10).
 *
 * Four tabs over a block's ratified truth: Files (membership grouped by role, with
 * a read-only viewer), Tests (the role=test subset), Receipts (the block's own
 * receipt contract with each required type's earned/not-earned state — the
 * auditable denominator), and Impact (the block's DECLARED sockets — structural
 * neighbours, never an invented blast radius). The header opens the packet
 * compositor (Copy context / Ask agent). Everything here is READ-ONLY. Copy law
 * holds: no "proven/done/correct".
 */
import { useState } from 'react';
import type { BlockRollup, MembershipRole, Socket, SystemBlock } from '../../lib/buildMap';
import { STATE_LABEL, domainTag as toDomainTag, membershipByRole } from '../../lib/buildMap';
import CodeReader from './CodeReader';
import { Icon } from '../../lib/icons/registry';

export type ShowCodeTab = 'files' | 'tests' | 'receipts' | 'impact';

export interface ShowCodeProps {
  block: SystemBlock;
  rollup: BlockRollup;
  repoId: string | null;
  /** §4A.9 — the brain whose repo the viewer reads (null = bound). */
  brainRoot?: string | null;
  onClose: () => void;
  /** Open the packet compositor scoped to this block (Copy context / Ask agent). */
  onAskAgent: (subPath?: string | null) => void;
  /** For tests/SSR: the tab to render, and a pre-selected viewer path. */
  initialTab?: ShowCodeTab;
  initialSelectedPath?: string | null;
}

const ROLE_LABEL: Record<MembershipRole, string> = {
  primary: 'primary',
  shared: 'shared',
  generated: 'generated',
  test: 'tests',
  docs: 'docs',
  external_socket: 'external sockets',
};

const TABS: Array<{ id: ShowCodeTab; label: string }> = [
  { id: 'files', label: 'Files' },
  { id: 'tests', label: 'Tests' },
  { id: 'receipts', label: 'Receipts' },
  { id: 'impact', label: 'Impact' },
];

function socketTarget(s: Socket): string {
  return s.to ?? s.alias ?? s.class ?? '—';
}

/** The right-rail health panel (F9) — real counts only, honest denominators. */
function HealthPanel({ block, rollup }: { block: SystemBlock; rollup: BlockRollup }) {
  const testCount = block.membership.filter((m) => m.role === 'test').length;
  return (
    <aside className="w-52 shrink-0 border-l border-ink/10 p-3 overflow-y-auto" data-role="health-panel">
      <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-2">Health</div>
      <div className="space-y-1.5 text-[11px] font-mono text-ink">
        <div className="flex justify-between">
          <span className="text-ink-soft">Files</span>
          <span className="tabular-nums">{block.membership.length}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-ink-soft">Tests</span>
          <span className="tabular-nums">{testCount}</span>
        </div>
        <div className="flex justify-between" data-role="health-receipts">
          <span className="text-ink-soft">Receipts</span>
          <span className="tabular-nums">
            {rollup.receiptsEarned}/{rollup.receiptsRequired}
          </span>
        </div>
        <div className="flex justify-between">
          <span className="text-ink-soft">Runtime</span>
          <span className="text-ink-soft">no signal</span>
        </div>
      </div>
      <div className="mt-2 pt-2 border-t border-ink/10 text-[11px]">
        <span className="text-ink-soft">state · </span>
        <span className="text-ink">{STATE_LABEL[rollup.state]}</span>
      </div>
      <p className="mt-2 text-[10px] text-ink-soft">
        type · lint · build · security are not scanned yet — no receipt.
      </p>
    </aside>
  );
}

export default function ShowCode({
  block,
  rollup,
  repoId,
  brainRoot = null,
  onClose,
  onAskAgent,
  initialTab = 'files',
  initialSelectedPath = null,
}: ShowCodeProps) {
  const [tab, setTab] = useState<ShowCodeTab>(initialTab);
  const [selectedPath, setSelectedPath] = useState<string | null>(initialSelectedPath);
  const tag = toDomainTag(block.block_id, repoId);
  const roles = membershipByRole(block);
  const testMembers = block.membership.filter((m) => m.role === 'test');
  const socketCount =
    block.sockets.inputs.length + block.sockets.outputs.length + block.sockets.external.length;

  return (
    <>
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} aria-hidden />
      <div
        className="fixed top-[6%] left-1/2 -translate-x-1/2 z-50 w-full max-w-5xl mx-4 h-[85vh] flex flex-col rounded-lg border border-hairline bg-warm-paper shadow-card"
        data-role="show-code"
        data-block-showcode={block.block_id}
      >
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-ink/10">
          <Icon name="blocks" size={14} decorative />
          <span className="text-[11px] font-semibold tracking-wide uppercase text-ink-soft">{tag}</span>
          <span className="text-sm font-semibold text-ink">{block.name}</span>
          <div className="ml-auto flex items-center gap-1.5">
            <button
              type="button"
              data-role="open-in-editor"
              disabled
              title="needs a local editor bridge — a later slice"
              className="px-2.5 py-1 text-xs bg-bone text-ink-soft border border-ink/15 rounded opacity-60 cursor-not-allowed"
            >
              Open in editor
            </button>
            <button
              type="button"
              data-role="copy-context"
              onClick={() => onAskAgent(selectedPath)}
              className="px-2.5 py-1 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
            >
              Copy context
            </button>
            <button
              type="button"
              data-role="showcode-ask-agent"
              onClick={() => onAskAgent(selectedPath)}
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
            >
              <Icon name="agents" size={14} decorative />
              Ask agent
            </button>
            <button
              type="button"
              data-role="showcode-close"
              onClick={onClose}
              aria-label="Close"
              className="text-ink-soft hover:text-ink text-sm px-1.5"
            >
              ✕
            </button>
          </div>
        </div>

        {/* Tabs */}
        <div className="flex items-center gap-1 px-3 pt-2 border-b border-ink/10" role="tablist">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              data-role={`tab-${t.id}`}
              data-active={tab === t.id}
              onClick={() => setTab(t.id)}
              className={`px-3 py-1.5 text-xs rounded-t border-b-2 -mb-px transition-colors ${
                tab === t.id
                  ? 'border-socket-blue text-ink font-semibold'
                  : 'border-transparent text-ink-soft hover:text-ink'
              }`}
            >
              {t.label}
              {t.id === 'files' && <span className="ml-1 text-ink-soft tabular-nums">{block.membership.length}</span>}
              {t.id === 'tests' && <span className="ml-1 text-ink-soft tabular-nums">{testMembers.length}</span>}
              {t.id === 'impact' && <span className="ml-1 text-ink-soft tabular-nums">{socketCount}</span>}
            </button>
          ))}
        </div>

        {/* Tab body */}
        <div className="flex-1 min-h-0 flex overflow-hidden">
          {tab === 'files' && (
            <>
              <div className="w-64 shrink-0 border-r border-ink/10 overflow-y-auto p-3" data-role="files-by-role">
                <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-1">
                  Files by role ({block.membership.length})
                </div>
                {roles.map((r) => (
                  <div key={r.role} className="mt-2" data-role={`role-group-${r.role}`}>
                    <div className="text-[11px] font-semibold text-ink flex items-center gap-1.5">
                      {ROLE_LABEL[r.role]}
                      <span className="text-ink-soft tabular-nums">{r.count}</span>
                    </div>
                    <ul className="mt-0.5">
                      {block.membership
                        .filter((m) => m.role === r.role)
                        .map((m) => (
                          <li key={m.path}>
                            <button
                              type="button"
                              data-role="file-path"
                              data-path={m.path}
                              onClick={() => setSelectedPath(m.path)}
                              className={`w-full text-left text-[11px] font-mono px-1 py-0.5 rounded truncate ${
                                selectedPath === m.path ? 'bg-bone text-ink' : 'text-ink-soft hover:text-ink hover:bg-bone/60'
                              }`}
                              title={m.path}
                            >
                              {m.path}
                            </button>
                          </li>
                        ))}
                    </ul>
                  </div>
                ))}
              </div>
              <CodeReader path={selectedPath} brainRoot={brainRoot} rollup={rollup} />
              <HealthPanel block={block} rollup={rollup} />
            </>
          )}

          {tab === 'tests' && (
            <div className="flex-1 overflow-y-auto p-4" data-role="tests-tab">
              {testMembers.length === 0 ? (
                <div className="text-xs text-ink-soft" data-role="tests-empty">
                  No files are tagged as tests in this block yet — a test receipt is a later slice.
                </div>
              ) : (
                <ul className="space-y-1">
                  {testMembers.map((m) => (
                    <li key={m.path}>
                      <button
                        type="button"
                        data-role="file-path"
                        data-path={m.path}
                        onClick={() => {
                          setSelectedPath(m.path);
                          setTab('files');
                        }}
                        className="text-[11px] font-mono text-ink-soft hover:text-ink"
                      >
                        {m.path}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}

          {tab === 'receipts' && (
            <div className="flex-1 overflow-y-auto p-4" data-role="receipts-tab">
              <div className="text-sm font-mono text-ink">
                Receipts {rollup.receiptsEarned}/{rollup.receiptsRequired}
                <span className="text-ink-soft"> · {STATE_LABEL[rollup.state]}</span>
              </div>
              <div className="text-[10px] uppercase tracking-wide text-ink-soft mt-3">Required</div>
              <ul className="mt-1 space-y-1">
                {block.receipt_contract.required.map((req) => {
                  const earned = rollup.earnedTypes.includes(req.type);
                  return (
                    <li key={req.type} className="text-[11px] font-mono flex justify-between" data-role="receipt-required">
                      <span className="text-ink">{req.type}</span>
                      <span className={earned ? 'text-verdict-act' : 'text-ink-soft'}>
                        {earned ? 'earned-fresh' : 'not earned yet'}
                      </span>
                    </li>
                  );
                })}
                {block.receipt_contract.required.length === 0 && (
                  <li className="text-[11px] font-mono text-ink-soft">no required contract — not scanned yet</li>
                )}
              </ul>
              {block.receipt_contract.optional.length > 0 && (
                <>
                  <div className="text-[10px] uppercase tracking-wide text-ink-soft mt-3">Optional (never counted)</div>
                  <ul className="mt-1 space-y-1">
                    {block.receipt_contract.optional.map((o) => (
                      <li key={o.type} className="text-[11px] font-mono text-ink-soft">
                        {o.type} — neutral, not required
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {block.receipt_contract.waived.length > 0 && (
                <p className="text-[10px] text-ink-soft mt-3">
                  waived: {block.receipt_contract.waived.map((w) => w.type).join(' · ')}
                </p>
              )}
            </div>
          )}

          {tab === 'impact' && (
            <div className="flex-1 overflow-y-auto p-4" data-role="impact-tab">
              <div className="text-xs text-ink">
                connects to <span className="font-mono tabular-nums">{socketCount}</span> declared
                {socketCount === 1 ? ' socket' : ' sockets'}
              </div>
              {socketCount === 0 ? (
                <div className="mt-2 text-xs text-ink-soft" data-role="impact-empty">
                  No declared sockets — this block connects to nothing in the skeleton yet.
                </div>
              ) : (
                <div className="mt-3 space-y-3">
                  {block.sockets.outputs.length > 0 && (
                    <div data-role="impact-outputs">
                      <div className="text-[10px] uppercase tracking-wide text-ink-soft">Outputs</div>
                      <ul className="mt-1 space-y-0.5">
                        {block.sockets.outputs.map((s, i) => (
                          <li key={i} className="text-[11px] font-mono text-ink">
                            <span className="text-socket-blue">→ </span>
                            {socketTarget(s)}
                            {s.type ? <span className="text-ink-soft"> · {s.type}</span> : null}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                  {block.sockets.inputs.length > 0 && (
                    <div data-role="impact-inputs">
                      <div className="text-[10px] uppercase tracking-wide text-ink-soft">Inputs</div>
                      <ul className="mt-1 space-y-0.5">
                        {block.sockets.inputs.map((s, i) => (
                          <li key={i} className="text-[11px] font-mono text-ink">
                            <span className="text-socket-blue">→ </span>
                            {socketTarget(s)}
                            {s.type ? <span className="text-ink-soft"> · {s.type}</span> : null}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                  {block.sockets.external.length > 0 && (
                    <div data-role="impact-external">
                      <div className="text-[10px] uppercase tracking-wide text-ink-soft">External</div>
                      <ul className="mt-1 space-y-0.5">
                        {block.sockets.external.map((s, i) => (
                          <li key={i} className="text-[11px] font-mono text-ink">
                            <span className="text-socket-blue">→ </span>
                            {socketTarget(s)}
                            {s.type ? <span className="text-ink-soft"> · {s.type}</span> : null}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              )}
              <p className="mt-3 text-[10px] text-ink-soft">
                structural neighbours from declared sockets — not a computed blast radius.
              </p>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
