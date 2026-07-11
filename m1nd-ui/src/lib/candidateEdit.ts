/*
 * candidateEdit — the PURE policy behind Edit Names & Boundaries (HUMAN-VIEW-V2
 * F11-c; screen book §3; F11-TECH §4).
 *
 * Every screen gesture COMPILES to `candidate_edit` ops, batched per gesture
 * (§4b): a rename commit is one op, a seam radio is one op, an assign is one op,
 * a merge is one op, a split is one op with EXPLICIT path groups built from the
 * member selection (the server validates disjoint/total — o3). These compilers
 * are pure and DOM-free: the component calls them; the write owner posts the
 * batch under the OCC key it read.
 *
 * The friction law (§0/§4c) lives here too: provisional blocks first, the
 * zero-touch header line, and the ratify gate that no longer demands a manual
 * accept for runner-named blocks (0b — the o6 server gate is the law; this is
 * its honest client mirror).
 */
import type { SystemBlock, SystemBlockStore, WriteToast } from './buildMap';
import { blockSupport, unresolvedSeamCount, writeErrorToast } from './buildMap';

// ---------------------------------------------------------------------------
// Wire types — the `candidate_edit` / `candidate_naming` shapes (Rust serde).
// ---------------------------------------------------------------------------

/** One typed edit op — the Rust `EditOp` tagged union, verbatim on the wire. */
export type EditOpInput =
  | { op: 'rename'; block_id: string; name?: string; purpose?: string }
  | { op: 'merge'; into: string; block_ids: string[] }
  | { op: 'split'; block_id: string; by: { paths: string[][] } }
  | { op: 'move_member'; path: string; from: string; to: string }
  | { op: 'resolve_seam'; path: string; resolution: string }
  | { op: 'assign_unmapped'; path: string; block_id: string };

/** The `candidate_edit` result envelope (the verb returns the fresh store). */
export interface CandidateEditResult {
  store_version: number;
  block_count: number;
  ops_applied: number;
  store: SystemBlockStore;
}

/** The `candidate_naming` route result (F11-c §2b). `fell_back` is the Rust
 *  `Vec<(String, String)>` — `[block_id, reason]` pairs. `refusal` present =
 *  no naming call could run (the screen says why); partial is normal. */
export interface CandidateNamingResult {
  store_version: number;
  named: string[];
  fell_back: [string, string][];
  refusal?: string;
}

/** The `curation_spawn` result (F12 §3) — the propose-apply outcome. `applied` is
 *  true when the hand's batch landed (an empty proposal applies trivially);
 *  `refusal` present = nothing applied (`no_hand_runner`, `proposal_malformed`,
 *  `batch_refused`) and the screen shows why (a `no_hand_runner` falls back to
 *  DIRECT). `report` is the hand's honest paragraph; `mission_id`/`mission_seq`
 *  name the summary letter the tray watches. */
export interface CurationSpawnResult {
  applied: boolean;
  ops_count: number;
  store_version: number;
  report?: string;
  mission_id?: string;
  mission_seq?: number;
  refusal?: string;
}

/** The `candidate_lease` result (F11-a o4) — advisory, never blocking. */
export interface CandidateLeaseResult {
  state: 'acquired' | 'refreshed' | 'released' | 'already_free';
  curating_by: string | null;
  curating_until: string | null;
  store_version: number;
}

// ---------------------------------------------------------------------------
// Gesture → op compilers (§4b: batched per gesture).
// ---------------------------------------------------------------------------

/** A committed rename (input blur/enter). Returns `null` when nothing changed —
 *  no op is posted for a no-op gesture. Trims; an empty name never compiles (the
 *  server would refuse it; the input keeps the old name instead). */
export function renameOp(
  block: SystemBlock,
  nextName: string,
  nextPurpose?: string,
): EditOpInput | null {
  const name = nextName.trim();
  const purpose = nextPurpose?.trim();
  const nameChanged = name.length > 0 && name !== block.name;
  const purposeChanged = purpose != null && purpose !== block.purpose;
  if (!nameChanged && !purposeChanged) return null;
  return {
    op: 'rename',
    block_id: block.block_id,
    ...(nameChanged ? { name } : {}),
    ...(purposeChanged ? { purpose } : {}),
  };
}

/** The one-click owner adoption of the CURRENT name ("Accept name") — a REAL
 *  rename op with the stored name, so the owner touch lands server-side
 *  (`named_by:owner`, `needs_owner_naming:false`) instead of a client-only flag. */
