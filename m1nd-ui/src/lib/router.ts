/*
 * The hash router — a thin URL⇄state sync over the Surface machine (App.tsx).
 *
 * WHY HASH: the SPA is served rust-embed from a single index.html
 * (http_server.rs:299). A hash fragment never reaches the server, so deep links
 * and back/forward need ZERO server route — the whole scheme lives client-side.
 *
 * WHY PURE: every function here is DOM-free and React-free (the repo's
 * `loadBuildMap`/`reduceUniversePoll` pattern) so parse/serialize, the deep-link
 * precedence, and the brain-key fallback are unit-provable without a render.
 *
 * THE ADDRESSABLE BOUNDARY (half the design): only DURABLE LOCATION is in the URL —
 * the surface, which brain the surface views, and the tray-targeted map block
 * (`?block=`). TRANSIENTS stay out: modals (ingest), the Cmd+K palette, the 3-beat
 * orientation, `hallOpenAlerts` (how you entered the Hall), and the Build Map's own
 * ad-hoc card selection (a click on a block card is exploration, not an address —
 * only the tray-seeded `mapTargetBlock` is addressable). See
 * docs/HUMAN-VIEW-V2-F30-UNIVERSE.md §placement.
 *
 * THE BRAIN KEY (R3, no-leak law): a hosted world is addressed in the URL by the
 * BASENAME of its root, never the absolute root — AGENTS.md forbids personal paths
 * in a public repo, and the e2e crystallizes URLs into public specs. The basename
 * is stable across owner restarts (a pure function of the path), unlike the
 * per-process `instance_id` (instance_registry.rs `generate_instance_id` hashes
 * pid+clock+seq — ephemeral by construction). An unresolvable key (brain evicted,
 * or a basename collision) falls back to the normal landing rule — it NEVER strands
 * the human in an empty map.
 */
import { type ViewedBrain, BOUND_VIEW } from './viewedBrain';
import type { UniverseWorld } from './universe';
import type { InstanceRegistryEntry } from '../types';
import { brainProjectPath, brainDisplayName } from './hallSemantics';

/** The shell surfaces (App.tsx). `threshold` (first-run onboarding) is the one
 *  surface that is NOT addressable — deep-linking to onboarding is meaningless, so
 *  it serializes to a neutral home hash and parses back to the landing rule. */
export type Surface = 'universe' | 'tree' | 'hall' | 'threshold' | 'map';

/** The addressable surfaces a URL can name (the `threshold` exclusion made a type). */
export type RouteSurface = 'universe' | 'hall' | 'tree' | 'map';

/** A parsed hash. `brainKey == null` = the bound/owner brain (`#/tree`, `#/map`);
 *  a key = a hosted world (`#/world/<key>/…`). `block` rides `#/…/map?block=`. */
export interface ParsedRoute {
  surface: RouteSurface;
  brainKey: string | null;
  block: string | null;
}

/**
 * A transition intent handed to `navigate()` — the ONE writer of both the shell
 * state and the URL (R4). An omitted `view`/`block` means "keep the current value"
 * (the caller is not changing that axis); `block: null` explicitly clears it.
 * `hallAlerts` is a transient (never serialized) — it only tells the Hall whether
 * to auto-open its owner-alerts panel.
 */
export interface NavTarget {
  surface: Surface;
  /** The brain to view; omit to keep the current viewed brain. */
  view?: ViewedBrain;
  /** The map block to target; omit to keep current, `null` to clear. */
  block?: string | null;
  /** Open the Hall focused on the owner-alerts panel (transient — not in the URL). */
  hallAlerts?: boolean;
}

/**
 * The repo basename of a root — the URL brain key. Separator-agnostic and
 * trailing-slash-tolerant, mirroring the server's `basename_of` (session.rs:571)
 * so a key serialized here resolves against a world whose `name` the server
 * computed the same way. Falls back to the trimmed input when there is no
 * separator (a bare name is its own basename).
 */
export function basename(root: string): string {
  const trimmed = root.trim().replace(/[/\\]+$/, '');
  if (trimmed.length === 0) return root.trim();
  const idx = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
  const base = idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
  return base.length > 0 ? base : trimmed;
}

/** Pull the `block` query value out of a hash query string (`block=sb_x`). */
function parseBlock(query: string): string | null {
  if (!query) return null;
  const b = new URLSearchParams(query).get('block');
  return b && b.length > 0 ? b : null;
}

/**
 * Parse `window.location.hash` into a route, or `null` when it names no known
 * surface (an empty/`#/` hash, `#/threshold`, or garbage) — a `null` route means
 * "no deep link, run the landing rule". Accepts the hash with or without its
 * leading `#`.
 */
