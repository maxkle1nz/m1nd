/*
 * Build Map render — honesty at the pixel boundary (HUMAN-VIEW-V2 F1).
 * Rendered with react-dom/server against the REAL captured snapshot fixture
 * (system_blocks_snapshot.json, built from docs/system-blocks/m1nd.seed.v0.json).
 * The acceptance teeth: twelve cards, every one "needs evidence" (no green
 * without a receipt), NO candidate banner on a ratified seed, the panel carries
 * the auditable denominator (0/3 · not earned yet), the Unmapped tray is present
 * even at zero, and the copy law holds (no "proven/done/correct").
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BuildMap from './BuildMap';
import { rollupStore, type SystemBlocksSnapshot } from '../../lib/buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

const snapshot = load<SystemBlocksSnapshot>('system_blocks_snapshot.json');
const store = snapshot.store!;
const rollup = rollupStore(store);
const firstId = store.blocks[0].block_id;

test('renders exactly twelve block cards from the real seed', () => {
  const out = html(<BuildMap snapshot={snapshot} rollup={rollup} />);
  assert.equal((out.match(/data-role="block-card"/g) ?? []).length, 12);
  // every card traces to a real block_id
  for (const b of store.blocks) {
    assert.match(out, new RegExp(`data-block-card="${b.block_id}"`));
  }
});

test('every card reads data-state="needs-evidence" (no green without a receipt)', () => {
  const out = html(<BuildMap snapshot={snapshot} rollup={rollup} />);
  assert.equal((out.match(/data-state="needs-evidence"/g) ?? []).length, 12);
  assert.doesNotMatch(out, /data-state="evidence-backed"/);
});

test('a ratified seed flies NO candidate banner, and its cards are not dashed', () => {
  const out = html(<BuildMap snapshot={snapshot} rollup={rollup} />);
  assert.doesNotMatch(out, /data-role="candidate-banner"/, 'the ratified seed is not a candidate map');
  assert.equal((out.match(/data-candidate="true"/g) ?? []).length, 0);
});

test('the panel opens with the auditable denominator: Receipts 0/3 · not earned yet', () => {
  const out = html(<BuildMap snapshot={snapshot} rollup={rollup} initialSelectedId={firstId} />);
  assert.match(out, /data-role="block-panel"/);
  const text = visibleText(<BuildMap snapshot={snapshot} rollup={rollup} initialSelectedId={firstId} />);
  assert.match(text, /Receipts 0\/3/, 'block 0 declares spec+structural+test = 3 required');
  assert.match(text, /not earned yet/, 'each required receipt is honestly not earned yet');
  // Show Code / Ask agent are now LIVE (F2) — present and no longer disabled.
  assert.match(out, /data-role="show-code"/);
  assert.match(out, /data-role="ask-agent"/);
  assert.doesNotMatch(out, /data-role="show-code"[^>]*disabled=""/, 'Show Code is active in F2');
  assert.doesNotMatch(out, /data-role="ask-agent"[^>]*disabled=""/, 'Ask agent is active in F2');
});

test('the Unmapped tray is present even before reconcile (F7 — never pretend full coverage)', () => {
  const out = html(<BuildMap snapshot={snapshot} rollup={rollup} />);
  assert.match(out, /data-role="unmapped-tray"/);
  // The real seed has never been reconciled (no block carries a fingerprint), so the
  // tray tells the honest truth — a neutral absence, NOT a false "0 files" (F3b §B).
  const trayText = visibleText(<BuildMap snapshot={snapshot} rollup={rollup} />);
  assert.match(trayText, /not reconciled yet — run reconcile/);
  assert.doesNotMatch(trayText, /0 files/, 'a never-reconciled store never fakes a zero');
});

test('the System Health counts render needs=12, others 0; runtime says "no signal"', () => {
  const text = visibleText(<BuildMap snapshot={snapshot} rollup={rollup} />);
  assert.match(text, /Needs evidence 12/);
  assert.match(text, /Evidence-backed 0/);
  assert.match(text, /Broken 0/);
  assert.match(text, /no signal/, 'the runtime axis is honest — no fabricated blue');
});

test('the wires layer + deterministic canvas render', () => {
  const out = html(<BuildMap snapshot={snapshot} rollup={rollup} />);
  assert.match(out, /data-role="wires"/);
  assert.match(out, /data-role="map-canvas"/);
});

test('COPY LAW: no "proven"/"done"/"correct" as a state, anywhere on the map', () => {
  const out = html(<BuildMap snapshot={snapshot} rollup={rollup} initialSelectedId={firstId} />);
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.doesNotMatch(text, /\bproven\b/i);
  assert.doesNotMatch(text, /\bdone\b/i);
  assert.doesNotMatch(text, /\bcorrect\b/i);
});

// ── First-run (§1.2): a candidate skeleton is visually distinct ───────────────
test('§1.2: a candidate skeleton flies the banner and dashes every card', () => {
  const cand = JSON.parse(JSON.stringify(snapshot)) as SystemBlocksSnapshot;
  cand.store!.skeleton.state = 'candidate';
  for (const b of cand.store!.blocks) b.state = 'candidate';
  const candRollup = rollupStore(cand.store!);
  const out = html(<BuildMap snapshot={cand} rollup={candRollup} />);
  assert.match(out, /data-role="candidate-banner"/, 'nothing on this map is ratified yet');
  assert.match(visibleText(<BuildMap snapshot={cand} rollup={candRollup} />), /nothing on this map is ratified yet/);
  assert.equal((out.match(/data-candidate="true"/g) ?? []).length, 12, 'every card is a candidate (dashed)');
});
