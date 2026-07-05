/*
 * Mailbox render — the caixinha's DOM (HUMAN-LAYER-PRD §4A.11, INV-17/18).
 *
 * The pure MailboxBody is rendered from REAL captured `/api/mailbox` fixtures
 * (neutral repo names — the committed fixture never carries a client project),
 * with react-dom/server. Anti-scope enforced: no compose affordance in the DOM;
 * the box renders ONLY its own brain's letters; every receipt link resolves in-box.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MailboxBody } from './MailboxView';
import BrainCard from './BrainCard';
import type { MailboxResponse } from '../../lib/mailbox';
import type { InstanceRegistryEntry } from '../../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const decode = (s: string) =>
  s
    .replace(/&#x27;/g, "'")
    .replace(/&amp;/g, '&')
    .replace(/&gt;/g, '>')
    .replace(/&lt;/g, '<')
    .replace(/&quot;/g, '"')
    .replace(/&#xE1;|&#225;/g, 'á');

const boxA = load<MailboxResponse>('mailbox_box_a.json');
const boxB = load<MailboxResponse>('mailbox_box_b.json');
const medulla = load<MailboxResponse>('mailbox_medulla.json');
const dangling = load<MailboxResponse>('mailbox_dangling.json');
const noop = () => {};

const render = (data: MailboxResponse, brainRoot: string, name: string | null, extra: object = {}) =>
  renderToStaticMarkup(
    React.createElement(MailboxBody, { brainRoot, displayName: name, onClose: noop, data, ...extra }),
  );

// ── Letters render, grouped by day-chapter, newest chapter first ────────────────
test('§4A.11: letters render in day-chapters (newest first) with the header count line', () => {
  const out = render(boxA, '/path/to/repo-alpha', 'repo-alpha');
  // Two day chapters, both present and labeled by date.
  assert.match(out, /data-day="2026-07-05"/);
  assert.match(out, /data-day="2026-07-04"/);
  // Newest chapter renders BEFORE the older one in the DOM.
  assert.ok(out.indexOf('data-day="2026-07-05"') < out.indexOf('data-day="2026-07-04"'), 'newest day first');
  // The header states the whole truth (external visible, uncounted in "abertas").
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /6 cartas/);
  assert.match(text, /4 abertas/);
  assert.match(text, /1 externa/);
});

// ── Class chips + fate lines are correct ────────────────────────────────────────
test('§4A.11: each letter wears its matte class chip + exactly one fate line', () => {
  const out = render(boxA, '/path/to/repo-alpha', 'repo-alpha');
  // Every letter card carries exactly one class chip and one fate line.
  const cards = out.match(/data-role="letter-card"/g) ?? [];
  const chips = out.match(/data-role="class-chip"/g) ?? [];
  const fates = out.match(/data-role="fate-line"/g) ?? [];
  assert.equal(cards.length, boxA.letters.length);
  assert.equal(chips.length, boxA.letters.length);
  assert.equal(fates.length, boxA.letters.length, 'exactly one fate line per letter');
  // The four fate glyphs all appear: ● wet · ◍ flight · ↳ answered · ◌ external.
  assert.match(out, /data-fate-glyph="●"/);
  assert.match(out, /data-fate-glyph="◍"/);
  assert.match(out, /data-fate-glyph="↳"/);
  assert.match(out, /data-fate-glyph="◌"/);
});

// ── INV-17: only the viewed brain's letters render (never cross-brain) ──────────
test('INV-17: box A DOM contains ZERO box-B letter ids', () => {
  const out = render(boxA, '/path/to/repo-alpha', 'repo-alpha');
  for (const l of boxB.letters) {
    assert.doesNotMatch(out, new RegExp(l.id), `box-B letter ${l.id} must not render in box A`);
  }
  // And every box-A id IS present.
  for (const l of boxA.letters) assert.match(out, new RegExp(l.id));
});

test('INV-17: a dropped wrong-echo response renders the notice, not the letters', () => {
  const out = renderToStaticMarkup(
    React.createElement(MailboxBody, {
      brainRoot: '/path/to/repo-alpha',
      displayName: 'repo-alpha',
      onClose: noop,
      data: null,
      dropped: true,
    }),
  );
  assert.match(out, /data-role="echo-drop-notice"/);
  assert.doesNotMatch(out, /data-role="letter-card"/, 'no letter renders when the echo is dropped');
});

// ── INV-18: receipts are always linked; a dangling link is honest ───────────────
test('INV-18: the receipt renders the down-pointing "responde a carta" line, linked in-box', () => {
  const out = render(boxA, '/path/to/repo-alpha', 'repo-alpha');
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /responde a carta/);
  // The fired bug carries the up-pointing "respondida pela carta" line.
  assert.match(text, /respondida pela carta/);
  // Every fate-link's target id is a letter that renders in THIS box.
  const boxIds = new Set(boxA.letters.map((l) => l.id));
  for (const m of out.matchAll(/data-target-id="([0-9a-f]{12})"/g)) {
    assert.ok(boxIds.has(m[1]), `fate link ${m[1]} resolves to an in-box letter`);
  }
});

test('INV-18: a dangling receipt link renders the honest breakage, never a bare chip', () => {
  const out = render(dangling, '/path/to/repo-gamma', 'repo-gamma');
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /recibo n(ã|&#xE3;)o localizado/);
  assert.match(out, /data-broken="true"/);
  // No fake link is offered for the unresolved receipt.
  assert.doesNotMatch(out, /data-target-id="deadbeef0000"/);
});

// ── The medulla box is its own labeled view ─────────────────────────────────────
test('§4A.11: the medulla box renders under its labeled header, external uncounted', () => {
  const out = render(medulla, 'medulla', 'medulla');
  assert.match(out, /data-brain="medulla"/);
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /Medulla — relatos transversais/);
  assert.match(text, /data-role="medulla-note"|transversais/);
  // The external (context7-shaped) letter renders ◌ and is NOT in the abertas count.
  assert.match(out, /data-fate-glyph="◌"/);
  assert.match(text, /1 abertas/); // wet_ink(1) only — external excluded
  assert.match(text, /1 externa/);
});

// ── Anti-scope: read-only, no compose affordance in the DOM ─────────────────────
test('§4A.11 anti-scope: the mailbox has NO compose/editor/reply affordance', () => {
  const out = render(boxA, '/path/to/repo-alpha', 'repo-alpha');
  assert.doesNotMatch(out, /<textarea/i, 'no compose box');
  assert.doesNotMatch(out, /type="text"|type="email"|contenteditable/i, 'no input field');
  assert.doesNotMatch(out, />\s*(Reply|Responder|Compose|Escrever|Comment|Comentar)\s*</i, 'no reply/compose button');
});

// ── The violet quarantine holds in the rendered DOM (external wears stone) ───────
test('§6.2: no iris/violet class appears anywhere in the mailbox DOM', () => {
  for (const [data, root] of [
    [boxA, '/path/to/repo-alpha'],
    [medulla, 'medulla'],
  ] as const) {
    const out = render(data, root, null);
    assert.doesNotMatch(out, /\b(text|bg|border|ring)-iris\b|iris-(veil|wisteria|deep)|#7c3aed|#a78bfa|#ede9fe/i);
  }
});

// ── The D3 entry on the Hall card opens the mailbox (absent-honest) ─────────────
test('§4A.11: the card D3 entry renders "N open" with a count, and is absent without one', () => {
  const base: InstanceRegistryEntry = {
    instance_id: 'i1',
    workspace_root: '/path/to/repo-alpha',
    runtime_root: '/rt',
    graph_source: 'x',
    plasticity_state: 'stable',
    pid: 1,
    started_at_ms: 0,
    last_heartbeat_ms: 0,
    mode: 'serve',
    status: 'ok',
    stale: false,
    conflicts: [],
    brain_kind: 'project',
    display_name: 'repo-alpha',
    project_root: '/path/to/repo-alpha',
    node_count: 100,
    edge_count: 200,
  };
  // With a count AND a handler → the entry renders "N open".
  const withCount = renderToStaticMarkup(
    React.createElement(BrainCard, {
      entry: { ...base, mailbox_open_count: 4 },
      isSelf: false,
      selected: false,
      onSelect: noop,
      onOpen: noop,
      onOpenMailbox: noop,
    }),
  );
  assert.match(withCount, /data-role="mailbox-entry"/);
  assert.match(withCount, /data-open-count="4"/);
  assert.match(decode(withCount.replace(/<[^>]+>/g, ' ')), /4 open/);

  // Absent count → NO entry (never a fabricated zero — INV-10).
  const noBox = renderToStaticMarkup(
    React.createElement(BrainCard, {
      entry: { ...base, mailbox_open_count: null },
      isSelf: false,
      selected: false,
      onSelect: noop,
      onOpen: noop,
      onOpenMailbox: noop,
    }),
  );
  assert.doesNotMatch(noBox, /data-role="mailbox-entry"/, 'no mailbox entry without a count');
  assert.doesNotMatch(noBox, /data-open-count/, 'no fabricated count in the DOM');
});
