/*
 * scanMachine — the scan gesture's loading STATE MACHINE (HUMAN-VIEW-V2 F0c §5,
 * UX fix: docs/uml/scan-loading.md). `skeleton_candidate` is one synchronous POST
 * on the owner (repo file list → Louvain clustering → a naming-runner batch whose
 * owner-side wait budget caps at 110s → persistence) with NO progress events —
 * minutes of held request. The old UI collapsed all of that into one boolean
 * (`scanning`), so the button read "Scanning…" and the screen looked dead.
 *
 * This module is the PURE core: named states, real events, a total reducer.
 * Laws (the invariants the tests pin):
 *  - NEVER-DEAD: while a scan is in flight the state always carries a live
 *    elapsed clock (`elapsedMs`) and a phase name — the render always has
 *    movement to show. No state hides the wait.
 *  - REAL EVENTS ONLY: the machine advances on a response, an error, a user
 *    gesture, or a timer tick — never on an invented percentage. There is no
 *    progress fraction anywhere in this file by design (the owner emits none;
 *    fabricating one would violate the honesty law).
 *  - TOTAL: every (state, event) pair is defined; an event that does not apply
 *    returns the state unchanged. The machine cannot throw and cannot wedge.
 *  - HONEST ABORT: aborting the fetch closes the browser's wait, NOT the owner's
 *    work — the scan may still land. The canceled toast says exactly that.
 */
import type { WriteToast } from './buildMap';

// ---------------------------------------------------------------------------
// States and events.
// ---------------------------------------------------------------------------

/** The named phases of one scan gesture.
 *  idle → submitting → clustering → (slow) → candidate_ready | error
 *  `slow` is clustering past the threshold — same wait, the UI now SAYS it is
 *  long and keeps counting. `candidate_ready` = resolved + reload fired (the
 *  map re-renders into candidate dress). `error` = refused/failed, retryable. */
export type ScanPhaseName =
  | 'idle'
  | 'submitting'
  | 'clustering'
  | 'slow'
  | 'candidate_ready'
  | 'error';

/** The owner-named phases the scan emits on the SSE channel (slice 2,
 *  docs/uml/scan-loading.md). `done`/`failed` are terminal — the authoritative
 *  outcome still arrives on the HTTP response (RESOLVED); these only close the
 *  narration. */
export type ServerScanPhaseName =
  | 'file_list'
  | 'clustering'
  | 'naming'
  | 'persisting'
  | 'done'
  | 'failed';

const SERVER_PHASES: ReadonlySet<string> = new Set<ServerScanPhaseName>([
  'file_list',
  'clustering',
  'naming',
  'persisting',
  'done',
  'failed',
]);

/** The latest server-reported phase — DISPLAY enrichment only (it never drives the
 *  client state machine; the clock and phase transitions stay TICK/response-driven,
 *  REAL EVENTS ONLY). Facts the owner actually computed at the boundary: a phase,
 *  the counts it knew, and the naming budget's wave estimate — never a percentage. */
export interface ScanServerPhase {
  phase: ServerScanPhaseName;
  fileCount: number | null;
  nodeCount: number | null;
  edgeCount: number | null;
  blockCount: number | null;
  namingWaves: number | null;
}

/** Parse a raw `scan_progress` SSE payload into a `ScanServerPhase`, or `null` when
 *  the phase is missing/unknown (a malformed or future event is ignored — honest
 *  degradation, never a throw). The wire is snake_case; the machine is camelCase. */
