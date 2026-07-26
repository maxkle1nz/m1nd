/*
 * MapStatusScreen — the Build Map's honest non-ready screens (HUMAN-VIEW-V2 F1,
 * the 5-stati law: a read is loading · slow · error, never a silent forever-spin).
 *
 * Pure and prop-only (no hook, no fetch, no timer) so every branch renders
 * deterministically under `renderToStaticMarkup` — the repo's pure-presentational
 * test pattern (BuildMap.tsx). BuildMapView owns the impure parts: the ~10s
 * slow-timer and the retry wiring.
 *
 * The cold-load gap this closes: when the engine is unreachable in a way that HANGS
 * the fetch (socket open, no response — a stalled owner), the read never rejects, so
 * the map used to sit on "Loading repository map…" forever with no note and no way
 * out. Past the slow threshold the loading screen now SAYS the wait is long, names
 * the likely cause, and offers Retry — mirroring the scan wait panel's `slow` note.
 */
import React from 'react';

const RETRY_CLASS =
  'px-3 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow';

/**
 * The loading screen. Below the slow threshold it is the calm one-liner; past it
 * (`slow`) it adds the honest "taking longer — the engine may be unreachable" note
 * and a Retry, so a hung read can never look frozen and dead.
 */
export function MapLoadingScreen({ slow, onRetry }: { slow: boolean; onRetry: () => void }): React.ReactElement {
  return (
    <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-loading">
      {slow ? (
        <div className="text-center space-y-3 px-6" data-role="build-map-loading-slow">
          <div className="text-sm text-ink-soft">Loading repository map…</div>
          <div className="text-xs text-ink-soft max-w-md">
            This is taking longer than usual — the engine may be unreachable. The request stays open.
          </div>
          <button type="button" data-role="retry" onClick={onRetry} className={RETRY_CLASS}>
            Retry
          </button>
        </div>
      ) : (
        <div className="text-sm text-ink-soft">Loading repository map…</div>
      )}
    </div>
  );
}

/** The error screen: the read rejected — say so, show the detail, offer Retry. */
export function MapErrorScreen({
  error,
  onRetry,
}: {
  error: string | null;
  onRetry: () => void;
}): React.ReactElement {
  return (
    <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-error">
      <div className="text-center space-y-3 px-6">
        <div className="text-sm text-state-failure">Failed to load map</div>
        {error && <div className="text-xs text-ink-soft font-mono max-w-md break-words">{error}</div>}
        <button type="button" data-role="retry" onClick={onRetry} className={RETRY_CLASS}>
          Retry
        </button>
      </div>
    </div>
  );
}
