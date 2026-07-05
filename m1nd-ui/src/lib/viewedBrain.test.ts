/*
 * viewedBrain — the per-brain Open scope + INV-15 drop (HUMAN-LAYER-PRD §4A.9).
 *
 * The tree renders a brain's nodes ONLY under that brain's chip. servedBrainMatches
 * is the pure hinge: given the brain we OPENED and the `served_brain` echo the
 * owner returned, decide render (match) vs drop (mismatch). Uses the REAL two-brain
 * fixture roots (bound m1nd + hosted project-b) so the invariant is pinned to
 * the same data the Hall renders.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  BOUND_VIEW,
  brainSelectorFor,
  canonRootForCompare,
  servedBrainMatches,
  type ViewedBrain,
} from './viewedBrain';
import type { InstanceListResponse } from '../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const list: InstanceListResponse = JSON.parse(readFileSync(join(FIX, 'instances.json'), 'utf8'));
const BOUND_ROOT = list.instances.find((e) => e.brain_kind == null)!.project_root!;
const HOSTED_ROOT = list.instances.find((e) => e.brain_kind === 'project')!.project_root!;

const hostedView: ViewedBrain = { root: HOSTED_ROOT, displayName: 'project-b', nodeCount: 2089 };

// ── The selector the fetch carries ────────────────────────────────────────────

test('the bound view sends NO selector (byte-compatible with pre-2H)', () => {
  assert.equal(brainSelectorFor(BOUND_VIEW), undefined);
  assert.equal(brainSelectorFor(null), undefined);
});

test('a hosted view sends its root as the ?brain= selector', () => {
  assert.equal(brainSelectorFor(hostedView), HOSTED_ROOT);
});

// ── INV-15: render the right brain, drop a mismatch ───────────────────────────

test('INV-15: a hosted view accepts an echo naming its OWN root', () => {
  assert.equal(servedBrainMatches(hostedView, { project_root: HOSTED_ROOT }), true);
});

test('INV-15: a hosted view DROPS an echo naming a DIFFERENT brain (the bound graph)', () => {
  // The owner served the bound graph under the hosted request — the exact bug the
  // echo exists to catch. Never render one brain's nodes under another's chip.
  assert.equal(servedBrainMatches(hostedView, { project_root: BOUND_ROOT }), false);
});

test('INV-15: a hosted view DROPS an ABSENT echo (a pre-2H owner ignored the selector)', () => {
  // No echo means the owner answered from the bound graph regardless of ?brain=.
  // For a hosted view that is a mismatch — drop it, do not show the bound map.
  assert.equal(servedBrainMatches(hostedView, undefined), false);
  assert.equal(servedBrainMatches(hostedView, null), false);
  assert.equal(servedBrainMatches(hostedView, { project_root: null }), false);
});

test('INV-15: the bound view accepts an absent echo AND a bound-named echo', () => {
  // The server resolves absent→bound, so the bound view trusts the request.
  assert.equal(servedBrainMatches(BOUND_VIEW, undefined), true);
  assert.equal(servedBrainMatches(BOUND_VIEW, { project_root: BOUND_ROOT }), true);
});

test('INV-15: a canonical alias (trailing slash) still matches', () => {
  assert.equal(servedBrainMatches(hostedView, { project_root: `${HOSTED_ROOT}/` }), true);
});

test('canonRootForCompare strips a trailing slash but keeps a bare root', () => {
  assert.equal(canonRootForCompare(`${HOSTED_ROOT}/`), HOSTED_ROOT);
  assert.equal(canonRootForCompare(HOSTED_ROOT), HOSTED_ROOT);
  assert.equal(canonRootForCompare('  '), null);
  assert.equal(canonRootForCompare(null), null);
});
