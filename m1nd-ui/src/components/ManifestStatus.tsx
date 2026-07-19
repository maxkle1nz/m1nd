import React, { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { OrganismManifestResponseV1 } from '../api/types';

export const MANIFEST_POLL_MS = 10_000;

export type ManifestLoadState =
  | { readonly kind: 'loading' }
  | {
      readonly kind: 'ready';
      readonly response: OrganismManifestResponseV1;
      readonly receivedAt: number;
    }
  | { readonly kind: 'unavailable'; readonly detail: string };

export type ManifestLoader = (
  brain?: string | null,
  signal?: AbortSignal,
) => Promise<OrganismManifestResponseV1>;

/** One injected read, factored so success/failure behavior is deterministic in tests. */
// eslint-disable-next-line react-refresh/only-export-components -- deterministic poll seam
export async function readManifestState(
  loadManifest: ManifestLoader,
  now: () => number,
  signal?: AbortSignal,
  brain?: string | null,
): Promise<ManifestLoadState> {
  try {
    const response = await loadManifest(brain, signal);
    return { kind: 'ready', response, receivedAt: now() };
  } catch (error) {
    return {
      kind: 'unavailable',
      detail: error instanceof Error ? error.message : 'manifest request failed',
    };
  }
}

export interface ManifestFreshness {
  readonly label: string;
  readonly state: 'fresh' | 'aging' | 'stale' | 'clock_drift';
}

/** Display-only age derivation. The manifest timestamp itself is never rewritten. */
// eslint-disable-next-line react-refresh/only-export-components -- deterministic freshness seam
export function manifestFreshness(generatedAt: number, now: number): ManifestFreshness {
  const ageMs = now - generatedAt;
  if (ageMs < -5_000) {
    return {
      label: `GEN +${Math.ceil(Math.abs(ageMs) / 1_000)}s CLOCK`,
      state: 'clock_drift',
    };
  }
  const ageSeconds = Math.max(0, Math.floor(ageMs / 1_000));
  if (ageSeconds <= 30) return { label: `GEN ${ageSeconds}s FRESH`, state: 'fresh' };
  if (ageSeconds <= 120) {
    return { label: `GEN ${Math.floor(ageSeconds / 60)}m AGING`, state: 'aging' };
  }
  return { label: `GEN ${Math.floor(ageSeconds / 60)}m STALE`, state: 'stale' };
}

function toneForCoherence(coherence: OrganismManifestResponseV1['verification']['coherence']) {
  switch (coherence) {
    case 'COHERENT':
      return 'border-verdict-act/30 text-verdict-act';
    case 'DRIFT':
      return 'border-state-failure/30 text-state-failure';
    case 'DEGRADED':
      return 'border-verdict-reverify/30 text-verdict-reverify';
    case 'UNKNOWN':
      return 'border-ink/15 text-ink-soft';
  }
}

function freshnessTone(state: ManifestFreshness['state']) {
  switch (state) {
    case 'fresh':
      return 'text-verdict-act';
    case 'aging':
      return 'text-verdict-reverify';
    case 'stale':
    case 'clock_drift':
      return 'text-state-failure';
  }
}

function generatedAtLabel(generatedAt: number): string {
  const date = new Date(generatedAt);
  return Number.isNaN(date.getTime()) ? `epoch-ms:${generatedAt}` : date.toISOString();
}

export function ManifestStatusView({
  state,
  now = Date.now,
}: {
  readonly state: ManifestLoadState;
  readonly now?: () => number;
}) {
  if (state.kind === 'loading') {
    return (
      <div
        data-role="manifest-status"
        data-manifest-state="loading"
        className="px-2 py-0.5 rounded border border-ink/10 text-[10px] font-mono text-ink-soft"
        aria-live="polite"
      >
        MANIFEST CHECKING
      </div>
    );
  }

  if (state.kind === 'unavailable') {
    return (
      <div
        data-role="manifest-status"
        data-manifest-state="unavailable"
        className="px-2 py-0.5 rounded border border-state-failure/25 text-[10px] font-mono text-state-failure"
        title={state.detail}
        aria-live="polite"
      >
        MANIFEST UNAVAILABLE
      </div>
    );
  }

  const { manifest, verification } = state.response;
  const sourceVersion = manifest.source.version;
  const binaryVersion = manifest.runtime.binary_version;
  const bundleVersion = manifest.ui.bundle_version;
  const versionsDrift = sourceVersion !== binaryVersion || sourceVersion !== bundleVersion;
  const versionSummary = `SRC/BIN/BND ${sourceVersion}/${binaryVersion}/${bundleVersion} · ${
    versionsDrift ? 'DRIFT' : 'ALIGNED'
  }`;
  const freshness = manifestFreshness(manifest.generated_at, now());
  const generatedAt = generatedAtLabel(manifest.generated_at);
  const issueSummary = verification.issues.map((issue) => issue.detail).join(' | ');

  return (
    <div
      data-role="manifest-status"
      data-manifest-state="ready"
      className="flex min-w-0 items-center gap-1 text-[10px] font-mono"
      aria-live="polite"
      title={issueSummary || `manifest generated_at=${generatedAt}`}
    >
      <span
        data-manifest-coherence={verification.coherence}
        className={`px-1.5 py-0.5 rounded border ${toneForCoherence(verification.coherence)}`}
      >
        {verification.coherence}
      </span>
      <span
        data-manifest-version-drift={String(versionsDrift)}
        className={`px-1.5 py-0.5 rounded border ${
          versionsDrift || manifest.source.dirty
            ? 'border-state-failure/25 text-state-failure'
            : 'border-verdict-act/25 text-verdict-act'
        }`}
        title={`source=${sourceVersion}; binary=${binaryVersion}; bundle=${bundleVersion}; source_dirty=${manifest.source.dirty}`}
      >
        {versionSummary}
      </span>
      <span
        data-manifest-active-mode={manifest.autonomy.active_mode}
        className="px-1.5 py-0.5 rounded border border-ink/10 text-ink-soft"
      >
        MODE {manifest.autonomy.active_mode}
      </span>
      <span
        data-manifest-issuance-frozen={String(manifest.autonomy.issuance_frozen)}
        className={`px-1.5 py-0.5 rounded border ${
          manifest.autonomy.issuance_frozen
            ? 'border-ink/10 text-ink-soft'
            : 'border-verdict-reverify/30 text-verdict-reverify'
        }`}
      >
        FROZEN {String(manifest.autonomy.issuance_frozen)}
      </span>
      <span
        data-manifest-freshness={freshness.state}
        data-manifest-generated-at={String(manifest.generated_at)}
        className={`px-1.5 py-0.5 whitespace-nowrap ${freshnessTone(freshness.state)}`}
        title={`generated_at=${generatedAt}`}
      >
        {freshness.label}
      </span>
    </div>
  );
}

export default function ManifestStatus({
  brain,
  loadManifest = api.manifest,
  pollMs = MANIFEST_POLL_MS,
  now = Date.now,
}: {
  readonly brain?: string | null;
  readonly loadManifest?: ManifestLoader;
  readonly pollMs?: number;
  readonly now?: () => number;
}) {
  const [state, setState] = useState<ManifestLoadState>({ kind: 'loading' });

  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let controller: AbortController | undefined;
    const delay = Math.max(1_000, pollMs);

    // A brain switch invalidates the old projection immediately. Keeping the
    // previous brain's manifest visible while the selected one loads would make
    // the TopBar claim authority facts for the wrong graph.
    setState({ kind: 'loading' });

    const poll = async () => {
      controller = new AbortController();
      const next = await readManifestState(loadManifest, now, controller.signal, brain);
      if (disposed || controller.signal.aborted) return;
      // A failed refresh replaces the old projection with UNAVAILABLE. Keeping an
      // old manifest on screen as if it were live would fabricate availability.
      setState(next);
      timer = setTimeout(poll, delay);
    };

    void poll();
    return () => {
      disposed = true;
      controller?.abort();
      if (timer) clearTimeout(timer);
    };
  }, [brain, loadManifest, now, pollMs]);

  return <ManifestStatusView state={state} now={now} />;
}
