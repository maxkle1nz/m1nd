/*
 * reader/definition — click-to-def from the edges, with honest per-language
 * degradation. The teeth (against the captured wire shape):
 *   - Rust jumps by call-edge (single grounded target);
 *   - an ambiguous same-name receiver → a CANDIDATE list;
 *   - an external/dangling reference → an explicit ABSTAIN (ungrounded);
 *   - TS jumps by def/import, but a call-ONLY reference abstains (calls-not-tracked);
 *   - a fabricated jump is NEVER produced.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { resolveDefinition } from './definition';
import { languageForPath } from './languages';
import type { GraphSnapshot } from '../snapshot';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const snap = JSON.parse(readFileSync(join(FIX, 'reader_snapshot.json'), 'utf8')) as GraphSnapshot;
const RUST = languageForPath('repo-alpha/src/store.rs');
const TS = languageForPath('repo-alpha/ui/panel.ts');

test('Rust: a call-edge resolves to a single grounded definition (a real jump)', () => {
  const r = resolveDefinition(snap, 'file::repo-alpha/src/store.rs::function::open', RUST);
  assert.equal(r.kind, 'target');
  if (r.kind !== 'target') return;
  assert.equal(r.target.path, 'repo-alpha/src/graph.rs');
  assert.equal(r.target.line, 18); // Graph::insert
  assert.equal(r.target.label, 'insert');
});

test('Rust: an ambiguous same-name receiver returns CANDIDATES, not a guess', () => {
  const r = resolveDefinition(snap, 'file::repo-alpha/src/store.rs::function::save', RUST);
  assert.equal(r.kind, 'candidates');
  if (r.kind !== 'candidates') return;
  assert.equal(r.targets.length, 2);
  assert.deepEqual(
    r.targets.map((t) => t.line).sort((a, b) => a - b),
    [18, 39], // Graph::insert @18 and Node::insert @39 — both grounded
  );
});

test('Rust: an external/dangling target ABSTAINS honestly (ungrounded), never a fake jump', () => {
  const r = resolveDefinition(snap, 'file::repo-alpha/src/store.rs::function::close', RUST);
  assert.equal(r.kind, 'abstain');
  if (r.kind !== 'abstain') return;
  assert.equal(r.reason, 'ungrounded');
  assert.match(r.message, /no grounded target/);
});

test('TS: a def/import edge jumps (call edges are not needed for TS)', () => {
  const r = resolveDefinition(snap, 'file::repo-alpha/ui/panel.ts::function::render', TS);
  assert.equal(r.kind, 'target');
  if (r.kind !== 'target') return;
  assert.equal(r.target.path, 'repo-alpha/ui/helpers.ts');
  assert.equal(r.target.line, 3); // format
});

test('TS: a CALL-ONLY reference abstains as calls-not-tracked (the honest PATHOS gap)', () => {
  const r = resolveDefinition(snap, 'file::repo-alpha/ui/panel.ts::function::mount', TS);
  assert.equal(r.kind, 'abstain');
  if (r.kind !== 'abstain') return;
  assert.equal(r.reason, 'calls-not-tracked');
  assert.match(r.message, /call edges are not tracked for this language/);
});

test('the SAME call-only reference WOULD jump under Rust — the degradation is per language', () => {
  // Prove the abstain above is a LANGUAGE policy, not missing data: pretend mount's
  // source language trusts call edges (Rust profile) and the call-edge now resolves.
  const r = resolveDefinition(snap, 'file::repo-alpha/ui/panel.ts::function::mount', languageForPath('x.rs'));
  assert.equal(r.kind, 'target');
  if (r.kind !== 'target') return;
  assert.equal(r.target.label, 'format');
});

test('a symbol with no outgoing reference abstains as "none" (no clutter marker)', () => {
  const r = resolveDefinition(snap, 'file::repo-alpha/src/graph.rs::struct::Node', RUST);
  assert.equal(r.kind, 'abstain');
  if (r.kind !== 'abstain') return;
  assert.equal(r.reason, 'none');
});
