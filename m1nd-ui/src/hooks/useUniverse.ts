/*
 * useUniverse — the L0 panorama's read (HUMAN-VIEW-V2 F30). Polls
 * `GET /api/universe` (~5s nerve, like the Hall's brains/self reads) and exposes a
 * settled `status` the landing decision gates on.
 *
 * Honest degradation, honest failure (the two are not the same):
 *  - a pre-F30 owner (404) is a SETTLED answer → a READY, EMPTY panorama, so the entry
 *    rule falls through to the prior doctrine untouched (byte-compatible with the Hall's
 *    feature-detections);
 *  - a REAL (non-404) blip with a prior good read keeps LAST-GOOD on screen + a discreet
 *    "read failed — retrying" note (a transient error never blanks a populated sky);
 *  - the FIRST poll failing for a non-404 reason is an honest `error` — it does NOT get
 *    silently reported as an empty sky, and it does NOT decide the landing.
 *
 * The classification is the pure `reduceUniversePoll` (DOM-free, unit-tested — the
 * repo's `loadBuildMap`/`landCandidate` pattern); the hook is the thin poll shell.
 */
import { useCallback, useState } from 'react';
import { api, ApiError } from '../api/client';
import type { UniverseResponse } from '../lib/universe';
import { usePoller } from './usePoller';

export type UniverseStatus = 'loading' | 'ready' | 'error';

const EMPTY: UniverseResponse = {
  schema: 'm1nd-universe-v0',
  worlds: [],
  owner: { alerts_pending: 0 },
  totals: { worlds: 0, awake: 0, pending: 0 },
};

/** Defensive normalization of a partial/legacy body — never a loading hang, never an
 *  undefined field into the render. */
function normalizeUniverse(u: UniverseResponse): UniverseResponse {
  return {
    schema: u.schema ?? EMPTY.schema,
    worlds: Array.isArray(u.worlds) ? u.worlds : [],
    owner: u.owner ?? EMPTY.owner,
    totals: u.totals ?? EMPTY.totals,
  };
}

export interface UniverseData {
  status: UniverseStatus;
  universe: UniverseResponse;
  /** A discreet note when a poll failed but LAST-GOOD is being shown
   *  ("read failed — retrying"). Null when the read is clean or genuinely empty. */
  note: string | null;
  reload: () => void;
}

/** The reducer's carried state — the last render plus whether ANY poll has settled a
 *  real (or degraded-404) answer (`hadGood`), which decides error-vs-last-good. */
export interface UniversePollState {
  status: UniverseStatus;
  universe: UniverseResponse;
  note: string | null;
  hadGood: boolean;
}

export const INITIAL_UNIVERSE_STATE: UniversePollState = {
  status: 'loading',
  universe: EMPTY,
  note: null,
  hadGood: false,
};

/** One poll's outcome: success carries the body; a failure only needs to know whether
 *  it was a 404 (the pre-F30 degrade) or a real error. */
export type UniversePollOutcome =
  | { ok: true; data: UniverseResponse }
  | { ok: false; is404: boolean };

/**
 * Fold one poll outcome onto the prior state — the honest classifier:
 *  - success → READY on the (normalized) body, note cleared, `hadGood` set;
 *  - 404 → a settled degrade to the EMPTY panorama (pre-F30), `hadGood` set;
 *  - real error WITH a prior settled read → KEEP last-good, fly the retry note;
 *  - real error as the FIRST outcome → honest `error`, empty, `hadGood` unset.
 */
export function reduceUniversePoll(
  prev: UniversePollState,
  outcome: UniversePollOutcome,
): UniversePollState {
  if (outcome.ok) {
    return { status: 'ready', universe: normalizeUniverse(outcome.data), note: null, hadGood: true };
  }
  if (outcome.is404) {
    return { status: 'ready', universe: EMPTY, note: null, hadGood: true };
  }
  if (prev.hadGood) {
    return { ...prev, note: 'read failed — retrying' };
  }
  return { status: 'error', universe: EMPTY, note: null, hadGood: false };
}

export function useUniverse(enabled: boolean): UniverseData {
  const [state, setState] = useState<UniversePollState>(INITIAL_UNIVERSE_STATE);
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => setTick((t) => t + 1), []);

  // ~5s panorama nerve, guarded: at most one read in flight (no stacking under a
  // stalled owner), paused while the tab is hidden, aborted on teardown. An abort
  // is NOT a real failure — a torn-down/superseded read returns before it can be
  // misread as an `error` that would blank or fly the retry note over the sky.
  usePoller(
    (signal) =>
      api
        .universe(signal)
        .then((u) => {
          if (!signal.aborted) setState((prev) => reduceUniversePoll(prev, { ok: true, data: u }));
        })
        .catch((err) => {
          if (signal.aborted) return;
          const is404 = err instanceof ApiError && err.status === 404;
          setState((prev) => reduceUniversePoll(prev, { ok: false, is404 }));
        }),
    5000,
    enabled,
    [tick],
  );

  return { status: state.status, universe: state.universe, note: state.note, reload };
}
