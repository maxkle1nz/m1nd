/*
 * pollGuard — the anti-stacking heart of the status pollers.
 *
 * Framework-free and DOM-free (no React, no timers, no `document`) so the
 * no-stack property is unit-testable deterministically — the repo's
 * `liveRefreshCore` / `scanMachine` pure-core pattern. The React shells that wire
 * it to `setInterval` + visibility live in useRunnerdStatus / useUniverse /
 * usePresences / useMissions.
 *
 * The disease it cures: a poller that fires a fresh request every tick STACKS
 * requests without bound when the server stalls (observed live: 19 pending health
 * polls while the owner was SIGSTOPped). This guard makes stacking impossible —
 * `begin()` hands out a fresh AbortController ONLY when none is in flight, and
 * returns `null` (SKIP this tick) while one still is. `settle(controller)` frees
 * the slot when THAT request finishes; `abort()` cancels the in-flight request on
 * teardown and frees the slot so the guard is immediately reusable.
 */

export interface PollGuard {
  /**
   * Start a poll iff none is in flight. Returns a fresh `AbortController` whose
   * `signal` the fetch must carry (so `abort()` can cancel it), or `null` when a
   * poll is already in flight — the caller SKIPS this tick, and nothing stacks.
   */
  begin(): AbortController | null;
  /**
   * Free the slot for the request `begin()` handed out. Identity-checked: a stale
   * settle (from a request that was already `abort()`ed and superseded) is a no-op,
   * so it can never free a newer in-flight poll's slot.
   */
  settle(controller: AbortController): void;
  /** Abort any in-flight request and free the slot (teardown / disable / dep change). */
  abort(): void;
  /** True while a poll is in flight (a `begin()` right now would return `null`). */
  readonly inFlight: boolean;
}

export function createPollGuard(): PollGuard {
  let current: AbortController | null = null;
  return {
    begin() {
      if (current !== null) return null; // in flight → skip this tick (no stacking)
      current = new AbortController();
      return current;
    },
    settle(controller) {
      // Only the CURRENT request may free the slot — a stale settle from a
      // superseded (aborted) request must not clear a newer poll.
      if (current === controller) current = null;
    },
    abort() {
      current?.abort();
      current = null;
    },
    get inFlight() {
      return current !== null;
    },
  };
}
