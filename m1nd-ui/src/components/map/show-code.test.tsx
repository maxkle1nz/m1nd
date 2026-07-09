/*
 * Show Code + Copy Packet — the F2 surfaces rendered against the REAL captured
 * snapshot fixture. renderToStaticMarkup (no clicks, no network — the F1 posture):
 * each tab is opened via `initialTab`. The teeth: Files lists the membership by
 * role; Receipts shows the contract 0/3 with each required type "not earned yet";
 * Impact shows the declared sockets, honestly labelled; the packet panel previews
 * the composed Markdown and DECLARES no side effects; direct/spawn are disabled;
 * and the clipboard path never references delegate or a tool route (read-only).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import ShowCode, { type ShowCodeTab } from './ShowCode';
import PacketCompose from './PacketCompose';
import { repoIdFromSkeletonId, rollupStore, type SystemBlocksSnapshot } from '../../lib/buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const snapshot = JSON.parse(
  readFileSync(join(FIX, 'system_blocks_snapshot.json'), 'utf8'),
) as SystemBlocksSnapshot;
const store = snapshot.store!;
const rollup = rollupStore(store);
const repoId = repoIdFromSkeletonId(store.skeleton.skeleton_id);
const block = store.blocks[0]; // Core Graph Kernel — 13 primary, 0 tests, 1 output socket
const blockRollup = rollup.rollups.get(block.block_id)!;
const noop = () => {};
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const showCode = (tab: ShowCodeTab) =>
  React.createElement(ShowCode, {
    block,
    rollup: blockRollup,
    repoId,
    onClose: noop,
    onAskAgent: noop,
    initialTab: tab,
  });
const text = (el: React.ReactElement) =>
  decode(renderToStaticMarkup(el).replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

test('Files tab lists the block membership grouped by role', () => {
  const out = renderToStaticMarkup(showCode('files'));
  assert.match(out, /data-role="files-by-role"/);
  assert.match(out, /data-role="role-group-primary"/);
  for (const m of block.membership) {
    assert.match(out, new RegExp(`data-path="${esc(m.path)}"`), `member ${m.path} is a selectable path`);
  }
  // No file selected → the viewer is idle (no network in a static render).
  assert.match(out, /data-role="viewer-idle"/);
});

test('a glob member is honestly deferred in the viewer, not fetched into a 404', () => {
  // Block 2 (Ingest) declares a glob member `m1nd-ingest/src/**` — the exact-path
  // viewer cannot read a pattern, so it says so instead of firing a doomed fetch.
  const ingest = store.blocks.find((b) => b.block_id === 'sb_m1nd_ingest')!;
  const ingestRollup = rollup.rollups.get(ingest.block_id)!;
  const glob = ingest.membership.find((m) => m.path.includes('*'))!.path;
  const out = renderToStaticMarkup(
    React.createElement(ShowCode, {
      block: ingest,
      rollup: ingestRollup,
      repoId,
      onClose: noop,
      onAskAgent: noop,
      initialTab: 'files' as ShowCodeTab,
      initialSelectedPath: glob,
    }),
  );
  assert.match(out, /data-role="viewer-glob"/);
  assert.match(out, /glob pattern/);
});

test('Receipts tab shows the contract 0/3 with each required type not earned yet', () => {
  const out = renderToStaticMarkup(showCode('receipts'));
  const t = text(showCode('receipts'));
  assert.match(t, /Receipts 0\/3/);
  assert.match(t, /not earned yet/);
  for (const type of ['spec', 'structural', 'test']) {
    assert.match(out, new RegExp(`>${type}<`), `required type ${type} is named`);
  }
});

test('Impact tab shows the declared sockets, honestly labelled (no invented blast radius)', () => {
  const t = text(showCode('impact'));
  assert.match(t, /Served Owner & Brain Routing/);
  assert.match(t, /structural neighbours from declared sockets/);
});

test('Tests tab is an honest empty state when a block declares no test members', () => {
  const out = renderToStaticMarkup(showCode('tests'));
  assert.match(out, /data-role="tests-empty"/);
});

test('the header offers Copy context / Ask agent; Open in editor is an honest disabled placeholder', () => {
  const out = renderToStaticMarkup(showCode('files'));
  assert.match(out, /data-role="copy-context"/);
  assert.match(out, /data-role="showcode-ask-agent"/);
  assert.match(out, /data-role="open-in-editor"[^>]*disabled=""/);
});

test('COPY LAW: Show Code never says proven/done/correct', () => {
  const t = text(showCode('receipts'));
  assert.doesNotMatch(t, /\bproven\b/i);
  assert.doesNotMatch(t, /\bdone\b/i);
  assert.doesNotMatch(t, /\bcorrect\b/i);
});

// ── Copy Packet — the read-only compositor ──────────────────────────────────

const packet = (message = '') =>
  React.createElement(PacketCompose, {
    block,
    rollup: blockRollup,
    repoId,
    onClose: noop,
    initialMessage: message,
  });

test('the packet panel previews the composed Markdown and declares no side effects', () => {
  const out = renderToStaticMarkup(packet('Add a test.'));
  const t = text(packet('Add a test.'));
  assert.match(out, /data-role="packet-preview"/);
  assert.match(t, /Mission packet — Core Graph Kernel/);
  assert.match(t, /Add a test\./);
  assert.match(t, /no side effects — nothing is written to the engine/);
});

test('clipboard is the default live mode; direct is un-disabled (F2.5b); spawn stays disabled (F2.5c)', () => {
  const out = renderToStaticMarkup(packet());
  assert.match(out, /data-role="mode-clipboard"[^>]*checked/);
  // F2.5b un-disables direct — the radio is now selectable (no disabled attr).
  assert.doesNotMatch(out, /data-role="mode-direct"[^>]*disabled=""/);
  // spawn waits for runnerd (F2.5c) — still disabled, with the amendment's note.
  assert.match(out, /data-role="mode-spawn"[^>]*disabled=""/);
  assert.match(text(packet()), /no runner daemon connected/);
  assert.match(out, /data-role="copy-packet"/);
  // screenshot toggle is present but OFF + disabled (capture + redaction is F2.5c).
  assert.match(out, /data-role="toggle-screenshot"[^>]*disabled=""/);
});

test('READ-ONLY PROOF: the packet path never references delegate or a tool route', () => {
  const out = renderToStaticMarkup(packet('x'));
  assert.doesNotMatch(out, /delegate/i);
  assert.doesNotMatch(out, /\/api\/tools/);
});