export function acceptNameOp(block: SystemBlock): EditOpInput {
  return { op: 'rename', block_id: block.block_id, name: block.name };
}

/** A committed purpose edit (textarea blur) — one rename op carrying purpose only. */
export function purposeOp(block: SystemBlock, nextPurpose: string): EditOpInput | null {
  return renameOp(block, block.name, nextPurpose);
}

/** A seam radio choice (§1c): `'both'` keeps the member on every owner as an
 *  acknowledged shared seam; a block id makes that block the primary owner. */
export function seamOp(path: string, choice: 'both' | { primary: string }): EditOpInput {
  return {
    op: 'resolve_seam',
    path,
    resolution: choice === 'both' ? 'both' : `primary:${choice.primary}`,
  };
}

/** The unmapped tray's "assign to block" pick — one op. */
export function assignOp(path: string, blockId: string): EditOpInput {
  return { op: 'assign_unmapped', path, block_id: blockId };
}

/** The "Merge into…" pick — one op absorbing the selected block into the target. */
export function mergeOp(into: string, absorbed: string[]): EditOpInput {
  return { op: 'merge', into, block_ids: absorbed };
}

/** Compile a split from the UI's member selection (o3): the selected paths become
 *  group 1, the REMAINDER becomes group 2 — explicit, disjoint, total (the server
 *  re-validates). Honest refusals: an empty selection or a total selection cannot
 *  split (one side would be an empty block). */
export function splitOpFromSelection(
  block: SystemBlock,
  selectedPaths: ReadonlySet<string>,
): { op: EditOpInput } | { reason: string } {
  const all = block.membership.map((m) => m.path);
  const selected = all.filter((p) => selectedPaths.has(p));
  const remainder = all.filter((p) => !selectedPaths.has(p));
  if (selected.length === 0) {
    return { reason: 'select the members to split out first' };
  }
  if (remainder.length === 0) {
    return { reason: 'a split needs members on BOTH sides — leave something behind' };
  }
  return {
    op: {
      op: 'split',
      block_id: block.block_id,
      by: { paths: [selected, remainder] },
    },
  };
}

// ---------------------------------------------------------------------------
// Seams — the many-to-many members and their owners (screen §3: "also claimed by").
// ---------------------------------------------------------------------------

export interface SeamInfo {
  path: string;
  /** Every block id claiming this path (2+ = a seam), store order. */
  owners: string[];
}

/** The multi-owner seam paths of ONE block: members claimed by 2+ candidate
 *  blocks, plus members the scan marked `role:"shared"` (a surfaced seam whose
 *  other owner may not be materialized as an exact path). Deterministic order. */
export function blockSeams(store: SystemBlockStore, blockId: string): SeamInfo[] {
  const block = store.blocks.find((b) => b.block_id === blockId);
  if (!block) return [];
  const owners = new Map<string, string[]>();
  for (const b of store.blocks) {
    for (const m of b.membership) {
      const list = owners.get(m.path) ?? [];
      list.push(b.block_id);
      owners.set(m.path, list);
    }
  }
  const seams: SeamInfo[] = [];
  for (const m of block.membership) {
    const claim = owners.get(m.path) ?? [];
    if (claim.length > 1 || m.role === 'shared') {
      seams.push({ path: m.path, owners: claim });
    }
  }
  return seams;
}

// ---------------------------------------------------------------------------
// The friction law (§0/§4c): ordering, the zero-touch line, the ratify gate.
// ---------------------------------------------------------------------------

/** Candidate blocks, PROVISIONAL FIRST (§4c: "provisional ones surface first"),
 *  then lowest support first within each half (stable block_id tie-break). */
export function provisionalFirstQueue(store: SystemBlockStore): SystemBlock[] {
  const candidates = store.blocks.filter((b) => b.state === 'candidate');
  return [...candidates].sort((a, b) => {
    const na = a.candidate_meta?.needs_owner_naming === true ? 0 : 1;
    const nb = b.candidate_meta?.needs_owner_naming === true ? 0 : 1;
    if (na !== nb) return na - nb;
    const sa = blockSupport(a);
    const sb = blockSupport(b);
    if (sa !== sb) return sa - sb;
    return a.block_id.localeCompare(b.block_id);
  });
}

/** The §4c zero-touch status line, or `null` while any block still needs a touch.
 *  The all-runner case carries the spec's exact phrasing; a map the owner already
 *  touched (some owner-named) reads "named" instead of "runner-named". */
