/*
 * packet — the MissionPacket compositor (HUMAN-VIEW-V2 F12–F13). Composed against
 * the REAL captured seed fixture. The teeth: the Markdown carries the block path +
 * state + the operator's message + files-by-role + the receipt denominator + the
 * declared sockets; an OFF toggle drops its whole section; screenshot is OFF by
 * default; and the copy law holds — never "proven/done/correct", never an absolute
 * path (the clipboard mode is a pure read-only compositor — no engine write).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { repoIdFromSkeletonId, rollupStore, type SystemBlocksSnapshot } from './buildMap';
import { composePacket, DEFAULT_TOGGLES } from './packet';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const snapshot = JSON.parse(
  readFileSync(join(FIX, 'system_blocks_snapshot.json'), 'utf8'),
) as SystemBlocksSnapshot;
const store = snapshot.store!;
const rollup = rollupStore(store);
const repoId = repoIdFromSkeletonId(store.skeleton.skeleton_id);
// Block 0 — Core Graph Kernel: 13 primary members, spec+structural+test = 3 required.
const block = store.blocks[0];
const blockRollup = rollup.rollups.get(block.block_id)!;
const MSG = 'Add a regression test for CSR activation.';

test('the packet carries block path, state, message, files-by-role, receipts and sockets', () => {
  const md = composePacket({ block, rollup: blockRollup, repoId, message: MSG, toggles: DEFAULT_TOGGLES });
  assert.match(md, /Mission packet — Core Graph Kernel/);
  assert.match(md, /CORE GRAPH · Core Graph Kernel/, 'the scope line carries the domain tag + name');
  assert.match(md, /state: needs evidence/);
  assert.match(md, /Add a regression test for CSR activation\./, 'the operator message is verbatim');
  assert.match(md, /## Likely files \(13\)/);
  assert.match(md, /\*\*primary \(13\)\*\*/, 'membership is grouped by role with a count');
  assert.match(md, /m1nd-core\/src\/graph\.rs/, 'a real repo-relative member path is present');
  assert.match(md, /## Receipts & evidence state/);
  assert.match(md, /0\/3 required receipt types earned-fresh/, 'the auditable denominator');
  assert.match(md, /## Connected systems/);
  assert.match(md, /Served Owner & Brain Routing/, 'the declared output socket target');
  assert.match(md, /structural neighbours from declared sockets/, 'no invented blast radius');
});

test('COPY LAW: never proven/done/correct, and never an absolute path', () => {
  const md = composePacket({ block, rollup: blockRollup, repoId, message: MSG, toggles: DEFAULT_TOGGLES });
  assert.doesNotMatch(md, /\bproven\b/i);
  assert.doesNotMatch(md, /\bdone\b/i);
  assert.doesNotMatch(md, /\bcorrect\b/i);
  assert.doesNotMatch(md, /\/(Users|home|etc|var|private|root)\//, 'no absolute/home path leaks');
  assert.doesNotMatch(md, /^- `\//m, 'no membership line carries an absolute path');
});

test('screenshot is OFF by default — the clipboard packet is text-only', () => {
  const md = composePacket({ block, rollup: blockRollup, repoId, message: MSG, toggles: DEFAULT_TOGGLES });
  assert.doesNotMatch(md, /screenshot/i);
});

test('an OFF toggle drops its whole section', () => {
  const noFiles = composePacket({
    block,
    rollup: blockRollup,
    repoId,
    message: MSG,
    toggles: { ...DEFAULT_TOGGLES, likelyFiles: false },
  });
  assert.doesNotMatch(noFiles, /## Likely files/);
  assert.match(noFiles, /## Receipts & evidence state/, 'the other sections remain');

  const minimal = composePacket({
    block,
    rollup: blockRollup,
    repoId,
    message: MSG,
    toggles: { ...DEFAULT_TOGGLES, receipts: false, impact: false, blockDetails: false },
  });
  assert.doesNotMatch(minimal, /## Receipts/);
  assert.doesNotMatch(minimal, /## Connected systems/);
  assert.doesNotMatch(minimal, /## Block details/);
  assert.match(minimal, /## Likely files/, 'the still-ON section survives');
});

test('an empty message renders an honest placeholder, never a fabricated ask', () => {
  const md = composePacket({ block, rollup: blockRollup, repoId, message: '', toggles: DEFAULT_TOGGLES });
  assert.match(md, /_\(no message yet\)_/);
});

test('a sub-path scope is carried into the title and the files section', () => {
  const md = composePacket({
    block,
    rollup: blockRollup,
    repoId,
    message: MSG,
    subPath: 'm1nd-core/src/graph.rs',
    toggles: DEFAULT_TOGGLES,
  });
  assert.match(md, /Mission packet — Core Graph Kernel ▸ m1nd-core\/src\/graph\.rs/);
  assert.match(md, /focused on: `m1nd-core\/src\/graph\.rs`/);
});
