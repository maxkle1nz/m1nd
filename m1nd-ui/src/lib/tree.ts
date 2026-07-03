/*
 * Living Tree assembly (HUMAN-LAYER-PRD §3.1–§3.3).
 *
 * The skeleton is the repo's real structure AS THE GRAPH KNOWS IT — file nodes
 * from the snapshot, arranged into a directory→file(→symbol) hierarchy by their
 * provenance.source_path. If the graph hasn't ingested a file, the tree honestly
 * doesn't decorate it (§3.5). Never a second fs view.
 *
 * Post-its are aggregated from the grounded_in topology captured on the live
 * wire: a `grounded_in` edge runs from a memory's `::tag::` fragment node to the
 * code node it cites; we join that fragment back to its `::file::` claim node
 * (shared doc slug) to recover the claim label + provenance, then pin it to the
 * cited file's tree row.
 */
import {
  NODE_TYPE,
  isPostItNode,
  lightDocSlug,
  readCreatedMs,
  readSourceAgent,
  type GraphSnapshot,
  type SnapshotNode,
} from './snapshot';
import { postItState, type PostItState } from './softProof';

export interface PostIt {
  /** The claim's one-line label (front face). */
  claim: string;
  /** `light:source_agent:<id>` or null (absent → "author unknown"). */
  sourceAgent: string | null;
  /** ms since authored, or null (absent → "age unknown"). */
  ageMs: number | null;
  /** Visual state: fresh / aging / stale / unknown. */
  state: PostItState;
  /** external_id of the memory claim node (for the drawer). */
  nodeId: string;
}

export interface TreeRow {
  /** Stable id (external_id for graph nodes; synthetic path id for pure dirs). */
  id: string;
  /** Display name (the familiar filetree label — last path segment). */
  name: string;
  /** 'dir' | 'file' | 'symbol'. */
  kind: 'dir' | 'file' | 'symbol';
  /** node_type for file/symbol rows (undefined for pure directories). */
  nodeType?: number;
  /** external_id of the backing graph node, when the row maps to one. */
  externalId?: string;
  /** Full repo-relative path (dirs and files). */
  path: string;
  depth: number;
  children: TreeRow[];
  /** Post-its pinned directly to this row. */
  postIts: PostIt[];
  /** Aggregate post-it count over this row's subtree (for directory rows, §3.3). */
  subtreePostItCount: number;
  /** True while this file node is present in the snapshot (decoratable). */
  mapped: boolean;
}

const NOW = () => Date.now();

/**
 * Build the post-it index: code file source_path -> PostIt[].
 * Uses now() for age when the claim node lacks a `light:created` tag but a
 * created ms is unavailable — falls back to null (never faked).
 */
export function buildPostItIndex(snap: GraphSnapshot, now: number = NOW()): Map<string, PostIt[]> {
  const byExternalId = new Map<string, SnapshotNode>();
  for (const n of snap.nodes) byExternalId.set(n.external_id, n);

  // doc slug -> the `::file::` claim node (carries provenance + a human label).
  const fileClaimBySlug = new Map<string, SnapshotNode>();
  for (const n of snap.nodes) {
    if (isPostItNode(n) && n.external_id.includes('::file::')) {
      const slug = lightDocSlug(n.external_id);
      if (slug) fileClaimBySlug.set(slug, n);
    }
  }

  // Prefer the section claim's label (the human title) over the file name.
  const sectionLabelBySlug = new Map<string, string>();
  for (const n of snap.nodes) {
    if (isPostItNode(n) && n.external_id.includes('::section::')) {
      const slug = lightDocSlug(n.external_id);
      if (slug && !sectionLabelBySlug.has(slug)) sectionLabelBySlug.set(slug, n.label);
    }
  }

  const index = new Map<string, PostIt[]>();
  const seen = new Set<string>(); // dedupe (slug + target path)

  for (const e of snap.edges) {
    if (e.relation !== 'grounded_in') continue;
    const slug = lightDocSlug(e.source_id);
    if (!slug) continue;
    const claimNode = fileClaimBySlug.get(slug);
    const target = byExternalId.get(e.target_id);
    const targetPath = target?.provenance.source_path ?? null;
    if (!claimNode || !targetPath) continue;

    const dedupeKey = `${slug}|${targetPath}`;
    if (seen.has(dedupeKey)) continue;
    seen.add(dedupeKey);

    const createdMs = readCreatedMs(claimNode);
    const sourceAgent = readSourceAgent(claimNode);
    const ageMs = createdMs != null ? Math.max(0, now - createdMs) : null;
    const claim = sectionLabelBySlug.get(slug) ?? claimNode.label;

    const postit: PostIt = {
      claim,
      sourceAgent,
      ageMs,
      state: postItState({ ageMs, sourceAgent }),
      nodeId: claimNode.external_id,
    };
    const list = index.get(targetPath) ?? [];
    list.push(postit);
    index.set(targetPath, list);
  }
  return index;
}