export function zeroTouchLine(store: SystemBlockStore): string | null {
  const candidates = store.blocks.filter((b) => b.state === 'candidate');
  if (candidates.length === 0) return null;
  if (candidates.some((b) => b.candidate_meta?.needs_owner_naming !== false)) return null;
  const n = candidates.length;
  const allRunner = candidates.every((b) => b.candidate_meta?.named_by === 'runner');
  return allRunner
    ? `all ${n} block${n === 1 ? '' : 's'} runner-named — ready to ratify`
    : `all ${n} block${n === 1 ? '' : 's'} named — ready to ratify`;
}

/** The o4 curating banner: present iff the advisory lease is LIVE (`curating_until`
 *  in the future). An expired lease renders nothing — reclaimable, never a trap. */
export function curatingBanner(
  store: SystemBlockStore,
  nowIso: string,
): { by: string; until: string } | null {
  const by = store.curating_by;
  const until = store.curating_until;
  if (!by || !until) return null;
  return until > nowIso ? { by, until } : null;
}

/** The F11 ratify gate (the honest client mirror of the o6 server gate + the seam
 *  law): blocks still carrying an untouched provisional name gate the blanket
 *  ratify (name them, or run the naming-runner); unresolved seams gate it too.
 *  Runner-named blocks need NO manual acceptance (0b — the friction law). */
export function ratifyGateReasonV2(store: SystemBlockStore): string | null {
  const candidates = store.blocks.filter((b) => b.state === 'candidate');
  if (candidates.length === 0) return 'nothing to ratify — no candidate blocks';
  const needing = candidates.filter(
    (b) => b.candidate_meta?.needs_owner_naming === true,
  ).length;
  if (needing > 0) {
    return `${needing} block${needing === 1 ? '' : 's'} still need${needing === 1 ? 's' : ''} a name — name ${needing === 1 ? 'it' : 'them'} (or run the naming-runner)`;
  }
  const seams = unresolvedSeamCount(store);
  if (seams > 0) {
    return `${seams} unresolved seam${seams === 1 ? '' : 's'} — resolve ${seams === 1 ? 'it' : 'them'} below`;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Write reducers — one gesture batch / one naming call → toast + reload decision.
// ---------------------------------------------------------------------------

/** Run one `candidate_edit` gesture batch and reduce it to a toast + a reload
 *  decision (mirrors `runRatify`): success reloads (the store moved forward), a
 *  conflict reloads (the store moved under us — never a silent merge), a
 *  read-only/error refusal informs without a reload. */
export async function runCandidateEdit(
  editFn: () => Promise<CandidateEditResult>,
  expectedVersion: number,
): Promise<{ toast: WriteToast; shouldReload: boolean }> {
  try {
    const res = await editFn();
    const n = res.ops_applied;
    return {
      toast: {
        kind: 'ok',
        text: `applied ${n} edit${n === 1 ? '' : 's'} → store v${res.store_version}`,
      },
      shouldReload: true,
    };
  } catch (err) {
    const toast = writeErrorToast(err, expectedVersion, 'edit');
    return { toast, shouldReload: toast.kind === 'conflict' };
  }
}

/** Run one `candidate_naming` call and reduce it honestly: named > 0 reloads with
 *  the partial ledger; zero named (incl. an explicit refusal) informs WITHOUT a
 *  reload (nothing changed); a thrown conflict reloads like every OCC conflict. */
export async function runCandidateNaming(
  namingFn: () => Promise<CandidateNamingResult>,
  expectedVersion: number,
): Promise<{ toast: WriteToast; shouldReload: boolean }> {
  try {
    const res = await namingFn();
    if (res.refusal) {
      return { toast: { kind: 'error', text: res.refusal }, shouldReload: false };
    }
    const named = res.named.length;
    const fell = res.fell_back.length;
    if (named === 0) {
      return {
        toast: {
          kind: 'error',
          text: `the naming-runner named 0 blocks${fell > 0 ? ` — ${fell} fell back (see reasons per block)` : ''}`,
        },
        shouldReload: false,
      };
    }
    return {
      toast: {
        kind: 'ok',
        text: `runner named ${named} block${named === 1 ? '' : 's'}${fell > 0 ? `, ${fell} fell back` : ''} → store v${res.store_version}`,
      },
      shouldReload: true,
    };
  } catch (err) {
    const toast = writeErrorToast(err, expectedVersion, 'name');
    return { toast, shouldReload: toast.kind === 'conflict' };
  }
}
