/*
 * Reconcile UI render — honesty at the pixel boundary (HUMAN-VIEW-V2 F3b).
 * Rendered with react-dom/server against the reconciled fixture
 * (system_blocks_reconciled.json): a store bumped to boundary v2 with one stale +
 * one fresh receipt, seven real unmapped files (three materialized).
 *
 * The acceptance teeth: the Unmapped tray shows the REAL total and expands the
 * list with an honest cap note; a never-reconciled store says so (no false zero);
 * a reconciled-but-empty store shows an honest "0 files"; the boundary-moved card
 * wears the badge and the panel marks the stale receipt with its reason while the
 * fresh one is NOT marked; the Reconcile button exists; a Conflict toast renders;
 * and the copy law holds (no "proven/done/correct").
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BuildMap from './BuildMap';
import { rollupStore, type ReconcileToast, type SystemBlocksSnapshot } from '../../lib/buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');
const clone = (s: SystemBlocksSnapshot): SystemBlocksSnapshot => JSON.parse(JSON.stringify(s));

const reconciled = load<SystemBlocksSnapshot>('system_blocks_reconciled.json');
const rollup = rollupStore(reconciled.store!);
const KERNEL = 'sb_m1nd_core_graph_kernel';

const base = load<SystemBlocksSnapshot>('system_blocks_snapshot.json');
const baseRollup = rollupStore(base.store!);

// ── B) The real Unmapped tray ─────────────────────────────────────────────────

test('the tray shows the REAL unmapped_total (7 files), not the declared residue', () => {
  const text = visibleText(<BuildMap snapshot={reconciled} rollup={rollup} />);
  assert.match(text, /7 files/, 'the tray reads the store\'s unmapped_total');
});

test('the tray expands the materialized list with an honest cap note (showing 3 of 7)', () => {
  const out = html(<BuildMap snapshot={reconciled} rollup={rollup} initialUnmappedExpanded />);
  assert.match(out, /data-role="unmapped-toggle"/, 'the count is a toggle');
  assert.match(out, /data-role="unmapped-list"/, 'the list is expanded');
  const text = visibleText(<BuildMap snapshot={reconciled} rollup={rollup} initialUnmappedExpanded />);
  assert.match(text, /scratchpad\/tmp_probe\.rs/, 'a real unmapped file is listed');
  assert.match(text, /showing 3 of 7 — the cap is honest, the count is true/);
});

test('a never-reconciled store says so honestly — no false zero (F3b §B)', () => {
  const out = html(<BuildMap snapshot={base} rollup={baseRollup} />);
  assert.match(out, /data-role="unmapped-tray"/, 'the tray never disappears (F7)');
  assert.match(out, /data-role="unmapped-unreconciled"/);
  const text = visibleText(<BuildMap snapshot={base} rollup={baseRollup} />);
  assert.match(text, /not reconciled yet — run reconcile/);
  assert.doesNotMatch(text, /\b0 files\b/, 'never a fabricated zero');
});

test('a reconciled store with zero unmapped shows an honest "0 files" (F7 zero-state)', () => {
  const zero = clone(reconciled);
  zero.store!.unmapped_total = 0;
  zero.store!.unmapped_files = [];
  const zeroRollup = rollupStore(zero.store!);
  const out = html(<BuildMap snapshot={zero} rollup={zeroRollup} />);
  assert.match(out, /data-role="unmapped-tray"/);
  assert.doesNotMatch(out, /data-role="unmapped-unreconciled"/, 'it HAS been reconciled — zero is real here');
  assert.match(visibleText(<BuildMap snapshot={zero} rollup={zeroRollup} />), /0 files/);
});

// ── C) The boundary badge on the card ─────────────────────────────────────────

test('only the boundary-moved block wears the ⚠ badge; a fresh block does not', () => {
  const out = html(<BuildMap snapshot={reconciled} rollup={rollup} />);
  assert.equal((out.match(/data-role="boundary-badge"/g) ?? []).length, 1, 'exactly one card is boundary-stale');
  assert.match(visibleText(<BuildMap snapshot={reconciled} rollup={rollup} />), /boundary v2/);
});

test('the base (un-reconciled) map wears NO boundary badge (no fingerprint → neutral)', () => {
  const out = html(<BuildMap snapshot={base} rollup={baseRollup} />);
  assert.doesNotMatch(out, /data-role="boundary-badge"/);
});

// ── C) The panel marks the stale receipt; the fresh one is NOT marked ─────────

test('the panel flies the honest boundary-changed note above the evidence', () => {
  const out = html(<BuildMap snapshot={reconciled} rollup={rollup} initialSelectedId={KERNEL} />);
  assert.match(out, /data-role="boundary-stale-note"/);
  const text = visibleText(<BuildMap snapshot={reconciled} rollup={rollup} initialSelectedId={KERNEL} />);
  assert.match(text, /boundary changed — evidence below predates the current membership/);
});

test('the stale receipt is marked with its reason; the fresh receipt is NOT marked stale', () => {
  const out = html(<BuildMap snapshot={reconciled} rollup={rollup} initialSelectedId={KERNEL} />);
  // Two evidence rows: the test (stale, boundary v1<v2) and the structural (fresh).
  assert.equal((out.match(/data-role="evidence-receipt"/g) ?? []).length, 2);
  assert.match(out, /data-role="evidence-receipt" data-stale="true"/);
  assert.match(out, /data-role="evidence-receipt" data-stale="false"/);
  const text = visibleText(<BuildMap snapshot={reconciled} rollup={rollup} initialSelectedId={KERNEL} />);
  assert.match(text, /— stale \(boundary v1 < v2\)/, 'the stale receipt names both boundaries');
  assert.match(text, /fresh/, 'the current-boundary receipt reads fresh, unmarked');
});

// ── D) The reconcile gesture ──────────────────────────────────────────────────

test('the Reconcile button exists when a handler is wired, and locks while running', () => {
  const out = html(<BuildMap snapshot={reconciled} rollup={rollup} onReconcile={() => {}} />);
  assert.match(out, /data-role="reconcile"/);
  // `disabled=""` is the real attribute; the Tailwind `disabled:` classes are not it.
  assert.doesNotMatch(out, /data-role="reconcile"[^>]*disabled=""/, 'idle button is active');
  const running = html(<BuildMap snapshot={reconciled} rollup={rollup} onReconcile={() => {}} reconciling />);
  assert.match(running, /data-role="reconcile"[^>]*disabled=""/, 'a reconcile in flight locks the button');
  assert.match(visibleText(<BuildMap snapshot={reconciled} rollup={rollup} onReconcile={() => {}} reconciling />), /Reconciling…/);
});

test('a Conflict toast renders honestly (OCC — the store moved, reloading)', () => {
  const toast: ReconcileToast = { kind: 'conflict', text: 'the store moved (expected v7, actual v9) — reloading' };
  const out = html(<BuildMap snapshot={reconciled} rollup={rollup} onReconcile={() => {}} reconcileToast={toast} />);
  assert.match(out, /data-role="reconcile-toast"/);
  assert.match(out, /data-toast-kind="conflict"/);
  assert.match(visibleText(<BuildMap snapshot={reconciled} rollup={rollup} reconcileToast={toast} />), /the store moved \(expected v7, actual v9\) — reloading/);
});

test('a read-only toast keeps the button VISIBLE (it never vanishes silently)', () => {
  const toast: ReconcileToast = { kind: 'readonly', text: 'this owner is read-only — reconcile from a writable session' };
  const out = html(<BuildMap snapshot={reconciled} rollup={rollup} onReconcile={() => {}} reconcileToast={toast} />);
  assert.match(out, /data-role="reconcile"/, 'the button stays — informative, not hidden');
  assert.match(out, /data-toast-kind="readonly"/);
});

// ── Copy law ──────────────────────────────────────────────────────────────────

test('COPY LAW: no "proven/done/correct" anywhere on the reconciled map + panel', () => {
  const text = decode(
    html(<BuildMap snapshot={reconciled} rollup={rollup} initialSelectedId={KERNEL} initialUnmappedExpanded />).replace(
      /<[^>]+>/g,
      ' ',
    ),
  );
  assert.doesNotMatch(text, /\bproven\b/i);
  assert.doesNotMatch(text, /\bdone\b/i);
  assert.doesNotMatch(text, /\bcorrect\b/i);
});
