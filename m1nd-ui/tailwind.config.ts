import type { Config } from 'tailwindcss';

/*
 * SOFT PROOF theme (HUMAN-LAYER-PRD §6.1).
 * Violet lives ONLY under the `abstain` / `iris` families — quarantined and
 * enforced by scripts/violet-lint.mjs. Nothing else references those hues.
 */
const config: Config = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Foundation
        porcelain: '#f7f4ef',
        bone: '#efeae2',
        ink: {
          DEFAULT: '#2b2836',
          soft: '#5b5566',
        },
        // Verdict / state (semantic, non-violet)
        verdict: {
          act: '#6fa287',
          'act-tint': '#dee9dc',
          reverify: '#c89b3c',
          'reverify-tint': '#f0e3c0',
        },
        state: {
          unverified: '#b8b2a8',
          'unverified-tint': '#e9e5de',
          failure: '#b0563b',
          'failure-tint': '#edcec3',
        },
        // VIOLET FAMILY — QUARANTINED to abstain/unknown surfaces only.
        // The violet-lint allow-list is what makes this safe; do not use these
        // classes outside the abstain/unknown component set.
        iris: {
          DEFAULT: '#7c3aed',
          wisteria: '#a78bfa',
          veil: '#ede9fe',
          deep: '#4c1d95',
        },
        // Human View v2 ARTKIT tokens (HUMAN-VIEW-V2-SCREENS §0/§10). The four
        // that the existing SOFT PROOF palette did not already carry; the Build
        // Map's states REUSE the verdict/state families above (the coabitação
        // decision), so only these four are new:
        //   socket-blue  → wires/sockets + the runtime axis ("no signal" today)
        //   stale-lilac  → the candidate/ratification accent (iris is reserved by
        //                  violet-lint, so the candidate wears stale-lilac)
        //   warm-paper   → block card fill
        //   hairline     → the neutral card/border hairline (no-sockets border)
        'socket-blue': '#3d6f8e',
        'stale-lilac': '#8a7ba8',
        'warm-paper': '#fcfaf6',
        hairline: '#d8d1c6',
      },
      fontFamily: {
        // Self-hosted per §6.5 (served UI must work air-gapped). Falls back to
        // system stacks until the woff2 files are vendored.
        sans: ['Instrument Sans', 'system-ui', '-apple-system', 'sans-serif'],
        mono: ['IBM Plex Mono', 'ui-monospace', 'SFMono-Regular', 'monospace'],
        serif: ['Fraunces', 'Georgia', 'serif'],
      },
      boxShadow: {
        contact: '0 1px 2px rgb(43 40 54 / 0.08)',
        card: '0 1px 3px rgb(43 40 54 / 0.1)',
      },
    },
  },
  plugins: [],
};

export default config;
