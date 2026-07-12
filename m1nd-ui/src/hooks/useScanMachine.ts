/*
 * useScanMachine — the thin React driver for the scan loading state machine
 * (lib/scanMachine.ts, the pure core; docs/uml/scan-loading.md). Owns the three
 * impure pieces the reducer must not: the 1s heartbeat interval (the TICK
 * source), the AbortController (the "stop waiting" gesture), and the wall
 * clock. Everything decidable lives in the reducer, so this hook stays a wire.
 *
 * Lifecycle it drives (BuildMapView is the caller):
 *   begin() → SCAN (arms an AbortController, returns it)
 *   sent()  → SENT (the POST left)
 *   [interval while in flight] → TICK every second
 *   resolve(toast, reloading) → RESOLVED   (ignored after an abort — total reducer)
 *   abort() → aborts the fetch + ABORTED   (honest canceled toast)
 *   dismissToast() / reset() → the settle gestures
 * Unmount aborts any in-flight fetch (the wait dies with the surface; the owner
 * still finishes server-side — the honesty note lives in the canceled copy).
 */
import { useCallback, useEffect, useMemo, useReducer, useRef } from 'react';
import {
  isScanInFlight,
  scanIdle,
  scanReducer,
  SCAN_SLOW_AFTER_MS,
  SCAN_TICK_MS,
  type ScanMachineState,
} from '../lib/scanMachine';
import type { WriteToast } from '../lib/buildMap';

export interface ScanMachineHandle {
  /** The machine's full state (phase, elapsedMs, toast) — render from this. */
  state: ScanMachineState;
  /** True while the request is out (button locks, wait panel shows). */
  inFlight: boolean;
  /** SCAN: start a gesture. Returns the armed AbortController whose signal the
   *  fetch must carry. No-op (returns null) while one is already in flight. */
  begin: () => AbortController | null;
  /** SENT: the POST left the browser. */
  sent: () => void;
  /** RESOLVED: the request settled into runScan's honest toast + reload decision. */
  resolve: (toast: WriteToast, reloading: boolean) => void;
  /** Stop WAITING: abort the fetch and settle to idle with the honest note. */
  abort: () => void;
  dismissToast: () => void;
  reset: () => void;
}

export function useScanMachine(slowAfterMs: number = SCAN_SLOW_AFTER_MS): ScanMachineHandle {
  const [state, dispatch] = useReducer(
    (s: ScanMachineState, e: Parameters<typeof scanReducer>[1]) => scanReducer(s, e, slowAfterMs),
    undefined,
    scanIdle,
  );
  const controllerRef = useRef<AbortController | null>(null);
  const inFlight = isScanInFlight(state.phase);

  // The heartbeat — the machine's only self-driven event. Real timer, 1s, only
  // while in flight; the reducer turns it into elapsed + the slow promotion.
  useEffect(() => {
    if (!inFlight) return;
    const id = setInterval(() => dispatch({ type: 'TICK', at: Date.now() }), SCAN_TICK_MS);
    return () => clearInterval(id);
  }, [inFlight]);

  // Unmount: release the wait (never leave a held fetch behind the surface).
  useEffect(() => {
    return () => controllerRef.current?.abort();
  }, []);

  const begin = useCallback((): AbortController | null => {
    if (controllerRef.current != null && !controllerRef.current.signal.aborted) return null;
    const controller = new AbortController();
    controllerRef.current = controller;
    dispatch({ type: 'SCAN', at: Date.now() });
    return controller;
  }, []);

  const sent = useCallback(() => dispatch({ type: 'SENT', at: Date.now() }), []);

  const resolve = useCallback((toast: WriteToast, reloading: boolean) => {
    controllerRef.current = null;
    dispatch({ type: 'RESOLVED', at: Date.now(), toast, reloading });
  }, []);

  const abort = useCallback(() => {
    controllerRef.current?.abort();
    controllerRef.current = null;
    dispatch({ type: 'ABORTED', at: Date.now() });
  }, []);

  const dismissToast = useCallback(() => dispatch({ type: 'DISMISS_TOAST' }), []);
  const reset = useCallback(() => dispatch({ type: 'RESET' }), []);

  return useMemo(
    () => ({ state, inFlight, begin, sent, resolve, abort, dismissToast, reset }),
    [state, inFlight, begin, sent, resolve, abort, dismissToast, reset],
  );
}
