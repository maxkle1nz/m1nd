/*
 * PacketCompose — Ask agent / Copy Packet (HUMAN-VIEW-V2-SCREENS §5, PRD F12–F13,
 * F2.5 §4). The panel that turns a block (or block + sub-path) into a MissionPacket.
 * The message field, the INCLUDE toggles, the live PREVIEW (exactly what is
 * copied/posted), and the MODE selector.
 *
 * Two modes are live in F2.5b:
 *   - `clipboard` — the unchanged READ-ONLY compositor: renders Markdown with
 *     `composePacket` (pure) and writes it to the clipboard. ZERO engine call.
 *   - `direct` (§4a) — composes the packet + `mission_post`s the seq-1 `judging`
 *     letter (packet_ref = sha256 of the composed markdown) + copies the packet for
 *     the human to paste into the agent. F2.5a exposes NO common-letter post path
 *     (the mailbox has no recipient field; the only box-writer is `mission_post`),
 *     so the MVP delivers the packet via the clipboard and DECLARES it: "delivery is
 *     not execution." The letter is state (§1c), the paste is the delivery.
 *
 * `spawn` (§4b, F2.5c) is LIVE when a runner daemon is registered: it hands the
 * composed packet to a pinned, live runner via the owner's `mission_spawn` proxy (the
 * browser never holds the shared secret). Disabled with the amendment's honest note
 * ("no runner daemon connected") when none is registered. Mode availability is
 * policy-gated (§4d): a read-only owner → clipboard only. Copy law holds: no
 * "proven/done/correct".
 */
import { useState } from 'react';
import type { BlockRollup, SystemBlock } from '../../lib/buildMap';
import { composePacket, DEFAULT_TOGGLES, type PacketToggles } from '../../lib/packet';
import {
  sendDirectPacket,
  sendSpawnPacket,
  type Capability,
  type LiveRunner,
  type MissionLetter,
  type PostOutcome,
  type SpawnInput,
  type SpawnOutcome,
} from '../../lib/missions';
import { api, ApiError } from '../../api/client';
import { Icon } from '../../lib/icons/registry';

/** Mode availability policy (§4d) — a read-only owner posts nothing. `runnerdAvailable`
 *  un-disables spawn (F2.5c); disabled states always say why. */
export interface ComposePolicy {
  readOnly?: boolean;
  runnerdAvailable?: boolean;
}

