import { COLORS } from './colors';
import type { SceneConfig } from './types';

export const SCENES: SceneConfig[] = [
  {
    id: 'problem',
    title: 'Rediscovery Is Expensive',
    subtitle: 'Agents lose time when every step starts from raw text again.',
    durationMs: 5000,
    color: COLORS.error,
  },
  {
    id: 'command',
    title: 'Ground The Task',
    subtitle: 'Ask the graph once, then keep moving with ranked structure.',
    durationMs: 5000,
    color: COLORS.M,
  },
  {
    id: 'brain',
    title: 'Read State, Not Just Results',
    subtitle: 'm1nd can tell an agent whether it is triaging, proving, or ready to edit.',
    durationMs: 6000,
    color: COLORS.one,
  },
  {
    id: 'killer',
    title: 'Prepare Safer Edits',
    subtitle: 'Connected edit prep, plan validation, and observable batch writes.',
    durationMs: 6000,
    color: COLORS.N,
  },
  {
    id: 'economy',
    title: 'Spend Less To Orient',
    subtitle: 'Less context churn, fewer false starts, narrower structural work.',
    durationMs: 5000,
    color: COLORS.D,
  },
  {
    id: 'proof',
    title: 'How The Workflow Changes',
    subtitle: 'm1nd helps agents know what to do next, not just what exists.',
    durationMs: 7000,
    color: COLORS.M,
  },
  {
    id: 'identity',
    title: 'The Runtime',
    subtitle: 'Guided tools, proof state, recovery loops, and local-first execution.',
    durationMs: 5000,
    color: COLORS.N,
  },
  {
    id: 'philosophy',
    title: 'The Product',
    subtitle: 'Before the model finishes reading, m1nd has already found the cut.',
    durationMs: 10000,
    color: COLORS.D,
  },
];

export const TOTAL_DURATION_MS = SCENES.reduce((sum, s) => sum + s.durationMs, 0);
