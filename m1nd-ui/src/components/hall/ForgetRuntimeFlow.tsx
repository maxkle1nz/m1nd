/*
 * ForgetRuntimeFlow — the calm two-step delete (HUMAN-LAYER-PRD §4A.4, INV-09).
 *
 * "Delete state" = clean of a STOPPED brain: removes the rebuildable runtime
 * (graph, calibration, caches), never the git-committed memory. The server
 * REFUSES a live brain by construction (instance_registry.rs:333-341) — the flow
 * renders that refusal verbatim and offers no delete path for a live brain.
 *
 * Two DISTINCT confirmations floor the destructive call (INV-09): the
 * consequence-card acknowledge, then a type-the-name gate whose confirm button is
 * unreachable until the typed value equals the repo basename EXACTLY. Matte
 * severity — brick only on the final act, nothing pulses, shakes, or glows
 * (§4A.4, §6.3). Focus lands in the input, never on the destructive button.
 *
 * The step views are a pure function (`ForgetStepView`) so every step is
 * statically renderable and the INV-09 floor is testable without a DOM.
 */
import { useEffect, useRef, useState } from 'react';
import type { InstanceRegistryEntry } from '../../types';
import { api, ApiError } from '../../api/client';
import { useToastStore } from '../../stores/toastStore';
import {
  brainDisplayName,
  nameMatches,
  DELETE_DIES,
  DELETE_SURVIVES,
} from '../../lib/hallSemantics';

export type ForgetStep = 'idle' | 'consequence' | 'confirm';

/** The verbatim server refusal for a live brain (mirrors instance_registry.rs:333-341). */
export function liveRefusalLine(instanceId: string, pid: number): string {
  return `cannot delete runtime state for live instance ${instanceId} (pid ${pid})`;
}

/**
 * INV-09 structural floor: whether the destructive call may fire at all. It is
 * reachable ONLY at the confirm step, on a dormant brain, with an exact name
 * match, and not mid-flight. Below two distinct confirmations this is false.
 */
export function canFireDelete(input: {
  step: ForgetStep;
  isLive: boolean;
  nameMatches: boolean;
  deleting: boolean;
}): boolean {
  return input.step === 'confirm' && !input.isLive && input.nameMatches && !input.deleting;
}

interface ForgetStepViewProps {
  step: ForgetStep;
  entry: InstanceRegistryEntry;
  isLive: boolean;
  basename: string;
  typed: string;
  matches: boolean;
  deleting: boolean;
  refusal: string | null;
  inputRef?: React.Ref<HTMLInputElement>;
  onReset: () => void;
  onContinueToConsequence: () => void;
  onContinueToConfirm: () => void;
  onTyped: (v: string) => void;
  onFire: () => void;
}

/**
 * Pure step view — no hooks, no state. Given the flow's state it returns the
 * markup for that step. Statically renderable → INV-09 is provable without a DOM.
 */
