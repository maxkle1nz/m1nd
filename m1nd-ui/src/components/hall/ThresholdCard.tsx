/*
 * ThresholdCard — first-run as empty state, not wizard (HUMAN-LAYER-PRD §4A.2).
 *
 * The moment a human opens the served UI and the owner holds nothing. That
 * emptiness IS the onboarding — no overlay tour, no checklist, no account. One
 * calm sentence, one action: read the first repo. The bootstrap is the one-call
 * two-tier envelope (ingest {path, project_root}) when the schema advertises it
 * (feature-detected — INV-11), else a plain ingest (safe here and only here, on
 * an empty owner — the clobber ban). Progress is WORDS, never a fabricated
 * percent (INV-05): the SSE `ingest` event is a completion, so we show calm
 * indeterminate copy, then land on the tree.
 */
import { useEffect, useState } from 'react';
import { api } from '../../api/client';
import { useSSE } from '../../hooks/useSSE';
import type { SseEvent } from '../../types';
import {
  ingestSupportsProjectRoot,
  bootstrapParams,
  progressCopy,
  type ThresholdPhase,
} from '../../lib/threshold';

interface ThresholdCardProps {
  /** Called once the first repo is read — the parent lands on the tree + orients. */
  onBootstrapped: () => void;
}

export default function ThresholdCard({ onBootstrapped }: ThresholdCardProps) {
  const [path, setPath] = useState('');
  const [phase, setPhase] = useState<ThresholdPhase>('idle');
  const [supportsProjectRoot, setSupportsProjectRoot] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Feature-detect the one-call bootstrap (INV-11) — never assume it.
  useEffect(() => {
    let mounted = true;
    api
      .tools()
      .then((r) => mounted && setSupportsProjectRoot(ingestSupportsProjectRoot(r.tools)))
      .catch(() => mounted && setSupportsProjectRoot(false));
    return () => {
      mounted = false;
    };
  }, []);

  // The SSE `ingest` completion lands us on the tree (§4A.2 step 3).
  const onEvent = (event: SseEvent) => {
    if (phase === 'reading' && event.event_type === 'ingest') {
      setPhase('done');
      // Give the "opening your map" beat a beat, then hand off.
      window.setTimeout(onBootstrapped, 300);
    }
  };
  useSSE({ onEvent, enabled: phase === 'reading' });

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!path.trim() || phase === 'reading') return;
    setError(null);
    setPhase('reading');
    try {
      await api.tool('ingest', bootstrapParams(path, supportsProjectRoot));
      // Belt-and-suspenders: if the SSE event was missed, the awaited call
      // completing is itself the completion — land on the tree.
      setPhase('done');
      window.setTimeout(onBootstrapped, 300);
    } catch (err) {
      setPhase('idle');
      setError(err instanceof Error ? err.message : 'Could not read the repo.');
    }
  };

  const reading = phase === 'reading' || phase === 'done';

  return (
    <div className="flex-1 flex items-center justify-center bg-porcelain" data-surface="threshold">
      <div className="max-w-md w-full px-8 text-center space-y-5">
        <div className="text-4xl text-ink-soft/30">🌱</div>
        <div className="space-y-2">
          <h1 className="text-xl text-ink font-semibold">Welcome to m1nd</h1>
          <p className="text-sm text-ink-soft leading-relaxed">
            m1nd keeps a living map of your code — what's proven, what's guessed, what changed.
          </p>
        </div>

        <form onSubmit={submit} className="space-y-3">
          <input
            type="text"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="/path/to/your/project"
            disabled={reading}
            data-role="threshold-path"
            className="w-full bg-bone/60 border border-ink/15 text-ink text-sm font-mono rounded px-3 py-2 outline-none focus:border-ink/30 placeholder-ink-soft/50 disabled:opacity-60"
            autoFocus
          />
          <button
            type="submit"
            disabled={!path.trim() || reading}
            data-role="read-first-repo"
            className="w-full px-4 py-2 text-sm bg-bone text-ink border border-ink/25 rounded hover:shadow-contact disabled:opacity-50 transition-shadow"
          >
            {reading ? 'Reading…' : 'Read your first repo'}
          </button>
        </form>

        {/* Word-grained progress — never a bar, never a percent (INV-05) */}
        {reading && (
          <div data-role="threshold-progress" className="text-xs text-ink-soft flex items-center justify-center gap-2">
            <span className="w-1.5 h-1.5 rounded-full bg-verdict-act inline-block" aria-hidden />
            {progressCopy(phase)}
          </div>
        )}

        {error && (
          <div className="text-xs text-ink font-mono border border-state-failure/25 bg-state-failure-tint/40 rounded px-3 py-2">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}
