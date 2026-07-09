/*
 * The reconcile gesture's view-model (HUMAN-VIEW-V2 F3b §C/§D) — pure, so the
 * honest copy and the conflict/read-only/success flows are proven without a DOM.
 *
 * receiptFreshness mirrors the owner's `receipt_recompute` truth (block | boundary |
 * contract | expired); reconcileSummary is the toast body; reconcileErrorToast maps
 * the owner's REAL error strings (OCC conflict, read-only refusal) onto honest copy;
 * runReconcile reduces a mocked client call to a toast + reload decision.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  receiptFreshness,
  reconcileSummary,
  reconcileErrorToast,
  runReconcile,
  type ReconcileReport,
  type SystemBlocksSnapshot,
} from './buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const snapshot = JSON.parse(
  readFileSync(join(FIX, 'system_blocks_reconciled.json'), 'utf8'),
) as SystemBlocksSnapshot;
const kernel = snapshot.store!.blocks[0]; // boundary_version 2, one stale + one fresh receipt

// ── receiptFreshness — the display freshness axis (F3b §C) ─────────────────────

test('receiptFreshness: a receipt earned against an OLDER boundary is stale by "boundary"', () => {
  const staleTest = kernel.receipts[0]; // scope.boundary_version 1, block is v2
  const f = receiptFreshness(staleTest, kernel);
  assert.equal(f.fresh, false);
  assert.equal(f.fresh === false && f.reason, 'boundary');
});

test('receiptFreshness: a receipt at the CURRENT boundary is fresh', () => {
  const freshStructural = kernel.receipts[1]; // scope.boundary_version 2 === block v2
  assert.deepEqual(receiptFreshness(freshStructural, kernel), { fresh: true });
});

test('receiptFreshness: contract + expired reasons, in the owner\'s order', () => {
  const r = JSON.parse(JSON.stringify(kernel.receipts[1]));
  r.scope.contract_version = 2; // block is contract v1
  assert.equal(receiptFreshness(r, kernel).fresh, false);
  assert.equal(receiptFreshness(r, kernel).fresh === false && receiptFreshness(r, kernel).reason, 'contract');
  const exp = JSON.parse(JSON.stringify(kernel.receipts[1]));
  exp.validity.expires_on = '2000-01-01T00:00:00Z';
  const fe = receiptFreshness(exp, kernel, Date.parse('2026-07-09T00:00:00Z'));
  assert.equal(fe.fresh === false && fe.reason, 'expired');
});

// ── reconcileSummary — the toast body ─────────────────────────────────────────

const baseReport: ReconcileReport = {
  dirty: true,
  blocks: [],
  bumped_block_ids: ['a', 'b'],
  unmapped_total: 5,
  unmapped_materialized: 5,
  store_version: 7,
  file_count: 100,
};

test('reconcileSummary: "N boundaries moved · U unmapped · store vV" (spec example)', () => {
  assert.equal(reconcileSummary(baseReport), '2 boundaries moved · 5 unmapped · store v7');
});

test('reconcileSummary: pluralization is honest (1 boundary, 0 boundaries)', () => {
  assert.match(reconcileSummary({ ...baseReport, bumped_block_ids: ['a'] }), /^1 boundary moved /);
  assert.match(reconcileSummary({ ...baseReport, bumped_block_ids: undefined }), /^0 boundaries moved /);
});

// ── reconcileErrorToast — the owner's real error strings → honest copy ─────────

test('reconcileErrorToast: OCC conflict → the store-moved copy, parsing the actual version', () => {
  // The owner's real SeedError::Conflict Display (system_blocks.rs).
  const err = { detail: 'store version conflict: expected 7, actual 9 — reload and retry (nothing was applied)' };
  const t = reconcileErrorToast(err, 7);
  assert.equal(t.kind, 'conflict');
  assert.equal(t.text, 'the store moved (expected v7, actual v9) — reloading');
});

test('reconcileErrorToast: a read-only owner → the writable-session copy', () => {
  // The owner's real read-only refusal (server.rs).
  const err = {
    detail:
      "m1nd is attached read-only (--read-only); mutation tool 'system_blocks_reconcile' is disabled. Detach or run a read-write instance to modify state.",
  };
  const t = reconcileErrorToast(err, 7);
  assert.equal(t.kind, 'readonly');
  assert.equal(t.text, 'this owner is read-only — reconcile from a writable session');
});

test('reconcileErrorToast: an unclassified error surfaces the owner message verbatim (never swallowed)', () => {
  const t = reconcileErrorToast({ detail: 'no workspace root is bound to this brain' }, 3);
  assert.equal(t.kind, 'error');
  assert.match(t.text, /no workspace root is bound/);
});

// ── runReconcile — the mocked-client flows (F3b §D) ───────────────────────────

test('runReconcile: success → an ok toast (summary) AND a reload (the map re-renders on the new truth)', async () => {
  const { toast, shouldReload } = await runReconcile(async () => baseReport, 6);
  assert.equal(toast.kind, 'ok');
  assert.equal(toast.text, '2 boundaries moved · 5 unmapped · store v7');
  assert.equal(shouldReload, true);
});

test('runReconcile: an OCC conflict renders the conflict toast + reloads, NEVER a silent retry', async () => {
  const conflict = { detail: 'store version conflict: expected 7, actual 9 — reload and retry' };
  const { toast, shouldReload } = await runReconcile(async () => {
    throw conflict;
  }, 7);
  assert.equal(toast.kind, 'conflict');
  assert.match(toast.text, /the store moved \(expected v7, actual v9\) — reloading/);
  assert.equal(shouldReload, true, 'conflict reloads against the fresh version');
});

test('runReconcile: a read-only refusal informs WITHOUT a reload (nothing changed)', async () => {
  const ro = { detail: 'm1nd is attached read-only (--read-only); mutation tool is disabled.' };
  const { toast, shouldReload } = await runReconcile(async () => {
    throw ro;
  }, 7);
  assert.equal(toast.kind, 'readonly');
  assert.equal(shouldReload, false);
});
