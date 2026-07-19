/*
 * ThresholdCard — first-run as empty state, not wizard (HUMAN-LAYER-PRD §4A.2).
 *
 * The moment a human opens the served UI and the owner holds nothing. That
 * emptiness IS the onboarding — no overlay tour, no checklist, no account. One
 * calm sentence and an honest closed state. Creating a first project brain is a
 * sovereign mutation; until the exact typed G2/G3 consumer exists, this surface
 * offers no generic-ingest fallback and changes nothing.
 */

export default function ThresholdCard() {
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

        <div
          data-role="bootstrap-unavailable"
          className="text-xs text-ink font-mono border border-ink/15 bg-bone/50 rounded px-3 py-3 space-y-2"
        >
          <p>Creating the first project brain is unavailable on this owner.</p>
          <p className="text-ink-soft">brain_bootstrap_consumer_not_installed · no repository was changed</p>
        </div>
      </div>
    </div>
  );
}