export function ForgetStepView({
  step,
  entry,
  isLive,
  basename,
  typed,
  matches,
  deleting,
  refusal,
  inputRef,
  onReset,
  onContinueToConsequence,
  onContinueToConfirm,
  onTyped,
  onFire,
}: ForgetStepViewProps) {
  // ── The trigger rung (part of the removal ladder) ───────────────────────────
  if (step === 'idle') {
    return (
      <div className="flex items-center justify-between gap-2">
        <button
          type="button"
          data-role="delete-trigger"
          onClick={onContinueToConsequence}
          className="px-3 py-1 text-xs bg-porcelain text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
          title="Forget this brain's runtime state (keeps committed memory)"
        >
          Delete…
        </button>
        <span className="text-[10px] text-ink-soft/70 font-mono">clean runtime · memory kept</span>
      </div>
    );
  }

  // ── Live brain: no delete path — the refusal, verbatim, + the fix ───────────
  if (isLive) {
    return (
      <div data-role="consequence-card" className="rounded-lg border border-ink/15 bg-bone/50 p-3 space-y-2">
        <div className="text-xs text-ink font-semibold">This brain is running — it can't be forgotten yet.</div>
        <div data-role="live-refusal" className="text-[11px] text-ink-soft font-mono border-l-2 border-state-failure/50 pl-2">
          {liveRefusalLine(entry.instance_id, entry.pid)}
        </div>
        <div className="text-[11px] text-ink-soft">
          Stop it first: <code className="select-all font-mono text-ink">m1nd brain stop {basename}</code>
        </div>
        <div className="flex gap-2 pt-1">
          <button type="button" onClick={onReset} className="px-3 py-1 text-xs bg-porcelain text-ink border border-ink/15 rounded hover:shadow-contact">
            Keep it
          </button>
        </div>
      </div>
    );
  }

  // ── Step 1: the consequence card (dormant brain) ────────────────────────────
  if (step === 'consequence') {
    return (
      <div data-role="consequence-card" className="rounded-lg border border-ink/15 bg-bone/50 p-3 space-y-2">
        <div className="text-xs text-ink font-semibold">Forget this brain's runtime?</div>
        <div className="text-[11px] text-ink-soft">
          <span className="text-ink/80">Dies:</span> {DELETE_DIES}.
        </div>
        <div data-role="survives" className="text-[11px] text-ink-soft">
          <span className="text-ink/80">Survives:</span> {DELETE_SURVIVES}.
        </div>
        <div className="flex gap-2 pt-1">
          <button type="button" onClick={onReset} className="px-3 py-1 text-xs bg-porcelain text-ink border border-ink/15 rounded hover:shadow-contact">
            Keep it
          </button>
          <button
            type="button"
            data-role="continue-delete"
            onClick={onContinueToConfirm}
            className="px-3 py-1 text-xs bg-bone text-ink border border-ink/25 rounded hover:shadow-contact"
          >
            Continue…
          </button>
        </div>
      </div>
    );
  }

  // ── Step 2: type the name (confirm disabled until exact match) ──────────────
  return (
    <div data-role="confirm-card" className="rounded-lg border border-ink/15 bg-bone/50 p-3 space-y-2">
      <div className="text-[11px] text-ink-soft">
        Type <code className="font-mono text-ink select-all">{basename}</code> to confirm.
      </div>
      <input
        ref={inputRef}
        type="text"
        value={typed}
        onChange={(e) => onTyped(e.target.value)}
        placeholder={basename}
        data-role="name-input"
        className="w-full bg-porcelain border border-ink/15 text-ink text-sm font-mono rounded px-2 py-1 outline-none focus:border-ink/30 placeholder-ink-soft/50"
        autoComplete="off"
        spellCheck={false}
      />
      {refusal && (
        <div data-role="live-refusal" className="text-[11px] text-ink-soft font-mono border-l-2 border-state-failure/50 pl-2">
          {refusal}
        </div>
      )}
      <div className="flex gap-2 pt-1">
        <button type="button" onClick={onReset} className="px-3 py-1 text-xs bg-porcelain text-ink border border-ink/15 rounded hover:shadow-contact">
          Keep it
        </button>
        <button
          type="button"
          data-role="forget-runtime"
          onClick={onFire}
          disabled={!matches || deleting}
          title={matches ? 'Forget this brain’s runtime state' : 'Type the exact name to enable'}
          className="px-3 py-1 text-xs rounded border border-state-failure/40 text-white bg-state-failure hover:bg-state-failure/90 transition-colors disabled:opacity-40 disabled:cursor-not-allowed disabled:bg-state-failure/60"
        >
          {deleting ? 'Forgetting…' : 'Forget runtime state'}
        </button>
      </div>
    </div>
  );
}

interface ForgetRuntimeFlowProps {
  entry: InstanceRegistryEntry;
  isLive: boolean;
  onDeleted: () => void;
}

export default function ForgetRuntimeFlow({ entry, isLive, onDeleted }: ForgetRuntimeFlowProps) {
  const [step, setStep] = useState<ForgetStep>('idle');
  const [typed, setTyped] = useState('');
  const [deleting, setDeleting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const addToast = useToastStore((s) => s.addToast);
  // The confirm target is the PROJECT name (§4A.4): type "m1nd" to forget, never
  // the runtime dir or the agent-memory sidecar — same name the card shows.
  const basename = brainDisplayName(entry);
  const matches = nameMatches(typed, basename);

  // Focus lands in the input at step 2 — never on the destructive button (§4A.4).
  useEffect(() => {
    if (step === 'confirm') inputRef.current?.focus();
  }, [step]);

  const reset = () => {
    setStep('idle');
    setTyped('');
    setRefusal(null);
  };

  // ESC aborts anywhere in the flow (§4A.4).
  useEffect(() => {
    if (step === 'idle') return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        reset();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [step]);

  const fire = async () => {
    // Structural floor (INV-09): unreachable unless the whole predicate holds.
    if (!canFireDelete({ step, isLive, nameMatches: matches, deleting })) return;
    setDeleting(true);
    setRefusal(null);
    try {
      await api.deleteInstanceState(entry.instance_id);
      addToast('runtime forgotten — memories kept', basename, 'info');
      reset();
      onDeleted();
    } catch (err) {
      // Render the server's refusal verbatim (INV-09) — e.g. the live-instance
      // PermissionDenied. Never a generic "failed".
      const msg = err instanceof ApiError ? err.detail : err instanceof Error ? err.message : 'delete failed';
      setRefusal(msg);
    } finally {
      setDeleting(false);
    }
  };

  return (
    <ForgetStepView
      step={step}
      entry={entry}
      isLive={isLive}
      basename={basename}
      typed={typed}
      matches={matches}
      deleting={deleting}
      refusal={refusal}
      inputRef={inputRef}
      onReset={reset}
      onContinueToConsequence={() => setStep('consequence')}
      onContinueToConfirm={() => setStep('confirm')}
      onTyped={setTyped}
      onFire={fire}
    />
  );
}