/**
 * Assemble the directory→file→symbol tree from a snapshot.
 * Directories are synthesized from file source_paths; symbol children are the
 * `contains`-style members whose external_id embeds the file path
 * (`file::<path>::<kind>::<name>`), which is how the graph names symbols.
 */
export function buildTree(snap: GraphSnapshot, postItIndex?: Map<string, PostIt[]>): TreeRow {
  const postIts = postItIndex ?? buildPostItIndex(snap);

  // Collect file nodes (node_type File) that have a real source_path, excluding
  // L1GHT memory nodes (those are post-its, not tree rows).
  const fileNodes = snap.nodes.filter(
    (n) =>
      n.node_type === NODE_TYPE.File &&
      n.provenance.source_path &&
      !n.tags.includes('light'),
  );

  const root: TreeRow = {
    id: '<root>',
    name: '',
    kind: 'dir',
    path: '',
    depth: -1,
    children: [],
    postIts: [],
    subtreePostItCount: 0,
    mapped: true,
  };

  // Map of path -> dir row, for O(1) parent lookup.
  const dirByPath = new Map<string, TreeRow>();
  dirByPath.set('', root);

  const ensureDir = (path: string, depth: number): TreeRow => {
    const existing = dirByPath.get(path);
    if (existing) return existing;
    const parentPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
    const parent = ensureDir(parentPath, depth - 1);
    const row: TreeRow = {
      id: `dir:${path}`,
      name: path.slice(path.lastIndexOf('/') + 1),
      kind: 'dir',
      path,
      depth: parent.depth + 1,
      children: [],
      postIts: [],
      subtreePostItCount: 0,
      mapped: true,
    };
    parent.children.push(row);
    dirByPath.set(path, row);
    return row;
  };

  // Symbol children per file path.
  const symbolsByFile = new Map<string, SnapshotNode[]>();
  for (const n of snap.nodes) {
    if (n.tags.includes('light')) continue;
    if (n.node_type === NODE_TYPE.File || n.node_type === NODE_TYPE.Directory) continue;
    const sp = n.provenance.source_path;
    if (!sp) continue;
    // Only treat as a child symbol when its id is scoped under the file id.
    if (!n.external_id.startsWith(`file::${sp}::`)) continue;
    const list = symbolsByFile.get(sp) ?? [];
    list.push(n);
    symbolsByFile.set(sp, list);
  }

  for (const fn of fileNodes) {
    const filePath = fn.provenance.source_path as string;
    const parentPath = filePath.includes('/')
      ? filePath.slice(0, filePath.lastIndexOf('/'))
      : '';
    const parentDir = ensureDir(parentPath, 0);
    const fileRow: TreeRow = {
      id: fn.external_id,
      name: filePath.slice(filePath.lastIndexOf('/') + 1),
      kind: 'file',
      nodeType: fn.node_type,
      externalId: fn.external_id,
      path: filePath,
      depth: parentDir.depth + 1,
      children: [],
      postIts: postIts.get(filePath) ?? [],
      subtreePostItCount: 0,
      mapped: true,
    };
    // Symbol children (sorted by line, then label).
    const syms = (symbolsByFile.get(filePath) ?? []).sort((a, b) => {
      const la = a.provenance.line_start ?? 0;
      const lb = b.provenance.line_start ?? 0;
      return la - lb || a.label.localeCompare(b.label);
    });
    for (const s of syms) {
      fileRow.children.push({
        id: s.external_id,
        name: s.label,
        kind: 'symbol',
        nodeType: s.node_type,
        externalId: s.external_id,
        path: `${filePath}::${s.label}`,
        depth: fileRow.depth + 1,
        children: [],
        postIts: [],
        subtreePostItCount: 0,
        mapped: true,
      });
    }
    parentDir.children.push(fileRow);
  }

  sortTree(root);
  computeSubtreeCounts(root);
  return root;
}

/** Directories first, then files/symbols; each group alphabetical. */
function sortTree(row: TreeRow): void {
  row.children.sort((a, b) => {
    if (a.kind !== b.kind) {
      if (a.kind === 'dir') return -1;
      if (b.kind === 'dir') return 1;
    }
    return a.name.localeCompare(b.name);
  });
  for (const c of row.children) sortTree(c);
}

/** Post-order fill of subtreePostItCount (§3.3 directory aggregate). */
function computeSubtreeCounts(row: TreeRow): number {
  let count = row.postIts.length;
  for (const c of row.children) count += computeSubtreeCounts(c);
  row.subtreePostItCount = count;
  return count;
}

/** Flatten a tree into visible rows given a set of expanded dir paths. */
export function flattenVisible(root: TreeRow, expanded: Set<string>): TreeRow[] {
  const out: TreeRow[] = [];
  const walk = (row: TreeRow) => {
    for (const c of row.children) {
      out.push(c);
      if ((c.kind === 'dir' || c.kind === 'file') && c.children.length > 0 && expanded.has(c.path)) {
        walk(c);
      }
    }
  };
  walk(root);
  return out;
}
