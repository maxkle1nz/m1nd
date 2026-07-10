/*
 * F11-c UI — the Edit Names & Boundaries screen (HUMAN-VIEW-V2 screen book §3;
 * F11-TECH §4). Rendered with react-dom/server (SSR + pure policy — interaction
 * correctness lives in candidateEdit.test.ts). The teeth:
 *  - the drawn two-column anatomy: the block list (inline-editable names,
 *    expandable members with certainty dots + "also claimed by") and the selected
 *    panel (editable purpose, the honest boundary view, seam radios, Split/Merge/
 *    Reset) — implementation invents nothing;
 *  - the unmapped tray assigns residue ("leave unmapped" is the honest default);
 *  - the friction law (§4c): the zero-touch header line; provisional-first;
 *  - "Name with runner" (§2b): a disabled button always says why (§4d);
 *  - the o4 curating banner: live lease → banner; expired → nothing;
 *  - the footer: Ratify all / Ratify selected only / Later, with the honest gate.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import ReviewRatify from './ReviewRatify';
import type { CandidateMeta, SystemBlock, SystemBlockStore } from '../../lib/buildMap';

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

function candBlock(id: string, name: string, m: CandidateMeta, paths: string[]): SystemBlock {
  return {
    block_id: id,
    name,
    purpose: `${name} purpose`,
    kind: 'scanned',
    state: 'candidate',
    boundary_version: 1,
    contract_version: 1,
    membership_source: 'proposed',
    membership: paths.map((path) => ({ path, role: 'primary' as const })),
    sockets: { inputs: [], outputs: [], external: [] },
    receipt_contract: { version: 1, required: [], optional: [], waived: [], declared_by: null, declared_at: null },
    receipts: [],
    layout: { x: null, y: null, locked: false, algorithm_seed: null, version: 1 },
    unmapped_residue: [],
    candidate_meta: m,
  };
}

function candStore(blocks: SystemBlock[], over: Partial<SystemBlockStore> = {}): SystemBlockStore {
  return {
    schema: 'm1nd-system-block-store-v0',
    store_version: 3,
    skeleton: {
      skeleton_id: 'sk_demo_seed_2026_07',
      version: 1,
      state: 'candidate',
      ratification: { method: 'verb', ratifier: '', ratified_at: '', commit: '' },
    },
    blocks,
    unmapped_policy: { visible: true, default_action: 'leave_unmapped_until_ratified' },
    ...over,
  };
}

const auth = candBlock('sb_auth', 'Auth', meta({ named_by: 'runner' }), [
  'auth/middleware.go',
  'auth/session.go',
  'billing/stripe_hook.go',
]);
const payments = candBlock('sb_pay', 'Payments', meta({ named_by: 'runner' }), [
  'billing/charge.go',
  'billing/stripe_hook.go',
]);
const provisional = candBlock('sb_prov', 'guess?', meta({ needs_owner_naming: true }), [
  'scripts/one.sh',
  'scripts/two.sh',
]);

const screen = (store: SystemBlockStore, extra: Record<string, unknown> = {}) => (
  <ReviewRatify store={store} repoId="demo" onApplyOps={noop} onRatifyAll={noop} onClose={noop} {...extra} />
);

// ── the drawn two-column anatomy (screen §3) ──────────────────────────────────

test('the screen renders BOTH drawn columns: the block list and the selected panel', () => {
  const out = html(screen(candStore([auth, payments])));
  assert.match(out, /data-role="block-list"/);
  assert.match(out, /data-role="selected-panel"/);
  assert.match(out, /data-role="review-census"/);
  assert.match(visible(screen(candStore([auth, payments]))), /2 blocks · 1 seam · 0 unmapped/);
});

test('names are inline-editable inputs; the selected panel carries the editable purpose', () => {
  const out = html(screen(candStore([auth, payments]), { initialSelectedId: 'sb_auth' }));
  assert.match(out, /data-role="name-input"[^>]*value="Auth"/);
  assert.match(out, /data-role="purpose-input"/);
  assert.match(visible(screen(candStore([auth, payments]), { initialSelectedId: 'sb_auth' })), /purpose \(editable\)/);
});

test('expanded members carry certainty dots and "also claimed by" on seam members (§3)', () => {
  const store = candStore([auth, payments]);
  const out = html(screen(store, { initialExpanded: true, initialSelectedId: 'sb_auth' }));
  assert.match(out, /data-role="member-row"/);
  assert.match(out, /data-role="member-certainty"/);
  assert.match(out, /data-member-seam="true"/, 'the shared hook is a seam member');
  assert.match(visible(screen(store, { initialExpanded: true, initialSelectedId: 'sb_auth' })), /also claimed by: sb_pay/);
});

test('the selected panel resolves seams with the drawn radios: keep in both / primary per REAL owner', () => {
  const store = candStore([auth, payments]);
  const out = html(screen(store, { initialSelectedId: 'sb_auth' }));
  assert.match(out, /data-role="seam-resolution"/);
  assert.match(out, /data-role="seam-radio" data-choice="both"/);
  assert.match(out, /data-role="seam-radio" data-choice="primary:sb_auth"/);
  assert.match(out, /data-role="seam-radio" data-choice="primary:sb_pay"/);
  assert.match(
    visible(screen(store, { initialSelectedId: 'sb_auth' })),
    /belongs to sb_auth and sb_pay \(membership is many-to-many\)/,
  );
});

test('the honest boundary view names the members the graph pulls in from OUTSIDE the dominant dir', () => {
  const out = visible(screen(candStore([auth, payments]), { initialSelectedId: 'sb_auth' }));
  assert.match(out, /\+ billing\/stripe_hook\.go/);
  assert.match(out, /outside auth/);
});

test('the Split/Merge/Reset actions render; an impossible split is disabled AND says why (§4d)', () => {
  const out = html(screen(candStore([auth, payments]), { initialSelectedId: 'sb_auth' }));
  assert.match(out, /data-role="split-block"[^>]*disabled/);
  assert.match(out, /title="select the members to split out first"/);
  assert.match(out, /data-role="merge-select"/);
  assert.match(out, /data-role="reset-proposal"/);
  // Merge lists the OTHER candidate blocks only.
  assert.match(out, /<option value="sb_pay">Payments<\/option>/);
  assert.doesNotMatch(out, /<option value="sb_auth">/);
});

// ── the unmapped tray (screen §3) ─────────────────────────────────────────────

test('the unmapped tray lists residue with an assign select whose default is "leave unmapped"', () => {
  const store = candStore([auth], {
    unmapped_files: ['scripts/x.sh', 'ci/build.yml'],
    unmapped_total: 23,
  });
  const out = html(screen(store));
  assert.match(out, /data-role="unmapped-tray"/);
  assert.match(out, /data-role="unmapped-row"/);
  assert.match(out, /data-role="assign-select"/);
  const text = visible(screen(store));
  assert.match(text, /unmapped residue \(23 files\)/);
  assert.match(text, /leave unmapped/);
  assert.match(text, /assign to Auth/);
  assert.match(text, /… 21 more \(the count is true\)/, 'the capped tray keeps the honest total');
});

// ── the friction law (§4c) + Name with runner (§2b/§4d) ───────────────────────

test('the zero-touch header line appears when ALL blocks are runner-named', () => {
  const out = html(screen(candStore([auth, payments])));
  assert.match(out, /data-role="zero-touch"/);
  assert.match(visible(screen(candStore([auth, payments]))), /all 2 blocks runner-named — ready to ratify/);
  // Any provisional block silences the line.
  assert.doesNotMatch(html(screen(candStore([auth, provisional]))), /data-role="zero-touch"/);
});

test('"Name with runner" disabled ALWAYS says why; enabled with a live runner + provisional blocks', () => {
  const store = candStore([provisional, auth]);
  const off = html(screen(store, { onNameWithRunner: noop, runnerAvailable: false }));
  assert.match(off, /data-role="name-with-runner"[^>]*disabled/);
  assert.match(off, /no runner daemon connected/, 'the disabled state says why (§4d)');
  const on = html(screen(store, { onNameWithRunner: noop, runnerAvailable: true }));
  assert.doesNotMatch(on, /data-role="name-with-runner"[^>]*disabled=""/);
  assert.match(on, /name the 1 provisional block with the live naming-runner/);
  // The per-block button rides the provisional row.
  assert.match(on, /data-role="name-with-runner-block"/);
});

// ── the o4 curating banner ────────────────────────────────────────────────────

test('a LIVE advisory lease renders the non-blocking banner; an expired one renders nothing', () => {
  const leased = candStore([auth], {
    curating_by: 'hand-7',
    curating_until: '2026-07-10T12:00:00Z',
  });
  const live = screen(leased, { nowIso: '2026-07-10T11:00:00Z' });
  assert.match(html(live), /data-role="curating-banner"/);
  assert.match(visible(live), /a hand is curating candidate v1 \(hand-7, until 2026-07-10T12:00:00Z\) — advisory, never blocking/);
  const expired = screen(leased, { nowIso: '2026-07-10T13:00:00Z' });
  assert.doesNotMatch(html(expired), /data-role="curating-banner"/, 'expired → reclaimable, no banner');
});

// ── the footer (screen §3) ────────────────────────────────────────────────────

test('the footer offers Ratify selected only + Later beside the gated blanket ratify', () => {
  const store = candStore([auth, provisional]);
  const out = html(
    screen(store, { onRatifySelected: noop, initialSelectedId: 'sb_auth' }),
  );
  assert.match(out, /data-role="ratify-selected"/);
  assert.doesNotMatch(
    out,
    /data-role="ratify-selected"[^>]*disabled=""/,
    'a NAMED selected block ratifies alone',
  );
  assert.match(out, /data-role="review-later"/);
  assert.match(out, /data-role="ratify-gate"/, 'the blanket stays gated by the provisional block');

  // A provisional selected block cannot ratify alone — disabled says why.
  const prov = html(
    screen(store, { onRatifySelected: noop, initialSelectedId: 'sb_prov' }),
  );
  assert.match(prov, /data-role="ratify-selected"[^>]*disabled=""/);
  assert.match(prov, /still needs a name/);
});

test('the drawn fine print rides the footer verbatim', () => {
  assert.match(
    visible(screen(candStore([auth]))),
    /Ratifying signs boundaries as v1 in this brain\. Agents and scans will respect them; drift will reopen this screen — scoped to what drifted, never a silent re-cluster\./,
  );
});