export function parseRoute(rawHash: string): ParsedRoute | null {
  let h = rawHash ?? '';
  if (h.startsWith('#')) h = h.slice(1);
  const qIdx = h.indexOf('?');
  const pathPart = qIdx >= 0 ? h.slice(0, qIdx) : h;
  const block = parseBlock(qIdx >= 0 ? h.slice(qIdx + 1) : '');
  const segs = pathPart.split('/').filter((s) => s.length > 0);

  if (segs.length === 1) {
    switch (segs[0]) {
      case 'universe':
        return { surface: 'universe', brainKey: null, block: null };
      case 'hall':
        return { surface: 'hall', brainKey: null, block: null };
      case 'tree':
        return { surface: 'tree', brainKey: null, block: null };
      case 'map':
        return { surface: 'map', brainKey: null, block };
      default:
        return null;
    }
  }
  if (segs.length === 3 && segs[0] === 'world') {
    const key = decodeURIComponent(segs[1]);
    if (!key) return null;
    if (segs[2] === 'tree') return { surface: 'tree', brainKey: key, block: null };
    if (segs[2] === 'map') return { surface: 'map', brainKey: key, block };
    return null;
  }
  return null;
}

/**
 * Serialize the shell state into the canonical hash — the inverse of `parseRoute`.
 * A hosted view (`view.root != null`) becomes `#/world/<basename>/…`; the bound
 * view keeps the short `#/tree` / `#/map` forms. `threshold` is not addressable
 * and yields a neutral home hash. The `?block=` rides ONLY the map surface.
 */
export function serializeRoute(surface: Surface, view: ViewedBrain, block: string | null): string {
  const key = view.root != null ? basename(view.root) : null;
  switch (surface) {
    case 'universe':
      return '#/universe';
    case 'hall':
      return '#/hall';
    case 'threshold':
      // Onboarding is not a place you link to — a neutral home hash the landing
      // rule re-derives from owner state on the next load.
      return '#/';
    case 'tree':
      return key ? `#/world/${encodeURIComponent(key)}/tree` : '#/tree';
    case 'map': {
      const base = key ? `#/world/${encodeURIComponent(key)}/map` : '#/map';
      return block ? `${base}?block=${encodeURIComponent(block)}` : base;
    }
  }
}

/**
 * Resolve a URL brain key (a basename) to a `ViewedBrain` against the live owner
 * state — the worlds panorama first (its `name` is the server basename), the
 * Hall's brains registry second. Returns `null` when the key resolves to NOTHING
 * (evicted / pre-F30 owner) or AMBIGUOUSLY (two worlds share a basename): both are
 * honest fallbacks to the landing rule, never a fabricated pick. Exactly one match
 * wins.
 */
export function resolveBrainKey(
  key: string,
  worlds: UniverseWorld[],
  brains: InstanceRegistryEntry[] | null,
): ViewedBrain | null {
  const matches: ViewedBrain[] = [];
  for (const w of worlds) {
    if (basename(w.root) === key) {
      matches.push({ root: w.root, displayName: w.name, nodeCount: w.node_count ?? null });
    }
  }
  // Only consult the Hall registry when the panorama named nothing — a world in
  // both lists must not double-count into a false ambiguity.
  if (matches.length === 0 && brains) {
    for (const b of brains) {
      const root = brainProjectPath(b);
      if (root && basename(root) === key) {
        matches.push({ root, displayName: brainDisplayName(b), nodeCount: b.node_count ?? null });
      }
    }
  }
  return matches.length === 1 ? matches[0] : null;
}

/** Turn a parsed route + its resolved brain view into a `navigate()` intent. The
 *  `?block=` is honored only on the map surface (a tree route carries no block). */
export function routeToIntent(route: ParsedRoute, view: ViewedBrain): NavTarget {
  return {
    surface: route.surface,
    view,
    block: route.surface === 'map' ? route.block : null,
  };
}

/** The verdict of one deep-link resolution attempt. */
export type DeepLinkOutcome =
  | { kind: 'apply'; intent: NavTarget } // resolved → seed the shell (navigate replace)
  | { kind: 'pending' } //                 sources not settled yet → wait a tick
  | { kind: 'give-up' }; //                unresolvable → the landing rule decides

/**
 * Decide what a deep link should do RIGHT NOW, given the owner state read so far.
 *
 *  - No route → give up (there is no deep link; the landing rule runs).
 *  - A bound route (`brainKey == null`) → apply immediately: `#/universe`, `#/hall`,
 *    `#/tree`, `#/map` need no async brain lookup, so they seed the shell at once
 *    (this is what makes a deep link BEAT the landing rule — the surface is seeded
 *    before the `surface == null` gate would fire).
 *  - A world route → apply when the key resolves; otherwise WAIT while the worlds /
 *    brains reads are still settling (`settled == false`), and GIVE UP once both
 *    have settled without a match (evicted brain / basename collision / pre-F30
 *    owner) so the human lands normally instead of stranding in an empty map.
 */
export function resolveDeepLink(
  route: ParsedRoute | null,
  worlds: UniverseWorld[],
  brains: InstanceRegistryEntry[] | null,
  settled: boolean,
): DeepLinkOutcome {
  if (!route) return { kind: 'give-up' };
  if (route.brainKey == null) {
    return { kind: 'apply', intent: routeToIntent(route, BOUND_VIEW) };
  }
  const view = resolveBrainKey(route.brainKey, worlds, brains);
  if (view) return { kind: 'apply', intent: routeToIntent(route, view) };
  return settled ? { kind: 'give-up' } : { kind: 'pending' };
}