export function scanServerPhaseFromEvent(data: unknown): ScanServerPhase | null {
  if (data == null || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  const phase = d.phase;
  if (typeof phase !== 'string' || !SERVER_PHASES.has(phase)) return null;
  const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  return {
    phase: phase as ServerScanPhaseName,
    fileCount: num(d.file_count),
    nodeCount: num(d.node_count),
    edgeCount: num(d.edge_count),
    blockCount: num(d.block_count),
    namingWaves: num(d.naming_waves),
  };
}

export interface ScanMachineState {
  phase: ScanPhaseName;
  /** Epoch ms when the request left (the SCAN event). `null` only in idle-from-reset. */
  startedAt: number | null;
  /** Milliseconds since `startedAt`, advanced ONLY by real events (TICK/RESOLVED/
   *  ABORTED carry their own clock). Frozen at the final value once resolved. */
  elapsedMs: number;
  /** The honest outcome (ok / conflict / readonly / error / canceled). `null`
   *  while in flight — starting a scan clears the previous outcome. */
  toast: WriteToast | null;
  /** The latest server-reported phase (SSE `scan_progress`), or `null` when the
   *  owner emits none (older owner / channel closed) — the panel then degrades to
   *  the static client phase label, EXACTLY this slice's prior behavior. Cleared
   *  on every new SCAN and on ABORT/RESET. */
  serverPhase: ScanServerPhase | null;
}

export type ScanMachineEvent =
  /** The human clicked Scan (or Retry — same gesture). Legal from idle,
   *  error, and candidate_ready (a re-scan); ignored while one is in flight. */
  | { type: 'SCAN'; at: number }
  /** The POST left the browser (fetch dispatched). */
  | { type: 'SENT'; at: number }
  /** The 1s heartbeat while in flight — the real timer event that moves the clock. */
  | { type: 'TICK'; at: number }
  /** The request settled. `reloading` mirrors runScan's decision: ok/conflict
   *  reload (→ candidate_ready); readonly/error do not (→ error, retry stays). */
  | { type: 'RESOLVED'; at: number; toast: WriteToast; reloading: boolean }
  /** The human stopped WAITING (AbortController). The owner may still finish. */
  | { type: 'ABORTED'; at: number }
  /** A server-named phase arrived on the SSE channel (slice 2). DISPLAY only — it
   *  enriches the label; it never moves the clock or the client phase machine. */
  | { type: 'PHASE'; server: ScanServerPhase }
  /** Dismiss the outcome toast (from candidate_ready or error → idle). */
  | { type: 'DISMISS_TOAST' }
  /** External reset (e.g. the reload landed a store and the empty screen is gone). */
  | { type: 'RESET' };

/** After this much in-flight time the machine enters `slow` — the UI adds the
 *  honest "this usually takes a while" note and keeps counting. 10s: clustering
 *  alone finishes under it; a live naming-runner call (~50s measured, 110s cap)
 *  does not. */
export const SCAN_SLOW_AFTER_MS = 10_000;

/** The heartbeat period the driving hook uses (1s — a calm clock, not a spinner). */
export const SCAN_TICK_MS = 1_000;

export function scanIdle(): ScanMachineState {
  return { phase: 'idle', startedAt: null, elapsedMs: 0, toast: null, serverPhase: null };
}

const IN_FLIGHT: ReadonlySet<ScanPhaseName> = new Set(['submitting', 'clustering', 'slow']);

/** True while a request is out (the button locks, the wait panel shows). */
export function isScanInFlight(phase: ScanPhaseName): boolean {
  return IN_FLIGHT.has(phase);
}

/** The honest canceled toast (HONEST ABORT law): closing the browser's wait does
 *  not stop the owner — `handle_skeleton_candidate` runs to completion and may
 *  still write the candidate store. */
export function canceledScanToast(): WriteToast {
  return {
    kind: 'canceled',
    text: 'stopped waiting — the owner may still finish this scan; reload to check',
  };
}

// ---------------------------------------------------------------------------
// The reducer — total over (state, event).
// ---------------------------------------------------------------------------

/** Elapsed from the machine's own clock; a skewed/backwards clock floors at 0
 *  (the display never runs negative — NEVER-DEAD includes never-absurd). */
function elapsedFrom(startedAt: number | null, at: number): number {
  if (startedAt == null) return 0;
  return Math.max(0, at - startedAt);
}

/**
 * Advance the machine by one real event. Pure and total: an event that does not
 * apply in the current phase returns the state UNCHANGED (same reference), so
 * a late RESOLVED after an abort, a stray TICK after settle, or a double SCAN
 * are all provable no-ops.
 */
export function scanReducer(
  state: ScanMachineState,
  event: ScanMachineEvent,
  slowAfterMs: number = SCAN_SLOW_AFTER_MS,
): ScanMachineState {
  switch (event.type) {
    case 'SCAN': {
      // Only from a settled phase — a click while in flight is the button's
      // disabled state anyway; the machine refuses it too (defense in depth).
      if (isScanInFlight(state.phase)) return state;
      return {
        phase: 'submitting',
        startedAt: event.at,
        elapsedMs: 0,
        toast: null,
        serverPhase: null,
      };
    }
    case 'SENT': {
      if (state.phase !== 'submitting') return state;
      return { ...state, phase: 'clustering', elapsedMs: elapsedFrom(state.startedAt, event.at) };
    }
    case 'TICK': {
      if (!isScanInFlight(state.phase)) return state;
      const elapsedMs = elapsedFrom(state.startedAt, event.at);
      // Liveness guard: even if SENT were ever missed, the heartbeat still
      // advances submitting → clustering — the wait can never look dead.
      const phase: ScanPhaseName = elapsedMs >= slowAfterMs ? 'slow' : 'clustering';
      return { ...state, phase, elapsedMs };
    }
    case 'RESOLVED': {
      if (!isScanInFlight(state.phase)) return state;
      return {
        ...state,
        phase: event.reloading ? 'candidate_ready' : 'error',
        elapsedMs: elapsedFrom(state.startedAt, event.at),
        toast: event.toast,
      };
    }
    case 'PHASE': {
      // Server narration is DISPLAY enrichment ONLY: it never advances the clock
      // or the client phase machine (REAL EVENTS ONLY holds — the elapsed clock
      // stays TICK-driven). Applies only in flight; a stray PHASE after settle is a
      // provable no-op (TOTAL law). A terminal server phase does NOT settle the
      // machine — the authoritative outcome is the HTTP response (RESOLVED).
      if (!isScanInFlight(state.phase)) return state;
      return { ...state, serverPhase: event.server };
    }
    case 'ABORTED': {
      if (!isScanInFlight(state.phase)) return state;
      return {
        phase: 'idle',
        startedAt: null,
        elapsedMs: elapsedFrom(state.startedAt, event.at),
        toast: canceledScanToast(),
        serverPhase: null,
      };
    }
    case 'DISMISS_TOAST': {
      if (state.toast == null) return state;
      // Dismissing the outcome settles the gesture back to idle (the button is
      // already re-enabled in candidate_ready/error; this only clears the note).
      const phase = isScanInFlight(state.phase) ? state.phase : 'idle';
      return { ...state, phase, toast: null };
    }
    case 'RESET': {
      if (state.phase === 'idle' && state.toast == null) return state;
      return scanIdle();
    }
  }
}

// ---------------------------------------------------------------------------
// Display derivations (pure — the component stays a projection).
// ---------------------------------------------------------------------------

/** "0:07", "1:52", "12:03" — the calm mm:ss clock the wait panel shows. */
export function formatElapsed(ms: number): string {
  const totalSecs = Math.max(0, Math.floor(ms / 1000));
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  return `${mins}:${String(secs).padStart(2, '0')}`;
}

/** The phase line of the wait panel. `slow` keeps the clustering verb — it is the
 *  same real phase, only older (the note below carries the "takes a while"). */
export function scanPhaseLabel(phase: ScanPhaseName): string {
  switch (phase) {
    case 'submitting':
      return 'sending the scan request…';
    case 'clustering':
    case 'slow':
      return 'clustering…';
    default:
      return '';
  }
}

const fmtCount = (n: number) => n.toLocaleString('en-US');

/** What the wait is actually doing, with the REAL node count when the graph
 *  stats read landed (never an invented number — absent stays generic). */
export function scanWaitCopy(nodeCount: number | null): string {
  return nodeCount != null
    ? `clustering ${fmtCount(nodeCount)} nodes into a candidate map`
    : 'clustering this repo into a candidate map';
}

/** The `slow` note — the screen SAYS the wait is long and why, instead of
 *  looking dead. Grounded: a live naming-runner batch waits up to ~2 minutes on
 *  the owner (naming_runner.rs budget, 110s cap) before the heuristic fallback. */
export function scanSlowNote(nodeCount: number | null): string {
  const subject = nodeCount != null ? `${fmtCount(nodeCount)} nodes` : 'a large graph';
  return `still working — clustering ${subject} usually takes a while (a live naming runner can add ~2 minutes). The request stays open.`;
}

/** The owner-named phase, rendered as a label. FACTS only — the counts the owner
 *  actually computed (files, nodes, blocks); NEVER a percentage. The slow naming
 *  call names its block count (the client clock carries the wait). The terminal
 *  `done`/`failed` phases return '' — they are settled by the HTTP response, so the
 *  panel is gone; `scanDisplayLabel` then falls back to the client phase label. */
export function serverPhaseLabel(server: ScanServerPhase): string {
  switch (server.phase) {
    case 'file_list':
      return server.fileCount != null ? `listing ${fmtCount(server.fileCount)} files…` : 'listing files…';
    case 'clustering':
      return server.nodeCount != null
        ? `clustering ${fmtCount(server.nodeCount)} nodes…`
        : 'clustering…';
    case 'naming':
      return server.blockCount != null ? `naming ${fmtCount(server.blockCount)} blocks…` : 'naming blocks…';
    case 'persisting':
      return 'saving the candidate map…';
    default:
      return '';
  }
}

/** The phase line the wait panel shows: the SERVER-named phase when the owner is
 *  narrating (slice 2), else the static client label — a clean degradation to this
 *  slice's prior behavior when no SSE is flowing (older owner / channel closed). */
export function scanDisplayLabel(phase: ScanPhaseName, server: ScanServerPhase | null): string {
  if (server) {
    const label = serverPhaseLabel(server);
    if (label) return label;
  }
  return scanPhaseLabel(phase);
}