export interface PacketComposeProps {
  block: SystemBlock;
  rollup: BlockRollup;
  repoId: string | null;
  /** Optional sub-path scope (block + a member path). */
  subPath?: string | null;
  onClose: () => void;
  /** §4A.9 — the brain the map is viewing; routes the `mission_post` write through
   *  the `?brain=` selector so `direct` posts into the box the human is viewing. */
  brainRoot?: string | null;
  /** §4d — mode availability policy. Absent → writable owner, no runnerd. */
  policy?: ComposePolicy;
  /** The intended lane for the seq-1 letter (§1). Default `build-runner` (§5b). */
  capability?: Capability;
  /** §4b — the pinned, live runners (from `GET /api/runnerd/status`). Non-empty
   *  un-disables the spawn radio and seeds the runner select. */
  liveRunners?: LiveRunner[];
  /** Test/SSR seam: the mission-post writer (default `api.missionPost`). */
  postMission?: (letter: MissionLetter) => Promise<PostOutcome>;
  /** Test/SSR seam: the spawn writer (default `api.missionSpawn`). */
  spawnMission?: (input: SpawnInput) => Promise<SpawnOutcome>;
  /** For tests/SSR: seed the message + toggles + mode deterministically. */
  initialMessage?: string;
  initialToggles?: PacketToggles;
  initialMode?: 'clipboard' | 'direct' | 'spawn';
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

/** The brain reference the seq-1 letter carries (§1f: a repo_id reference, NEVER an
 *  absolute path). Prefer the repoId; fall back to the block_id's repo token. */
function brainRefFor(repoId: string | null, block: SystemBlock): string {
  return repoId ?? block.block_id.match(/^sb_([^_]+)_/)?.[1] ?? 'brain';
}

export default function PacketCompose({
  block,
  rollup,
  repoId,
  subPath = null,
  onClose,
  brainRoot = null,
  policy,
  capability,
  liveRunners,
  postMission,
  spawnMission,
  initialMessage = '',
  initialToggles,
  initialMode = 'clipboard',
}: PacketComposeProps) {
  const [message, setMessage] = useState(initialMessage);
  const [toggles, setToggles] = useState<PacketToggles>(initialToggles ?? DEFAULT_TOGGLES);
  const [copied, setCopied] = useState(false);
  const [mode, setMode] = useState<'clipboard' | 'direct' | 'spawn'>(initialMode);
  const [sending, setSending] = useState(false);
  const [directResult, setDirectResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [spawnResult, setSpawnResult] = useState<{ ok: boolean; message: string } | null>(null);

  const readOnly = policy?.readOnly ?? false;
  const runners = liveRunners ?? [];
  // A runner daemon is available when the status listed live runners (§4b), or the
  // policy asserts it. Spawn is a WRITE (it launches a mission) → a read-only owner
  // can never spawn (§4d).
  const runnerd = (policy?.runnerdAvailable ?? false) || runners.length > 0;
  const directEnabled = !readOnly;
  const spawnEnabled = !readOnly && runnerd && runners.length > 0;
  const [selectedRunner, setSelectedRunner] = useState<string>(runners[0]?.runner_id ?? '');
  // A read-only owner can never be in `direct`/`spawn` — fall back to clipboard (§4d);
  // spawn falls back when no runner is live.
  const activeMode =
    mode === 'spawn' && spawnEnabled
      ? 'spawn'
      : mode === 'direct' && directEnabled
        ? 'direct'
        : 'clipboard';

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

  // `direct` (§4a): post the seq-1 letter, then copy the packet for the human to
  // paste (the honest MVP delivery). An error (`stale_head`, read-only deny,
  // network) surfaces verbatim — never a silent retry (§4d).
  const sendDirect = async () => {
    if (sending) return;
    setSending(true);
    setDirectResult(null);
    const clip = typeof navigator !== 'undefined' ? navigator.clipboard : undefined;
    const poster = postMission ?? ((letter: MissionLetter) => api.missionPost(letter, brainRoot));
    try {
      const res = await sendDirectPacket(
        { markdown, blockId: block.block_id, brainRef: brainRefFor(repoId, block), capability },
        {
          postMission: poster,
          writeClipboard: clip?.writeText ? (t) => clip.writeText(t) : undefined,
        },
      );
      setDirectResult({
        ok: true,
        message: res.clipboardCopied
          ? 'packet copied — paste it into the agent; the mission letter is posted'
          : 'the mission letter is posted — copy the packet to paste it into the agent',
      });
    } catch (err) {
      const detail =
        err instanceof ApiError ? err.detail : err instanceof Error ? err.message : 'the post was refused';
      setDirectResult({ ok: false, message: detail });
    } finally {
      setSending(false);
    }
  };

  // `spawn` (§4b): hand the packet to a pinned, live runner via the owner's
  // `mission_spawn` proxy (the browser never holds the secret). The daemon opens the
  // chain and NEVER lands (§1d); the toast shows the mission_id. An error (no runner,
  // `unpinned_runner`, read-only deny, network) surfaces verbatim — never a fake start.
  const sendSpawn = async () => {
    if (sending || !selectedRunner) return;
    setSending(true);
    setSpawnResult(null);
    const spawner = spawnMission ?? ((input: SpawnInput) => api.missionSpawn(input, brainRoot));
    try {
      const res = await sendSpawnPacket(
        { markdown, blockId: block.block_id, brainRef: brainRefFor(repoId, block), runnerId: selectedRunner },
        { spawnMission: spawner },
      );
      setSpawnResult({ ok: true, message: `mission started — watch the tray (${res.mission_id})` });
    } catch (err) {
      const detail =
        err instanceof ApiError ? err.detail : err instanceof Error ? err.message : 'the spawn was refused';
      setSpawnResult({ ok: false, message: detail });
    } finally {
      setSending(false);
    }
  };

  return (
    <>
      <div className="fixed inset-0 bg-ink/30 z-40" onClick={onClose} aria-hidden />
      <div
        className="fixed top-[8%] left-1/2 -translate-x-1/2 z-50 w-full max-w-3xl mx-4 max-h-[84vh] flex flex-col rounded-lg border border-hairline bg-warm-paper shadow-card"
        data-role="packet-compose"
        data-block-packet={block.block_id}
        data-mode={activeMode}
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
                hint="screenshot capture + redaction arrives with spawn (F2.5c)"
              />
            </div>

            <div className="space-y-1.5">
              <div className="text-[10px] uppercase tracking-wide text-ink-soft">Mode</div>
              <label className="flex items-center gap-2 text-xs text-ink cursor-pointer">
                <input
                  type="radio"
                  name="packet-mode"
                  data-role="mode-clipboard"
                  checked={activeMode === 'clipboard'}
                  onChange={() => setMode('clipboard')}
                  className="accent-socket-blue"
                />
                clipboard — paste anywhere (universal)
              </label>
              <label
                className={`flex items-center gap-2 text-xs ${directEnabled ? 'text-ink cursor-pointer' : 'text-ink-soft opacity-70'}`}
                title={directEnabled ? 'post the mission letter + copy the packet to paste' : 'this owner is read-only — direct posts a letter (a write)'}
              >
                <input
                  type="radio"
                  name="packet-mode"
                  data-role="mode-direct"
                  checked={activeMode === 'direct'}
                  disabled={!directEnabled}
                  onChange={() => setMode('direct')}
                  className="accent-socket-blue"
                />
                direct — deliver to an agent inbox
                {!directEnabled && <span className="text-[10px]">read-only</span>}
              </label>
              <label
                className={`flex items-center gap-2 text-xs ${spawnEnabled ? 'text-ink cursor-pointer' : 'text-ink-soft opacity-70'}`}
                title={
                  spawnEnabled
                    ? 'spawn: the runner runs the packet in an isolated worktree'
                    : readOnly
                      ? 'this owner is read-only — spawn launches a mission (a write)'
                      : 'no runner daemon connected'
                }
              >
                <input
                  type="radio"
                  name="packet-mode"
                  data-role="mode-spawn"
                  checked={activeMode === 'spawn'}
                  disabled={!spawnEnabled}
                  onChange={() => setMode('spawn')}
                  className="accent-socket-blue"
                />
                spawn — launch via a runner{' '}
                <span className="text-[10px]" data-role="spawn-note">
                  {spawnEnabled ? 'runner ready' : 'no runner daemon connected'}
                </span>
              </label>
            </div>
          </div>

          {/* Right: the exact preview + the mode's action + the honest declaration. */}
          <div className="p-4 overflow-hidden flex flex-col min-h-0">
            <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-1">
              Packet preview (exactly what is{' '}
              {activeMode === 'spawn'
                ? 'handed to the runner'
                : activeMode === 'direct'
                  ? 'posted & copied'
                  : 'copied'}
              )
            </div>
            <pre
              data-role="packet-preview"
              className="flex-1 min-h-0 overflow-auto text-[11px] font-mono text-ink bg-porcelain border border-hairline rounded p-2 whitespace-pre-wrap break-words"
            >
              {markdown}
            </pre>

            {activeMode === 'spawn' ? (
              <>
                <div className="mt-3 flex items-center gap-2">
                  <label htmlFor="spawn-runner" className="text-[10px] uppercase tracking-wide text-ink-soft">
                    Runner
                  </label>
                  <select
                    id="spawn-runner"
                    data-role="spawn-runner"
                    value={selectedRunner}
                    onChange={(e) => setSelectedRunner(e.target.value)}
                    className="text-xs text-ink bg-porcelain border border-hairline rounded px-1.5 py-1"
                  >
                    {runners.map((r) => (
                      <option key={r.runner_id} value={r.runner_id}>
                        {r.runner_id}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="mt-2 flex items-center gap-3">
                  <button
                    type="button"
                    data-role="send-spawn"
                    onClick={sendSpawn}
                    disabled={sending || !selectedRunner}
                    className="flex items-center gap-1.5 px-3 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-progress"
                  >
                    <Icon name="agents" size={14} decorative />
                    {sending ? 'Spawning…' : 'Spawn mission'}
                  </button>
                  {spawnResult && (
                    <span
                      data-role="spawn-result"
                      data-ok={spawnResult.ok ? 'true' : 'false'}
                      className={`text-[11px] break-words ${spawnResult.ok ? 'text-verdict-act' : 'text-state-failure'}`}
                    >
                      {spawnResult.message}
                    </span>
                  )}
                </div>
                <p className="mt-2 text-[10px] text-ink-soft" data-role="spawn-note-text">
                  spawn: the runner runs the packet in an isolated worktree — the daemon posts the mission
                  letters (it never lands the receipt; that stays your gesture). Watch the tray.
                </p>
              </>
            ) : activeMode === 'direct' ? (
              <>
                <div className="mt-3 flex items-center gap-3">
                  <button
                    type="button"
                    data-role="send-direct"
                    onClick={sendDirect}
                    disabled={sending}
                    className="flex items-center gap-1.5 px-3 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow disabled:opacity-60 disabled:cursor-progress"
                  >
                    <Icon name="inbox" size={14} decorative />
                    {sending ? 'Sending…' : 'Send to agent inbox'}
                  </button>
                  {directResult && (
                    <span
                      data-role="direct-result"
                      data-ok={directResult.ok ? 'true' : 'false'}
                      className={`text-[11px] break-words ${directResult.ok ? 'text-verdict-act' : 'text-state-failure'}`}
                    >
                      {directResult.message}
                    </span>
                  )}
                </div>
                <p className="mt-2 text-[10px] text-ink-soft" data-role="direct-note">
                  direct: posted to the agent’s inbox — delivery is not execution. The packet is copied for you to
                  paste into the agent; the mission letter is posted to the box.
                </p>
              </>
            ) : (
              <>
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
              </>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
