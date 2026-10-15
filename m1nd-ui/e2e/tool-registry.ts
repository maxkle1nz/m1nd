import { readFileSync } from 'node:fs';

/** Actual native full-registry capture; the historical G0 fixture stays separate. */
export const currentToolRegistry = JSON.parse(
  readFileSync(new URL('../src/__fixtures__/tools-current.json', import.meta.url), 'utf8'),
);

const withdrawnToolNames = new Set([
  'transplant', 'transplant_preview', 'transplant_commit',
  'mission_spawn', 'candidate_naming', 'curation_spawn',
]);

/** A removed write must never receive the generic mock's successful envelope. */
export function withdrawnToolPost(path: string, method: string): boolean {
  if (method !== 'POST' || !path.startsWith('/api/tools/')) return false;
  return withdrawnToolNames.has(path.slice('/api/tools/'.length).replace(/^m1nd\./, ''));
}
