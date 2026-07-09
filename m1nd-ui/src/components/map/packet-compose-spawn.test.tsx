/*
 * PacketCompose · spawn mode (HUMAN-VIEW-V2 F2.5c §4b). The teeth: `spawn` un-disables
 * ONLY when a runner is live (the status listed pinned-live runners); its send POSTs
 * the composed packet through the owner's `mission_spawn` proxy (a mock asserts the
 * body — runner_id, block_id, brain_ref, the packet markdown) and the toast carries
 * the mission_id ("mission started — watch the tray"); without a runner the radio is
 * disabled with the amendment's exact note ("no runner daemon connected"); a read-only
 * owner falls back to clipboard; and a refused spawn surfaces the daemon's error
 * verbatim, never a fake start.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import PacketCompose from './PacketCompose';
import { sendSpawnPacket, type LiveRunner, type SpawnInput } from '../../lib/missions';
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

const RUNNERS: LiveRunner[] = [
  { runner_id: 'build-runner-local', port: 61999, last_seen_ms: 1 },
  { runner_id: 'naming-runner-local', port: 61999, last_seen_ms: 1 },
];

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

// ── no runner → spawn disabled with the amendment's exact note ────────────────

test('without a live runner, spawn stays disabled with "no runner daemon connected"', () => {
  const out = compose({ initialMode: 'direct' });
  assert.match(out, /data-role="mode-spawn"[^>]*disabled=""/);
  assert.match(visible(out), /no runner daemon connected/);
});

// ── a live runner un-disables spawn + lists the pinned-live runners ───────────

test('a live runner un-disables spawn, renders the runner select, and asks for spawn', () => {
  const out = compose({ liveRunners: RUNNERS, initialMode: 'spawn' });
  assert.match(out, /data-role="packet-compose"[^>]*data-mode="spawn"/);
  // The radio is enabled (no disabled="") and the note reads "runner ready".
  assert.doesNotMatch(out, /data-role="mode-spawn"[^>]*disabled=""/);
  assert.match(out, /data-role="spawn-runner"/);
  assert.match(out, /data-role="send-spawn"/);
  const t = visible(out);
  assert.match(t, /build-runner-local/, 'the runner select lists the pinned-live ids');
  assert.match(t, /naming-runner-local/);
  assert.match(t, /isolated worktree/, 'the honest declaration');
});

// ── the spawn send — the mock asserts the proxy body ──────────────────────────

test('the spawn send posts the packet through the mission_spawn proxy (mock asserts the body)', async () => {
  const captured: SpawnInput[] = [];
  const outcome = await sendSpawnPacket(
    { markdown: '# Mission packet — Core Graph Kernel', blockId: block.block_id, brainRef: 'm1nd', runnerId: 'build-runner-local' },
    {
      spawnMission: async (input) => {
        captured.push(input);
        return { mission_id: 'msn_0123456789ab', accepted: true, runner_id: input.runnerId };
      },
    },
  );

  assert.equal(captured.length, 1, 'exactly one mission_spawn');
  const sent = captured[0];
  assert.equal(sent.runnerId, 'build-runner-local');
  assert.equal(sent.blockId, block.block_id);
  assert.equal(sent.brainRef, 'm1nd', 'a reference string, never an absolute path (§1f)');
  assert.equal(sent.packetMarkdown, '# Mission packet — Core Graph Kernel');
  assert.equal(outcome.mission_id, 'msn_0123456789ab');
  assert.equal(outcome.accepted, true);
});

test('a refused spawn rejects with the daemon keyword verbatim (never a fake start)', async () => {
  await assert.rejects(
    sendSpawnPacket(
      { markdown: '# packet', blockId: block.block_id, brainRef: 'm1nd', runnerId: 'build-runner-local' },
      {
        spawnMission: async () => {
          throw new Error('unpinned_runner: runner is not pinned in runners.toml');
        },
      },
    ),
    /unpinned_runner/,
  );
});

// ── §4d — a read-only owner falls back to clipboard, spawn disabled ───────────

test('a read-only owner cannot spawn — spawn is disabled even with a live runner', () => {
  const out = compose({ policy: { readOnly: true }, liveRunners: RUNNERS, initialMode: 'spawn' });
  // Even asked for spawn, a read-only owner renders clipboard (§4d).
  assert.match(out, /data-role="packet-compose"[^>]*data-mode="clipboard"/);
  assert.match(out, /data-role="mode-spawn"[^>]*disabled=""/);
});
