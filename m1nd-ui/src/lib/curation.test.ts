/*
 * curation — the F11-c §3a curation-packet compositor + its direct-path dispatch
 * (the heavy-case escape hatch of the friction law §0c). Pure + DOM-free: the
 * compositor mirrors packet.test.ts; the dispatch proof drives the EXISTING
 * `sendDirectPacket` with injected deps and asserts the CURATION shape — a seq-1
 * `judging` letter, capability `hand-runner` (the honest lane intent; the spawn
 * waits for the capability, refused in the MVP), block_id = the skeleton id.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { composeCurationPacket, dispatchCuration } from './curation';
import { brainRefFor, repoIdFromSkeletonId } from './buildMap';
import { sendDirectPacket, type MissionLetter, type PostOutcome } from './missions';
import type { CandidateMeta, SystemBlock, SystemBlockStore } from './buildMap';
import type { CurationSpawnResult } from './candidateEdit';

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

const store: SystemBlockStore = {
  schema: 'm1nd-system-block-store-v0',
  store_version: 7,
  skeleton: {
    skeleton_id: 'sk_demo_candidate',
    version: 1,
    state: 'candidate',
    ratification: { method: '', ratifier: '', ratified_at: '', commit: '' },
  },
  blocks: [
    candBlock('sb_auth', 'Auth', meta({ named_by: 'runner' }), ['auth/a.go', 'shared/hook.go']),
    candBlock('sb_pay', 'Payments', meta({ named_by: 'runner' }), ['billing/b.go', 'shared/hook.go']),
    candBlock('sb_prov', 'guess?', meta({ needs_owner_naming: true }), ['scripts/one.sh']),
  ],
  unmapped_policy: { visible: true, default_action: 'leave_unmapped_until_ratified' },
  unmapped_files: ['ci/build.yml', 'assets/logo.svg'],
  unmapped_total: 23,
};

// ── the compositor (§3a) ──────────────────────────────────────────────────────

test('the curation packet carries the census, every block honestly, the residue, and the note', () => {
  const md = composeCurationPacket({ store, repoId: 'demo', note: 'Merge the thin scripts block.' });
  assert.match(md, /# Curation mission — candidate skeleton of demo/);
  assert.match(md, /store v7 · skeleton `sk_demo_candidate` \(candidate v1\)/);
  assert.match(md, /3 candidate blocks · 1 provisional name · 1 seam member · 23 unmapped files/);
  // Per-block honesty: runner-named vs PROVISIONAL, seam members counted.
  assert.match(md, /`sb_auth` \*\*Auth\*\* · 2 members · named by runner · 1 seam member/);
  assert.match(md, /`sb_prov` \*\*guess\?\*\* · 1 member · PROVISIONAL — needs a name/);
  // The residue sample + the owner's note, verbatim.
  assert.match(md, /## Unmapped residue \(23 total — a sample\)/);
  assert.match(md, /- `ci\/build\.yml`/);
  assert.match(md, /Merge the thin scripts block\./);
});

test('the packet binds the hand to the two laws: candidate_edit-only under OCC, never ratify', () => {
  const md = composeCurationPacket({ store, repoId: 'demo' });
  assert.match(md, /Edit ONLY through the `candidate_edit` verb, under the OCC key/);
  assert.match(md, /You can NEVER ratify \(the owner signs/);
  assert.match(md, /named_by:"runner"/, 'honest provenance is part of the contract');
  assert.match(md, /_\(none — use your judgment, honestly\)_/, 'an absent note is honest, never invented');
});

test('COPY LAW: the packet never claims proven/done/correct and carries no absolute paths', () => {
  const md = composeCurationPacket({ store, repoId: 'demo', note: 'be careful' });
  assert.doesNotMatch(md, /\bproven\b/i);
  assert.doesNotMatch(md, /\bdone\b/i);
  assert.doesNotMatch(md, /\bcorrect\b/i);
  assert.doesNotMatch(md, /\/Users\//);
  assert.doesNotMatch(md, /\/home\//);
});

// ── the dispatch (the EXISTING direct path — delivery is not execution) ───────

test('the curation dispatch is a seq-1 judging letter with hand-runner intent + the packet on the clipboard', async () => {
  const md = composeCurationPacket({ store, repoId: 'demo' });
  const posted: MissionLetter[] = [];
  const clipboard: string[] = [];
  const outcome: PostOutcome = {
    letter_id: 'lt_1',
    mission_id: 'msn_0123456789ab',
    mission_seq: 1,
    deduped: false,
  };
  const res = await sendDirectPacket(
    {
      markdown: md,
      blockId: store.skeleton.skeleton_id,
      brainRef: 'demo',
      seat: 'oracle',
      capability: 'hand-runner',
    },
    {
      postMission: async (letter) => {
        posted.push(letter);
        return outcome;
      },
      writeClipboard: (text) => {
        clipboard.push(text);
      },
      hash: async () => 'abc123',
      now: () => '2026-07-10T00:00:00Z',
      newId: () => 'msn_0123456789ab',
    },
  );
  assert.equal(posted.length, 1, 'the letter posts FIRST');
  const letter = posted[0];
  assert.equal(letter.mission_seq, 1);
  assert.equal(letter.phase, 'judging');
  assert.equal(letter.capability, 'hand-runner', 'the honest lane intent — the spawn waits for the capability');
  assert.equal(letter.block_id, 'sk_demo_candidate', 'the whole-skeleton mission anchors to the skeleton id');
  assert.equal(letter.packet_ref, 'sha256:abc123');
  assert.equal(res.clipboardCopied, true);
  assert.equal(clipboard[0], md, 'the delivery half: the exact packet rides the clipboard');
});

// ── the F12 dispatch decision (spawn when a runner is announced, else DIRECT) ──

test('dispatchCuration SPAWNS when a runner is announced — the owner applies the proposal', async () => {
  let spawnVersion: number | null = null;
  let directCalled = false;
  const spawnResult: CurationSpawnResult = {
    applied: true,
    ops_count: 2,
    store_version: 8,
    report: 'Merged the thin block and named it.',
    mission_id: 'msn_0123456789ab',
    mission_seq: 2,
  };
  const out = await dispatchCuration(
    { runnerAvailable: true, storeVersion: 7 },
    {
      spawn: async (v) => {
        spawnVersion = v;
        return spawnResult;
      },
      direct: async () => {
        directCalled = true;
        return { mission_id: 'x', mission_seq: 1, clipboardCopied: false };
      },
    },
  );
  assert.equal(spawnVersion, 7, 'the spawn is OCC-keyed on the read version');
  assert.equal(directCalled, false, 'the DIRECT path is never touched when a runner is live');
  assert.equal(out.ok, true);
  assert.equal(out.applied, true, 'a spawn success changed the store — the caller reloads');
  assert.match(out.message, /curation applied — 2 ops, store v8 · Merged the thin block/);
});

test('dispatchCuration shows a SPAWN refusal verbatim — never a silent DIRECT', async () => {
  let directCalled = false;
  const out = await dispatchCuration(
    { runnerAvailable: true, storeVersion: 7 },
    {
      spawn: async () => ({
        applied: false,
        ops_count: 0,
        store_version: 7,
        refusal: 'no_hand_runner: no runner daemon announced',
      }),
      direct: async () => {
        directCalled = true;
        return { mission_id: 'x', mission_seq: 1, clipboardCopied: false };
      },
    },
  );
  assert.equal(directCalled, false, 'a refusal is honest, not a fallback');
  assert.equal(out.ok, false);
  assert.equal(out.applied, false, 'nothing applied → no reload');
  assert.match(out.message, /no_hand_runner: no runner daemon announced/);
});

test('dispatchCuration falls back to DIRECT when NO runner is announced (the fallback intact)', async () => {
  let spawnCalled = false;
  const out = await dispatchCuration(
    { runnerAvailable: false, storeVersion: 7 },
    {
      spawn: async () => {
        spawnCalled = true;
        return { applied: false, ops_count: 0, store_version: 7 };
      },
      direct: async () => ({ mission_id: 'msn_0123456789ab', mission_seq: 1, clipboardCopied: true }),
    },
  );
  assert.equal(spawnCalled, false, 'the SPAWN verb is never called without a runner');
  assert.equal(out.ok, true);
  assert.equal(out.applied, false, 'the DIRECT path posts a letter — the store is unchanged');
  assert.match(out.message, /curation letter posted \(msn_0123456789ab, seq 1\)/);
  assert.match(out.message, /the packet is on your clipboard/);
});

test('on a HOSTED brain the curation letter carries the canonical brain_ref: the basename of the brain root', async () => {
  // The field bug (2026-07-10, twice in one dogfood): the map derived brain_ref
  // from the skeleton id — null on the scan's candidate form (so the letter said
  // "brain"), a lowercase sanitized slug otherwise — and the hosted brain's
  // mission_post guard refused with brain_mismatch. The §1f canonical ref is the
  // brain's DISPLAY NAME = the basename of its project root (case-sensitive),
  // derived exactly as BuildMapView.handleSendCuration now does.
  const hostedRoot = '/private/tmp/ws/Repo-B1';
  const hostedStore = {
    ...store,
    skeleton: { ...store.skeleton, skeleton_id: 'sk_repo_b1_candidate' },
  };
  const brainRef = brainRefFor(hostedRoot, repoIdFromSkeletonId(hostedStore.skeleton.skeleton_id));
  assert.equal(brainRef, 'Repo-B1', 'basename of the root, never the sanitized slug');

  const md = composeCurationPacket({ store: hostedStore, repoId: brainRef });
  assert.match(md, /# Curation mission — candidate skeleton of Repo-B1/);

  const posted: MissionLetter[] = [];
  await sendDirectPacket(
    {
      markdown: md,
      blockId: hostedStore.skeleton.skeleton_id,
      brainRef,
      seat: 'oracle',
      capability: 'hand-runner',
    },
    {
      postMission: async (letter) => {
        posted.push(letter);
        return { letter_id: 'lt_2', mission_id: letter.mission_id, mission_seq: 1, deduped: false };
      },
      hash: async () => 'beef',
      now: () => '2026-07-10T00:00:00Z',
      newId: () => 'msn_0123456789ab',
    },
  );
  const letter = posted[0];
  assert.equal(letter.brain_ref, 'Repo-B1', 'the guard identity — basename, case intact (§1f)');
  assert.equal(letter.block_id, 'sk_repo_b1_candidate', 'the skeleton-scoped anchor');
  assert.doesNotMatch(letter.brain_ref, /\//, 'never an absolute path (§1f)');
});
