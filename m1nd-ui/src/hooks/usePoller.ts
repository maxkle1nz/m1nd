/*
 * usePoller — the safe status-poll loop the Hall's reads share.
 *
 * Every status poller in the Hall used the same naive shape — `setInterval(() =>
 * void load(), N)` — with three holes this hook closes once, for all of them:
 *
 *  1. anti-stacking: a tick that fires while the previous request is still in
 *     flight is SKIPPED (the pure `createPollGuard`), so a stalled server can no
 *     longer pile up requests (observed live: 19 pending health polls under
 *     SIGSTOP). At most ONE request per poller is ever outstanding.
 *  2. visibility pause: while the tab is hidden the interval ticks are skipped;
 *     when the tab returns the loop polls immediately (no full-interval wait for
 *     fresh data). The mount/dep-change poll always runs (you just arrived).
 *  3. teardown abort: unmount, `enabled → false`, or a `deps` change aborts the
 *     in-flight request through its AbortSignal — no request outlives its surface.
 *
 * `poll` receives the `AbortSignal` to thread into its fetch. `intervalMs === null`
 * means poll ONCE per mount/dep-change with no repeating timer (the collapsed
 * mission strip's "one glance, not a live drain"). `deps` re-arm the loop.
 */
import { useEffect, useRef, type DependencyList } from 'react';
import { createPollGuard } from './pollGuard';

function documentHidden(): boolean {
  return typeof document !== 'undefined' && document.hidden === true;
}

export function usePoller(
  poll: (signal: AbortSignal) => Promise<void>,
  intervalMs: number | null,
  enabled: boolean,
  deps: DependencyList = [],
): void {
  // Keep the latest poll closure without re-arming the loop every render.
  const pollRef = useRef(poll);
  pollRef.current = poll;

  useEffect(() => {
    if (!enabled) return;
    const guard = createPollGuard();

    const run = (respectVisibility: boolean) => {
      if (respectVisibility && documentHidden()) return; // paused while backgrounded
      const controller = guard.begin();
      if (!controller) return; // a previous poll is still in flight — skip (never stack)
      void pollRef.current(controller.signal).finally(() => guard.settle(controller));
    };

    // Poll immediately on mount/dep-change regardless of visibility — the surface
    // just appeared (or its inputs changed) and wants data now.
    run(false);

    if (intervalMs === null) {
      // One-shot: no repeating timer, but still abort the in-flight poll on teardown.
      return () => guard.abort();
    }

    const id = setInterval(() => run(true), intervalMs);
    const onVisible = () => run(true); // tab came back → poll now, don't wait a full tick
    const hasDoc = typeof document !== 'undefined';
    if (hasDoc) document.addEventListener('visibilitychange', onVisible);
    return () => {
      clearInterval(id);
      if (hasDoc) document.removeEventListener('visibilitychange', onVisible);
      guard.abort();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, intervalMs, ...deps]);
}
