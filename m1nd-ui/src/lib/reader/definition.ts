/*
 * reader/definition — click-to-definition FROM THE EDGES (donor dossier §2 Fatia
 * 1.3: "not an LSP"). A symbol's outgoing navigable edge → resolve `target_id` →
 * jump to `target.provenance.source_path + line_start`. Pure + deterministic.
 *
 * HONEST DEGRADATION IS THE LAW (dossier + docs/PATHOS.md Known Problems):
 *   - Rust gets call-edge jumps; TS/JS/Python/Go get def/import/use jumps (their
 *     call edges are not tracked yet) — encoded in `languages.ts`.
 *   - an ambiguous receiver (≥2 grounded same-name targets) → a CANDIDATE list, the
 *     human picks;
 *   - a reference whose target is external/dangling, or whose only edges are
 *     call-class in a language without call edges → an explicit ABSTAIN
 *     ("no grounded target"), NEVER a fabricated jump.
 */
import { NODE_TYPE, type GraphSnapshot, type SnapshotNode } from '../snapshot';
import { relationClass, relationNavigable, type LanguageProfile } from './languages';

/** A navigable definition target — a grounded node with a real file + line. */
export interface DefTarget {
  id: string;
  label: string;
  /** A readable node kind ("function"/"struct"/… or "symbol" when unclassified). */
  kind: string;
  path: string;
  line: number;
  namespace: string | null;
}

export type AbstainReason = 'none' | 'ungrounded' | 'calls-not-tracked';

export type DefResolution =
  | { kind: 'target'; target: DefTarget }
  | { kind: 'candidates'; targets: DefTarget[] }
  | { kind: 'abstain'; reason: AbstainReason; message: string };

const KIND_LABEL: Record<number, string> = {
  [NODE_TYPE.File]: 'file',
  [NODE_TYPE.Directory]: 'directory',
  [NODE_TYPE.Function]: 'function',
  [NODE_TYPE.Class]: 'class',
  [NODE_TYPE.Struct]: 'struct',
  [NODE_TYPE.Enum]: 'enum',
  [NODE_TYPE.Type]: 'type',
  [NODE_TYPE.Module]: 'module',
  [NODE_TYPE.Reference]: 'reference',
  [NODE_TYPE.Concept]: 'concept',
};

/** A node is a GROUNDED jump target iff it carries a real file path + start line —
 *  an external/synthetic node (no provenance) is never a jump, it abstains. */
export function defTargetOf(node: SnapshotNode | undefined): DefTarget | null {
  if (!node) return null;
  const p = node.provenance;
  if (!p || !p.source_path || p.line_start == null) return null;
  return {
    id: node.external_id,
    label: node.label,
    kind: KIND_LABEL[node.node_type] ?? 'symbol',
    path: p.source_path,
    line: p.line_start,
    namespace: p.namespace,
  };
}

const ABSTAIN_MESSAGE: Record<AbstainReason, string> = {
  none: 'no outgoing reference in the graph',
  ungrounded: 'no grounded target — the reference leaves the graph (external/unresolved)',
  'calls-not-tracked': 'no grounded target — call edges are not tracked for this language yet',
};

/**
 * Resolve a source symbol's definition target(s) from the graph edges, under the
 * SOURCE file's language profile (which decides whether call edges are trusted).
 *
 * @param snapshot the served graph snapshot
 * @param sourceId the outline symbol's node id (the reference site)
 * @param profile  the source file's language profile (Rust ⇒ call edges trusted)
 */
export function resolveDefinition(
  snapshot: GraphSnapshot | null | undefined,
  sourceId: string,
  profile: LanguageProfile,
): DefResolution {
  if (!snapshot) return { kind: 'abstain', reason: 'none', message: ABSTAIN_MESSAGE.none };

  const byId = new Map<string, SnapshotNode>();
  for (const n of snapshot.nodes) byId.set(n.external_id, n);

  let sawNavigable = false;
  let sawUntrustedCall = false;
  const targets: DefTarget[] = [];
  const seen = new Set<string>();

  for (const e of snapshot.edges) {
    if (e.source_id !== sourceId) continue;
    if (e.target_id === sourceId) continue; // never a self-jump
    if (!relationNavigable(e.relation, profile)) {
      // Track the honest reason: a call-class edge the language cannot trust.
      if (relationClass(e.relation) === 'call' && !profile.callEdgesTrusted) sawUntrustedCall = true;
      continue;
    }
    sawNavigable = true;
    const target = defTargetOf(byId.get(e.target_id));
    if (!target) continue; // navigable but the target left the graph (external/dangling)
    if (seen.has(target.id)) continue;
    seen.add(target.id);
    targets.push(target);
  }

  if (targets.length === 1) return { kind: 'target', target: targets[0] };
  if (targets.length > 1) {
    // A stable, human-scannable order (path, then line, then label).
    targets.sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line || a.label.localeCompare(b.label));
    return { kind: 'candidates', targets };
  }

  // Zero grounded targets — abstain with the most specific honest reason.
  const reason: AbstainReason = sawNavigable ? 'ungrounded' : sawUntrustedCall ? 'calls-not-tracked' : 'none';
  return { kind: 'abstain', reason, message: ABSTAIN_MESSAGE[reason] };
}
