/*
 * viewedBrain — which brain the tree is currently reading (HUMAN-LAYER-PRD §4A.9).
 *
 * The tree was bound-graph-only. Opening a hosted card from the Hall now sets a
 * ViewedBrain; every graph/tool fetch carries its `?brain=<root>` selector, the
 * Brain Chip flips to the served brain, and a response whose `served_brain` echo
 * names a DIFFERENT brain is dropped, never rendered (INV-15). Pure + DOM-free so
 * the invariant is unit-testable with captured two-brain fixtures.
 *
 * `root == null` is the BOUND brain (the owner's own graph) — the tree's default,
 * byte-compatible with pre-2H behavior. A non-null root is a hosted project brain.
 */

/** The brain the tree is viewing. `root: null` = the bound/owner graph. */
export interface ViewedBrain {
  /** The hosted brain's absolute project root, or null for the bound graph. */
  root: string | null;
  /** The brain's display name — seeded from the Hall card, confirmed by the echo. */
  displayName: string | null;
  /** Node count for the Brain Chip — seeded from the card, confirmed by the echo. */
  nodeCount: number | null;
}

/** The bound brain — the tree's default viewed brain (no selector). */
export const BOUND_VIEW: ViewedBrain = { root: null, displayName: null, nodeCount: null };

/** Canonicalize a root for comparison: trim + strip a single trailing slash. The
 *  server compares on canonical paths (macOS `/var`→`/private/var`, trailing
 *  slash); the client cannot resolve symlinks, so it does the cheap normalization
 *  and leans on the SERVER's echo as the source of truth (this is a guard, not the
 *  resolver). */
export function canonRootForCompare(root: string | null | undefined): string | null {
  const r = root?.trim();
  if (!r) return null;
  return r.length > 1 && r.endsWith('/') ? r.replace(/\/+$/, '') : r;
}

/**
 * The `?brain=` selector to send for a viewed brain: the hosted root, or
 * undefined for the bound brain (so the URL is untouched — byte-compatible).
 */
export function brainSelectorFor(view: ViewedBrain | null): string | undefined {
  return view?.root ?? undefined;
}

/**
 * INV-15 — does this `served_brain` echo belong to the brain we asked for?
 *
 * - Viewing the BOUND brain (`view.root == null`): accept an absent echo (a
 *   pre-2H owner sends none) AND an echo naming the bound brain. A 2H owner's
 *   bound echo names the bound root; the client can't know the bound root's
 *   canonical form without the self envelope, so the bound case trusts the
 *   request (the server already routed absent→bound). Accept.
 * - Viewing a HOSTED brain (`view.root` set): the echo MUST name that root
 *   (canonical compare). An absent echo means the owner ignored the selector
 *   (pre-2H) — that is a MISMATCH for a hosted view: it would render the bound
 *   graph under the hosted chip. Drop.
 *
 * Returns true = render; false = drop (one brain's nodes under another's chip).
 */
export function servedBrainMatches(
  view: ViewedBrain | null,
  echo: { project_root?: string | null } | null | undefined,
): boolean {
  const want = canonRootForCompare(view?.root);
  // Bound view: the server resolves absent→bound, so trust it (echo optional).
  if (want == null) return true;
  // Hosted view: the echo must be present AND name our root.
  const got = canonRootForCompare(echo?.project_root);
  if (got == null) return false; // owner ignored the selector → would be the bound graph
  return got === want;
}
