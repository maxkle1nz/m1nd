/*
 * curation — the PURE curation-packet compositor (HUMAN-VIEW-V2 F11-TECH §3a; the
 * heavy-case escape hatch of the friction law §0c).
 *
 * `composeCurationPacket` turns the WHOLE candidate store + the owner's optional
 * note into the Markdown a curation mission receives — the mirror of `packet.ts`
 * (a pure compositor: no I/O, no engine call). The dispatch rides the EXISTING
 * direct path (`sendDirectPacket`: a seq-1 `judging` letter + the packet on the
 * clipboard — delivery is not execution); a curation SPAWN waits for the
 * hand-runner capability, which the runner daemon refuses in the MVP (F2.5 §5e).
 *
 * Copy law (PRD §13): scoped, auditable claims only. The packet carries block ids,
 * names, and repo-relative context — no absolute paths, no secrets, no file
 * bodies. The two laws the hand must keep travel IN the packet: it edits ONLY
 * through `candidate_edit` under OCC (§3c/1a), and it can NEVER ratify (1e).
 */
import type { SystemBlock, SystemBlockStore } from './buildMap';
import type { CurationSpawnResult } from './candidateEdit';

export interface CurationPacketInput {
  store: SystemBlockStore;
  /** The §1f brain reference (a repo_id, never an absolute path). */
  repoId: string | null;
  /** The owner's optional guidance — verbatim, may be empty. */
  note?: string;
}

/** One block's honest one-liner for the curation table. */
function blockLine(store: SystemBlockStore, block: SystemBlock): string {
  const meta = block.candidate_meta;
  const naming =
    meta?.needs_owner_naming === true
      ? 'PROVISIONAL — needs a name'
      : `named by ${meta?.named_by ?? 'unknown'}`;
  const seams = block.membership.filter(
    (m) =>
      m.role === 'shared' ||
      store.blocks.filter((b) => b.membership.some((x) => x.path === m.path)).length > 1,
  ).length;
  return `- \`${block.block_id}\` **${block.name}** · ${block.membership.length} member${block.membership.length === 1 ? '' : 's'} · ${naming}${seams > 0 ? ` · ${seams} seam member${seams === 1 ? '' : 's'}` : ''}`;
}

/**
 * Compose the curation-mission packet (§3a): the candidate census, every block's
 * honest state, the unmapped residue sample, the owner's note, and the two laws
 * the hand is bound by. Pure — the exact Markdown the letter's `packet_ref`
 * anchors and the clipboard receives.
 */
export function composeCurationPacket(input: CurationPacketInput): string {
  const { store, repoId, note } = input;
  const candidates = store.blocks.filter((b) => b.state === 'candidate');
  const provisional = candidates.filter((b) => b.candidate_meta?.needs_owner_naming === true);
  const seamPaths = new Set<string>();
  for (const b of candidates) {
    for (const m of b.membership) {
      if (
        m.role === 'shared' ||
        store.blocks.filter((x) => x.membership.some((e) => e.path === m.path)).length > 1
      ) {
        seamPaths.add(m.path);
      }
    }
  }
  const unmappedTotal = store.unmapped_total ?? 0;
  const unmappedSample = (store.unmapped_files ?? []).slice(0, 12);

  const parts: string[] = [
    `# Curation mission — candidate skeleton of ${repoId ?? 'this brain'}`,
    [
      `store v${store.store_version} · skeleton \`${store.skeleton.skeleton_id}\` (candidate v${store.skeleton.version})`,
      `${candidates.length} candidate block${candidates.length === 1 ? '' : 's'} · ${provisional.length} provisional name${provisional.length === 1 ? '' : 's'} · ${seamPaths.size} seam member${seamPaths.size === 1 ? '' : 's'} · ${unmappedTotal} unmapped file${unmappedTotal === 1 ? '' : 's'}`,
    ].join('\n'),
    `## What to do\nCurate this candidate to a ratifiable map: merge thin blocks, name the provisional ones (a short plain-text name ≤ 40 chars + a one-line purpose ≤ 120), resolve the seams (keep-in-both or name a primary), and assign the unmapped residue that clearly belongs somewhere.`,
    `## The blocks\n${candidates.map((b) => blockLine(store, b)).join('\n')}`,
  ];
  if (unmappedSample.length > 0) {
    parts.push(
      `## Unmapped residue (${unmappedTotal} total — a sample)\n${unmappedSample.map((p) => `- \`${p}\``).join('\n')}`,
    );
  }
  parts.push(`## Owner's note\n${note?.trim() || '_(none — use your judgment, honestly)_'}`);
  parts.push(
    [
      '## The laws you are bound by',
      '- Edit ONLY through the `candidate_edit` verb, under the OCC key you read (`system_blocks_snapshot` → `expected_store_version`). A conflict means reload and rebase — never force.',
      '- Renaming through your seat stamps `named_by:"runner"` — honest provenance, never claim the owner seat.',
      '- You can NEVER ratify (the owner signs; `system_blocks_ratify` is the human gesture). Editing a ratified skeleton is refused.',
      '- When you finish, report what you merged/named/resolved and what you left, with reasons.',
    ].join('\n'),
  );
  parts.push(
    '---\n_Composed read-only from the m1nd Build Map — block ids and repo-relative context only, no secrets, no file bodies. The hand proposes; the human signs._',
  );
  return parts.join('\n\n') + '\n';
}

/** The two dispatch seams (F12), injected so the decision is provable DOM-free (the
 *  repo's `sendDirectPacket` pattern). `spawn` is the F12 propose-apply verb; `direct`
 *  is the no-runner fallback (a letter + the packet on the clipboard). */
export interface CurationDispatchDeps {
  /** POST `curation_spawn` for the read store version (default `api.curationSpawn`).
   *  May throw (`conflict`, read-only, network) — the caller surfaces it verbatim. */
  spawn: (expectedStoreVersion: number) => Promise<CurationSpawnResult>;
  /** The DIRECT fallback: compose + post the seq-1 letter and copy the packet.
   *  Returns the honest ids + whether the clipboard write succeeded. */
  direct: () => Promise<{ mission_id: string; mission_seq: number; clipboardCopied: boolean }>;
}

/**
 * Dispatch a curation (§3): when a runner daemon is announced (`runnerAvailable`) run
 * the propose-apply SPAWN — the owner applies the hand's proposal and posts the
 * summary; an honest `refusal` (`no_hand_runner`/`proposal_malformed`/`batch_refused`)
 * is shown verbatim (never a silent DIRECT). With NO runner announced fall back to the
 * DIRECT path. Pure: the exact `{ ok, message }` the banner's curation-result reads.
 * The store only changes on a SPAWN success, so the caller reloads only then.
 */
export async function dispatchCuration(
  args: { runnerAvailable: boolean; storeVersion: number },
  deps: CurationDispatchDeps,
): Promise<{ ok: boolean; message: string; applied: boolean }> {
  if (args.runnerAvailable) {
    const res = await deps.spawn(args.storeVersion);
    if (res.refusal) {
      return { ok: false, message: res.refusal, applied: false };
    }
    const ops = `${res.ops_count} op${res.ops_count === 1 ? '' : 's'}`;
    return {
      ok: true,
      applied: true,
      message: `curation applied — ${ops}, store v${res.store_version}${res.report ? ` · ${res.report}` : ''}`,
    };
  }
  const d = await deps.direct();
  return {
    ok: true,
    applied: false,
    message: `curation letter posted (${d.mission_id}, seq ${d.mission_seq})${
      d.clipboardCopied ? ' — the packet is on your clipboard, paste it into your agent' : ''
    }`,
  };
}
