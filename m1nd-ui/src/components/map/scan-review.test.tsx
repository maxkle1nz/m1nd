/*
 * F0c UI — the scan gesture, the candidate provisional dress, and the Review-&-ratify
 * walk (HUMAN-VIEW-V2 F0c §5). Rendered with react-dom/server (the suite is SSR + pure
 * policy; interaction correctness lives in candidate.test.ts). The teeth:
 *  - the empty state offers a REAL "Scan this repo" (a write, enabled) while Import
 *    stays the honest disabled CLI-verb affordance;
 *  - a candidate block's card wears the provisional "unnamed, needs you" + a component
 *    confidence chip whose tooltip carries the real scores (never a vibe blend);
 *  - the candidate banner's button is "Review & ratify" (objection 10);
 *  - the walk lists blocks lowest-support first, flags seams, gates "Ratify all" behind
 *    every-name-accepted + no-unresolved-seam, and honestly defers rename/seam editing;
 *  - the copy law holds (no "proven/done/correct").
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BuildMap from './BuildMap';
import BlockCard from './BlockCard';
import ReviewRatify from './ReviewRatify';
import {
  rollupStore,
  domainTag,
  type CandidateMeta,
  type SystemBlock,
  type SystemBlockStore,
  type SystemBlocksSnapshot,
} from '../../lib/buildMap';

const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');
const noop = () => {};

// ── factories ─────────────────────────────────────────────────────────────────

function meta(over: Partial<CandidateMeta> = {}): CandidateMeta {
  return {
    named_by: 'heuristic',
    needs_owner_naming: false,
    edge_sample_size: 40,
    directory_support: 0.9,
    coverage_ratio: 0.9,
    shared_member_count: 0,
    ...over,
  };
}

function candBlock(id: string, name: string, m: CandidateMeta, members = 3): SystemBlock {
  return {
    block_id: id,
    name,
    purpose: `${name} purpose`,
    kind: 'scanned',
    state: 'candidate',
    boundary_version: 1,
    contract_version: 1,
    membership_source: 'proposed',
    membership: Array.from({ length: members }, (_, i) => ({ path: `${id}/f${i}.rs`, role: 'primary' as const })),
    sockets: { inputs: [], outputs: [], external: [] },
    receipt_contract: { version: 1, required: [], optional: [], waived: [], declared_by: null, declared_at: null },
    receipts: [],
    layout: { x: null, y: null, locked: false, algorithm_seed: null, version: 1 },
    unmapped_residue: [],
    candidate_meta: m,
  };
}

function candStore(blocks: SystemBlock[]): SystemBlockStore {
  return {
    schema: 'm1nd-system-block-store-v0',
    store_version: 2,
    skeleton: {
      skeleton_id: 'sk_demo_seed_2026_07',
      version: 1,
      state: 'candidate',
      ratification: { method: 'verb', ratifier: '', ratified_at: '', commit: '' },
    },
    blocks,
    unmapped_policy: { visible: true, default_action: 'leave_unmapped_until_ratified' },
  };
}

const low = candBlock('sb_low', 'Low', meta({ directory_support: 0.4, coverage_ratio: 0.4, graph_cohesion: 0.4, needs_owner_naming: true }));
const mid = candBlock('sb_mid', 'Mid', meta({ directory_support: 0.7, coverage_ratio: 0.7, graph_cohesion: 0.7 }));
const high = candBlock('sb_high', 'High', meta({ directory_support: 0.97, coverage_ratio: 0.97, graph_cohesion: 0.97 }));
const seam = candBlock('sb_seam', 'Seam', meta({ shared_member_count: 2 }));

const emptySnap: SystemBlocksSnapshot = { present: false, honest: 'no skeleton yet — import a seed or run a scan' };

// ── the empty-state Scan gesture (§5) ─────────────────────────────────────────

test('§5: the empty state offers a REAL "Scan this repo" button (a write, enabled)', () => {
  const out = html(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} />);
  assert.match(out, /data-role="scan-repo"/);
  // the real `disabled=""` attribute is absent (the Tailwind `disabled:` variant
  // class is not the attribute) — the scan is a live write, not a dead button.
  assert.doesNotMatch(out, /data-role="scan-repo"[^>]*disabled=""/, 'the scan is a real write, not a dead button');
  assert.match(visible(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} />), /auto-clustering only ever produces a candidate/);
});

test('the Scan button locks while a scan is in flight', () => {
  const out = html(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} scanning />);
  assert.match(out, /data-role="scan-repo"[^>]*disabled=""/);
  assert.match(visible(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} scanning />), /Scanning…/);
});

test('Import stays the honest DISABLED CLI-verb affordance alongside the live scan', () => {
  const out = html(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} />);
  assert.match(out, /data-role="import-seed"[^>]*disabled/);
  assert.match(visible(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} />), /system_blocks_seed_import verb/);
});

test('a read-only scan refusal surfaces its honest toast in the empty state', () => {
  const out = html(
    <BuildMap
      snapshot={emptySnap}
      rollup={null}
      onScan={noop}
      scanToast={{ kind: 'readonly', text: 'this owner is read-only — scan from a writable session' }}
    />,
  );
  assert.match(out, /data-role="scan-toast"/);
  assert.match(visible(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} scanToast={{ kind: 'readonly', text: 'this owner is read-only — scan from a writable session' }} />), /read-only/);
});

// ── the candidate provisional dress (BlockCard, §3b/§5) ───────────────────────

test('a candidate block whose name needs the owner renders muted "unnamed, needs you"', () => {
  const store = candStore([low]);
  const rollup = rollupStore(store).rollups.get('sb_low')!;
  const out = html(<BlockCard block={low} rollup={rollup} domainTag={domainTag('sb_low', 'demo')} selected={false} onSelect={noop} />);
  assert.match(out, /data-role="provisional-name"/);
  assert.match(visible(<BlockCard block={low} rollup={rollup} domainTag="LOW" selected={false} onSelect={noop} />), /unnamed, needs you/);
});

test('the confidence chip shows a summary AND carries the COMPONENT scores in its tooltip (§5)', () => {
  const store = candStore([mid]);
  const rollup = rollupStore(store).rollups.get('sb_mid')!;
  const out = html(<BlockCard block={mid} rollup={rollup} domainTag="MID" selected={false} onSelect={noop} />);
  assert.match(out, /data-role="confidence-chip"/);
  // the tooltip (title) breaks the score into its honest components
  assert.match(out, /title="[^"]*cohesion 70%[^"]*directory 70%[^"]*coverage 70%[^"]*named by heuristic/);
  assert.match(visible(<BlockCard block={mid} rollup={rollup} domainTag="MID" selected={false} onSelect={noop} />), /conf 70%/);
});

test('a seam block flags the multi-owner seam on its chip', () => {
  const store = candStore([seam]);
  const rollup = rollupStore(store).rollups.get('sb_seam')!;
  const out = html(<BlockCard block={seam} rollup={rollup} domainTag="SEAM" selected={false} onSelect={noop} />);
  assert.match(out, /data-role="chip-seam"/);
  assert.match(visible(<BlockCard block={seam} rollup={rollup} domainTag="SEAM" selected={false} onSelect={noop} />), /2 seam/);
});

// ── the candidate banner's Review button (objection 10) ───────────────────────

test('objection 10: the candidate banner offers "Review & ratify", not a blanket ratify', () => {
  const store = candStore([mid]);
  const snap: SystemBlocksSnapshot = { present: true, store_version: 2, block_count: 1, store };
  const rollup = rollupStore(store);
  const out = html(<BuildMap snapshot={snap} rollup={rollup} onReview={noop} />);
  assert.match(out, /data-role="candidate-banner"/);
  assert.match(out, /data-role="review-ratify-open"/);
  assert.match(visible(<BuildMap snapshot={snap} rollup={rollup} onReview={noop} />), /Review & ratify/);
});

// ── the Review-&-ratify walk (§5) ─────────────────────────────────────────────

const store3 = candStore([high, low, mid]);

test('the walk lists candidate blocks LOWEST-SUPPORT first (§3b)', () => {
  const out = html(<ReviewRatify store={store3} repoId="demo" acceptedIds={new Set()} onAccept={noop} onRatifyAll={noop} onClose={noop} />);
  const order = [...out.matchAll(/data-role="review-block" data-block-id="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(order, ['sb_low', 'sb_mid', 'sb_high']);
});

test('the walk shows the provisional treatment + the component confidence per block', () => {
  const out = html(<ReviewRatify store={store3} repoId="demo" acceptedIds={new Set()} onAccept={noop} onRatifyAll={noop} onClose={noop} />);
  assert.match(out, /data-role="provisional-name"/);
  assert.match(out, /data-role="confidence"/);
  assert.match(visible(<ReviewRatify store={store3} repoId="demo" acceptedIds={new Set()} onAccept={noop} onRatifyAll={noop} onClose={noop} />), /named by heuristic/);
});

test('"Ratify all" is NOT offered while names are unaccepted — the honest gate reason shows instead', () => {
  const out = html(<ReviewRatify store={store3} repoId="demo" acceptedIds={new Set()} onAccept={noop} onRatifyAll={noop} onClose={noop} />);
  assert.doesNotMatch(out, /data-role="ratify-all"/, 'no blanket gesture over unreviewed guesses');
  assert.match(out, /data-role="ratify-gate"/);
  assert.match(visible(<ReviewRatify store={store3} repoId="demo" acceptedIds={new Set()} onAccept={noop} onRatifyAll={noop} onClose={noop} />), /accept 3 more names/);
});

test('"Ratify all → v1" appears ONLY when every block is accepted AND no seam is unresolved (§5)', () => {
  const accepted = new Set(['sb_low', 'sb_mid', 'sb_high']);
  const out = html(<ReviewRatify store={store3} repoId="demo" acceptedIds={accepted} onAccept={noop} onRatifyAll={noop} onClose={noop} />);
  assert.match(out, /data-role="ratify-all"/);
  assert.match(visible(<ReviewRatify store={store3} repoId="demo" acceptedIds={accepted} onAccept={noop} onRatifyAll={noop} onClose={noop} />), /Ratify all 3 → v1/);
});

test('an unresolved seam blocks "Ratify all" even when every name is accepted', () => {
  const store = candStore([low, seam]);
  const accepted = new Set(['sb_low', 'sb_seam']);
  const out = html(<ReviewRatify store={store} repoId="demo" acceptedIds={accepted} onAccept={noop} onRatifyAll={noop} onClose={noop} />);
  assert.doesNotMatch(out, /data-role="ratify-all"/);
  assert.match(out, /data-role="seam-warning"/);
  assert.match(visible(<ReviewRatify store={store} repoId="demo" acceptedIds={accepted} onAccept={noop} onRatifyAll={noop} onClose={noop} />), /unresolved seam/);
});

test('the accepted state is reflected + renaming/seam editing is honestly deferred (never faked)', () => {
  const accepted = new Set(['sb_mid']);
  const out = html(<ReviewRatify store={candStore([mid])} repoId="demo" acceptedIds={accepted} onAccept={noop} onRatifyAll={noop} onClose={noop} />);
  assert.match(out, /data-role="review-block" data-block-id="sb_mid" data-accepted="true"/);
  assert.match(out, /data-role="edit-names"[^>]*disabled/, 'the full editor is a later slice — shown, never a dead-but-live button');
  assert.match(out, /data-role="deferred-editor"/);
});

test('the review_limit bounds the queue page with an honest overflow total (§7)', () => {
  const many = Array.from({ length: 6 }, (_, i) =>
    candBlock(`sb_${String(i).padStart(2, '0')}`, `B${i}`, meta({ directory_support: i / 6, coverage_ratio: i / 6 })),
  );
  const out = html(<ReviewRatify store={candStore(many)} repoId="demo" reviewLimit={2} acceptedIds={new Set()} onAccept={noop} onRatifyAll={noop} onClose={noop} />);
  assert.match(out, /data-role="review-overflow"/);
  assert.match(visible(<ReviewRatify store={candStore(many)} repoId="demo" reviewLimit={2} acceptedIds={new Set()} onAccept={noop} onRatifyAll={noop} onClose={noop} />), /showing 2 of 6/);
});

// ── the copy law (PRD §13) ────────────────────────────────────────────────────

test('COPY LAW holds across the scan/review surfaces (no proven/done/correct)', () => {
  const surfaces = [
    html(<BuildMap snapshot={emptySnap} rollup={null} onScan={noop} />),
    html(<ReviewRatify store={store3} repoId="demo" acceptedIds={new Set(['sb_low', 'sb_mid', 'sb_high'])} onAccept={noop} onRatifyAll={noop} onClose={noop} />),
  ].map((s) => s.replace(/<[^>]+>/g, ' '));
  for (const text of surfaces) {
    assert.doesNotMatch(text, /\bproven\b/i);
    assert.doesNotMatch(text, /\bdone\b/i);
    assert.doesNotMatch(text, /\bcorrect\b/i);
  }
});
