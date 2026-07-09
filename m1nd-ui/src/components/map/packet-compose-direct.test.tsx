/*
 * PacketCompose · direct mode (HUMAN-VIEW-V2 F2.5 §4). The teeth: `direct` is
 * un-disabled (F2.5b) and its send composes + posts the seq-1 `judging` letter (a
 * mock `mission_post` asserts the body — seq 1, null prev, `msn_<12hex>` id, the
 * packet_ref anchored); the panel DECLARES "delivery is not execution"; policy gating
 * holds (§4d — a read-only owner falls back to clipboard, spawn stays disabled with
 * the amendment's note); a refused post surfaces honestly and NEVER copies as if it
 * had posted; and the clipboard path stays pure (proven in show-code.test.tsx).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import PacketCompose from './PacketCompose';
import { sendDirectPacket, type MissionLetter } from '../../lib/missions';
import { repoIdFromSkeletonId, rollupStore, type SystemBlocksSnapshot } from '../../lib/buildMap';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const snapshot = JSON.parse(
  readFileSync(join(FIX, 'system_blocks_snapshot.json'), 'utf8'),
) as SystemBlocksSnapshot;
const store = snapshot.store!;
const rollup = rollupStore(store);
const repoId = repoIdFromSkeletonId(store.skeleton.skeleton_id);
const block = store.blocks[0];
const blockRollup = rollup.rollups.get(block.block_id)!;
const noop = () => {};

const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (out: string) => decode(out.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

const compose = (props: Partial<React.ComponentProps<typeof PacketCompose>> = {}) =>
  renderToStaticMarkup(
    React.createElement(PacketCompose, {
      block,
      rollup: blockRollup,
      repoId,
      onClose: noop,
      initialMessage: 'Add a test.',
      ...props,
    }),
  );

// ── §4a — direct is live and declares itself ──────────────────────────────────

test('direct mode shows the "Send to agent inbox" action and declares delivery is not execution', () => {
  const out = compose({ initialMode: 'direct' });
  assert.match(out, /data-role="packet-compose"[^>]*data-mode="direct"/);
  assert.match(out, /data-role="send-direct"/);
  const t = visible(out);
  assert.match(t, /Send to agent inbox/);
  assert.match(t, /direct: posted to the agent’s inbox — delivery is not execution/);
});

// ── the seq-1 letter — the mock asserts the body ──────────────────────────────

test('the direct send composes + posts a seq-1 judging letter (mock asserts the body)', async () => {
  const captured: MissionLetter[] = [];
  const copiedTo: string[] = [];
  const res = await sendDirectPacket(
    { markdown: '# Mission packet — Core Graph Kernel', blockId: block.block_id, brainRef: 'm1nd' },
    {
      postMission: async (letter) => {
        captured.push(letter);
        return { letter_id: 'abc123def456', mission_id: letter.mission_id, mission_seq: 1, deduped: false };
      },
      hash: async () => 'cafef00dcafef00d',
      now: () => '2026-07-09T12:00:00Z',
      newId: () => 'msn_0123456789ab',
      writeClipboard: (text) => {
        copiedTo.push(text);
      },
    },
  );

  assert.equal(captured.length, 1, 'exactly one mission_post');
  const letter = captured[0];
  assert.equal(letter.schema, 'm1nd-mission-letter-v0');
  assert.equal(letter.mission_seq, 1, 'seq 1');
  assert.equal(letter.prev_letter_id, undefined, 'null prev on seq-1 (§1e)');
  assert.equal(letter.phase, 'judging');
  assert.equal(letter.seat, 'oracle');
  assert.equal(letter.block_id, block.block_id);
  assert.match(letter.mission_id, /^msn_[0-9a-f]{12}$/);
  assert.equal(letter.packet_ref, 'sha256:cafef00dcafef00d', 'packet_ref = sha256 of the composed markdown');
  assert.equal(letter.brain_ref, 'm1nd', 'a reference string, never an absolute path (§1f)');
  // The packet is copied for the human to paste — the honest MVP delivery.
  assert.equal(res.clipboardCopied, true);
  assert.deepEqual(copiedTo, ['# Mission packet — Core Graph Kernel']);
});

test('a refused post surfaces the error and NEVER copies as if it had posted', async () => {
  const copiedTo: string[] = [];
  await assert.rejects(
    sendDirectPacket(
      { markdown: '# packet', blockId: block.block_id, brainRef: 'm1nd' },
      {
        postMission: async () => {
          throw new Error('stale_head: the head moved under this letter');
        },
        hash: async () => 'deadbeef',
        writeClipboard: (t) => copiedTo.push(t),
      },
    ),
    /stale_head/,
  );
  assert.equal(copiedTo.length, 0, 'a failed post must not copy — the post runs first and throws');
});

// ── §4d — policy gating ───────────────────────────────────────────────────────

test('a read-only owner falls back to clipboard — direct is disabled, and says why', () => {
  const out = compose({ policy: { readOnly: true }, initialMode: 'direct' });
  // Even asked for direct, a read-only owner renders clipboard (§4d).
  assert.match(out, /data-role="packet-compose"[^>]*data-mode="clipboard"/);
  assert.match(out, /data-role="mode-direct"[^>]*disabled=""/);
  assert.match(out, /data-role="copy-packet"/, 'clipboard is the only live action');
  assert.match(visible(out), /read-only/);
});

test('spawn stays disabled with the amendment note (runnerd is F2.5c)', () => {
  const out = compose({ initialMode: 'direct' });
  assert.match(out, /data-role="mode-spawn"[^>]*disabled=""/);
  assert.match(visible(out), /no runner daemon connected/);
});

test('the clipboard default render still declares no engine side effects (pure)', () => {
  const out = compose();
  assert.match(out, /data-role="packet-compose"[^>]*data-mode="clipboard"/);
  assert.match(visible(out), /clipboard mode: no side effects — nothing is written to the engine/);
  // The read-only proof: the clipboard render names no tool route.
  assert.doesNotMatch(out, /\/api\/tools/);
  assert.doesNotMatch(out, /delegate/i);
});
