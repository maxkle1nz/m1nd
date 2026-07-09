/*
 * PacketCompose — Ask agent / Copy Packet (HUMAN-VIEW-V2-SCREENS §5, PRD F12–F13).
 *
 * The panel that turns a block (or block + sub-path) into a MissionPacket. The
 * message field, the INCLUDE toggles, the live PREVIEW (exactly what is copied),
 * and the MODE selector. In this slice ONLY `clipboard` is live — a READ-ONLY
 * compositor: it renders Markdown with `composePacket` (a pure function) and calls
 * `navigator.clipboard.writeText`. ZERO engine call, ZERO delegate registry write
 * (F0-TECH §9); the panel DECLARES this. `direct`/`spawn` are disabled radios
 * pointing at F2.5. Copy law holds: no "proven/done/correct".
 */
import { useState } from 'react';
import type { BlockRollup, SystemBlock } from '../../lib/buildMap';
import { composePacket, DEFAULT_TOGGLES, type PacketToggles } from '../../lib/packet';
import { Icon } from '../../lib/icons/registry';

export interface PacketComposeProps {
  block: SystemBlock;
  rollup: BlockRollup;
  repoId: string | null;
  /** Optional sub-path scope (block + a member path). */
  subPath?: string | null;
  onClose: () => void;
  /** For tests/SSR: seed the message + toggles deterministically. */
  initialMessage?: string;
  initialToggles?: PacketToggles;
}

interface ToggleRowProps {
  label: string;
  checked: boolean;
  onChange?: (v: boolean) => void;
  role: string;
  disabled?: boolean;
  hint?: string;
}

function ToggleRow({ label, checked, onChange, role, disabled, hint }: ToggleRowProps) {
  return (
    <label
      className={`flex items-center gap-2 text-xs ${disabled ? 'text-ink-soft opacity-70' : 'text-ink cursor-pointer'}`}
      title={hint}
    >
      <input
        type="checkbox"
        data-role={role}
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange?.(e.target.checked)}
        className="accent-socket-blue"
      />
      {label}
    </label>
  );
}

export default function PacketCompose({
  block,
  rollup,
  repoId,
  subPath = null,
  onClose,
  initialMessage = '',
  initialToggles,
}: PacketComposeProps) {
  const [message, setMessage] = useState(initialMessage);
  const [toggles, setToggles] = useState<PacketToggles>(initialToggles ?? DEFAULT_TOGGLES);
  const [copied, setCopied] = useState(false);

  const markdown = composePacket({ block, rollup, repoId, message, subPath, toggles });
  const set = (key: keyof PacketToggles) => (v: boolean) => setToggles((t) => ({ ...t, [key]: v }));

  // The ONLY effect of the clipboard mode: write the composed Markdown to the
  // clipboard. No engine call, no delegate — a read-only compositor (F0-TECH §9).
  const copyPacket = () => {
    const clip = typeof navigator !== 'undefined' ? navigator.clipboard : undefined;
    if (clip?.writeText) {
      void clip.writeText(markdown).then(
        () => setCopied(true),
        () => setCopied(false),
      );
    }
  };

  return (
    <>
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} aria-hidden />
      <div
        className="fixed top-[8%] left-1/2 -translate-x-1/2 z-50 w-full max-w-3xl mx-4 max-h-[84vh] flex flex-col rounded-lg border border-hairline bg-warm-paper shadow-card"
        data-role="packet-compose"
        data-block-packet={block.block_id}
      >
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-ink/10">
          <Icon name="agents" size={14} decorative />
          <span className="text-sm font-semibold text-ink">
            Ask agent · from {block.name}
            {subPath ? <span className="text-ink-soft"> ▸ {subPath}</span> : null}
          </span>
          <button
            type="button"
            data-role="packet-close"
            onClick={onClose}
            aria-label="Close"
            className="ml-auto text-ink-soft hover:text-ink text-sm px-1.5"
          >
            ✕
          </button>
        </div>

        <div className="flex-1 min-h-0 grid grid-cols-2 gap-0 overflow-hidden">
          {/* Left: message + toggles + mode. */}
          <div className="border-r border-ink/10 p-4 overflow-y-auto space-y-4">
            <div>
              <label htmlFor="packet-msg" className="text-[10px] uppercase tracking-wide text-ink-soft">
                What should change?
              </label>
              <textarea
                id="packet-msg"
                data-role="packet-message"
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                rows={4}
                placeholder="Describe the change in plain language…"
                className="mt-1 w-full text-xs text-ink bg-porcelain border border-hairline rounded p-2 font-sans resize-y"
              />
            </div>

            <div className="space-y-1.5">
              <div className="text-[10px] uppercase tracking-wide text-ink-soft">Include</div>
              <ToggleRow label="Selected block details" checked={toggles.blockDetails} onChange={set('blockDetails')} role="toggle-blockDetails" />
              <ToggleRow label="Likely files" checked={toggles.likelyFiles} onChange={set('likelyFiles')} role="toggle-likelyFiles" />
              <ToggleRow label="Receipts & evidence state" checked={toggles.receipts} onChange={set('receipts')} role="toggle-receipts" />
              <ToggleRow label="Impact overview" checked={toggles.impact} onChange={set('impact')} role="toggle-impact" />
              <ToggleRow
                label="Screenshot of current view"
                checked={toggles.screenshot}
                role="toggle-screenshot"
                disabled
                hint="screenshot capture + redaction arrives with spawn (F2.5)"
              />
            </div>

            <div className="space-y-1.5">
              <div className="text-[10px] uppercase tracking-wide text-ink-soft">Mode</div>
              <label className="flex items-center gap-2 text-xs text-ink cursor-pointer">
                <input type="radio" name="packet-mode" data-role="mode-clipboard" checked readOnly className="accent-socket-blue" />
                clipboard — paste anywhere (universal)
              </label>
              <label className="flex items-center gap-2 text-xs text-ink-soft opacity-70" title="direct delivery arrives in F2.5">
                <input type="radio" name="packet-mode" data-role="mode-direct" disabled className="accent-socket-blue" />
                direct — deliver to an agent inbox <span className="text-[10px]">F2.5</span>
              </label>
              <label className="flex items-center gap-2 text-xs text-ink-soft opacity-70" title="spawn via a runner arrives in F2.5">
                <input type="radio" name="packet-mode" data-role="mode-spawn" disabled className="accent-socket-blue" />
                spawn — launch via a runner <span className="text-[10px]">F2.5</span>
              </label>
            </div>
          </div>

          {/* Right: the exact preview + copy + the read-only declaration. */}
          <div className="p-4 overflow-hidden flex flex-col min-h-0">
            <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-1">
              Packet preview (exactly what is copied)
            </div>
            <pre
              data-role="packet-preview"
              className="flex-1 min-h-0 overflow-auto text-[11px] font-mono text-ink bg-porcelain border border-hairline rounded p-2 whitespace-pre-wrap break-words"
            >
              {markdown}
            </pre>
            <div className="mt-3 flex items-center gap-3">
              <button
                type="button"
                data-role="copy-packet"
                onClick={copyPacket}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
              >
                <Icon name="receipt" size={14} decorative />
                Copy packet (Markdown)
              </button>
              {copied && (
                <span data-role="packet-copied" className="text-[11px] text-verdict-act">
                  packet copied — paste it into any agent
                </span>
              )}
            </div>
            <p className="mt-2 text-[10px] text-ink-soft" data-role="clipboard-note">
              clipboard mode: no side effects — nothing is written to the engine.
            </p>
          </div>
        </div>
      </div>
    </>
  );
}
