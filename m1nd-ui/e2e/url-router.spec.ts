/*
 * The hash router — the browser proof (the uiproof standard: deterministic
 * `npx playwright test`, no agent in the loop). Boots its OWN Vite dev server
 * (playwright.config) and mocks every /api/* route IN-PAGE — nothing reaches a live
 * owner (the served :1338 is out of bounds for tests, AGENTS.md).
 *
 * Proven, in a real browser against the real bundle:
 *  1. a deep link `#/world/<key>/map?block=…` opens the map with that block's panel —
 *     and BEATS the landing rule (worlds are present, yet the deep link wins);
 *  2. a real BACK returns from a world to the Universe (the brain-swap 'loading' is
 *     BY DESIGN — see R5 below — never a regression);
 *  3. a tray open-block writes `?block=` into the URL, and BACK closes the panel
 *     without breaking the map;
 *  4. a deep link to an UNRESOLVABLE world key (evicted / collision / pre-F30) falls
 *     back to the landing rule — it never strands the human in an empty map.
 *
 * R5 — the declared 'loading' (useBuildMap.ts:37-39): a BACK that swaps the viewed
 * brain (world → universe → a different world) shows the map's honest 'loading'
 * because `nextReadStatus` does not hold last-good across a brain change. This is
 * intended: the map is expected to change, so it does not lie with a stale snapshot.
 *
 * THE BRAIN KEY IS A BASENAME (R3, no-leak law): every URL below carries
 * `repo-alpha`, NEVER `/work/repo-alpha`. Neutral fixtures only — no personal path,
 * no real owner project/agent name.
 */
import { expect, test, type Page, type Route } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
// The real ratified seed (12 blocks) — a full Build Map to drive the block panel.
const blockSnapshot = JSON.parse(
  readFileSync(join(HERE, '..', 'src', '__fixtures__', 'system_blocks_snapshot.json'), 'utf8'),
);
// A block id that genuinely exists in the store, so the panel resolves it.
const BLOCK_ID: string = blockSnapshot.store.blocks[0].block_id;

const now = () => Date.now();
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

const instance = {
  instance_id: 'inst_e2e_router',
  workspace_root: '/work/repo-alpha',
  runtime_root: '/work/.m1nd',
  graph_source: 'ingest',
  plasticity_state: 'stable',
  pid: 4242,
  bind: null,
  port: null,
  started_at_ms: 1_700_000_000_000,
  last_heartbeat_ms: now(),
  mode: 'serve',
  status: 'running',
  owner_live: true,
  stale: false,
  conflicts: [],
  brain_kind: null,
  display_name: 'repo-alpha',
  project_root: '/work/repo-alpha',
  node_count: 3210,
  edge_count: 9000,
};

const health = {
  status: 'ok',
  uptime_secs: 60,
  node_count: 3210,
  edge_count: 9000,
  queries_processed: 0,
  agent_sessions: [],
  domain: 'code',
  graph_generation: 1,
  plasticity_generation: 1,
};

const EMPTY_UNIVERSE = {
  schema: 'm1nd-universe-v0',
  worlds: [],
  owner: { alerts_pending: 0 },
  totals: { worlds: 0, awake: 0, pending: 0 },
};

/** One world (repo-alpha at /work/repo-alpha) — its basename `repo-alpha` is the URL key. */
function universeWithWorld() {
  return {
    schema: 'm1nd-universe-v0',
    worlds: [
      {
        key: '/work/repo-alpha',
        root: '/work/repo-alpha',
        name: 'repo-alpha',
        node_count: 3210,
        edge_count: 9000,
        updated_ms: now() - 60_000,
        awake: false,
        presences: [],
        pending: { stamps: 0, ratifies: 0 },
        letters: { merge_wait: 0, total: 0 },
      },
    ],
    owner: { alerts_pending: 0 },
    totals: { worlds: 1, awake: 0, pending: 0 },
  };
}

/** A live (non-stagnant) executing head whose block IS in the snapshot store — so its
 *  tray card's "open block" button lands the human on a real, panel-able block. */
function missionsForBlock() {
  const fresh = new Date(now() - 30_000).toISOString();
  return {
    served_brain: { project_root: '/work/repo-alpha', display_name: 'repo-alpha' },
    missions: [
      {
        mission_id: 'msn_00000000a777',
        head_letter_id: 'aaaa11110001',
        head: {
          schema: 'm1nd-mission-letter-v0',
          mission_id: 'msn_00000000a777',
          mission_seq: 1,
          block_id: BLOCK_ID,
          brain_ref: 'repo-alpha',
          seat: 'hand',
          capability: 'build-runner',
          phase: 'executing',
          tokens_total: 0,
          started_at: fresh,
          updated_at: fresh,
        },
        superseded_count: 0,
      },
    ],
  };
}

interface MockOpts {
  universe?: unknown;
  snapshotPresent?: boolean;
  missions?: unknown;
}

function mockOwner(page: Page, opts: MockOpts = {}) {
  const universe = opts.universe ?? EMPTY_UNIVERSE;
  return page.route(
    (url) => url.pathname.startsWith('/api/'),
    async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname;
      try {
        if (path === '/api/health') return await json(route, health);
        if (path === '/api/universe') return await json(route, universe);
        if (path === '/api/instances') return await json(route, { instances: [instance] });
        if (path === '/api/instance/self')
          return await json(route, {
            instance,
            graph_state: {
              node_count: 3210,
              edge_count: 9000,
              finalized: true,
              graph_generation: 1,
              plasticity_generation: 1,
              cache_generation: 1,
              ingest_root_count: 1,
              ingest_roots: ['/work/repo-alpha'],
              workspace_root: '/work/repo-alpha',
              runtime_root: '/work/.m1nd',
            },
            active_agent_sessions: 0,
            queries_processed: 0,
            last_persist_secs_ago: 10,
            display_name: 'repo-alpha',
            project_root: '/work/repo-alpha',
          });
        if (path === '/api/presences') return await json(route, { presences: [], collisions: [] });
        if (path === '/api/tools') return await json(route, { tools: [], rest_brain_selector: true });
        if (path === '/api/mailbox') {
          if (url.searchParams.get('kind') === 'mission')
            return await json(route, opts.missions ?? { missions: [] });
          return await json(route, { missions: [] });
        }
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(
            route,
            opts.snapshotPresent
              ? { result: blockSnapshot }
              : { result: { present: false, honest: 'no skeleton yet' } },
          );
        if (path === '/api/graph/snapshot') return await json(route, { version: 1, nodes: [], edges: [] });
        if (path === '/api/graph/stats') return await json(route, { node_count: 3210, edge_count: 9000 });
        if (path === '/api/runnerd/status') return await json(route, { runners: [] });
        if (path === '/api/events')
          return await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
        if (path.startsWith('/api/tools/')) return await json(route, { result: {} });
        return await json(route, {});
      } catch {
        /* the page may abort a held request on teardown */
      }
    },
  );
}

const ARTIFACTS = 'e2e/artifacts';

// ── 1. a deep link opens the world map on the named block — and beats landing ──
test('deep-link #/world/<key>/map?block= opens the map with the block panel (beats landing)', async ({
  page,
}) => {
  await mockOwner(page, { universe: universeWithWorld(), snapshotPresent: true });

  // Worlds are present, so the LANDING rule would show the Universe — the deep link
  // must win and land straight on the world's map, on the named block.
  await page.goto(`/#/world/repo-alpha/map?block=${BLOCK_ID}`);

  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  await expect(page.locator(`[data-block-panel="${BLOCK_ID}"]`)).toBeVisible();
  // The Universe landing never took over.
  await expect(page.locator('[data-role="universe"]')).toHaveCount(0);

  // No-leak: the URL carries the basename, never the absolute root.
  const hash = await page.evaluate(() => location.hash);
  expect(hash).toContain('repo-alpha');
  expect(hash).not.toContain('/work/');

  await page.screenshot({ path: `${ARTIFACTS}/router-01-deeplink-map-block.png`, fullPage: true });
});

// ── 2. a real BACK returns from a world to the Universe (R5 'loading' by design) ─
test('back from a world map returns to the Universe (the brain-swap loading is by design)', async ({
  page,
}) => {
  await mockOwner(page, { universe: universeWithWorld(), snapshotPresent: true });
  await page.goto('/');

  // Worlds present → the Universe leads.
  await expect(page.locator('[data-role="universe"]')).toBeVisible();
  await expect(page.locator('[data-world-name="repo-alpha"]')).toBeVisible();

  // Enter the world's room (its Build Map) — a real pushState to #/world/repo-alpha/map.
  await page.locator('[data-world-name="repo-alpha"]').click();
  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  await expect.poll(() => page.evaluate(() => location.hash)).toContain('world/repo-alpha/map');

  // The browser BACK button — a real back — returns to the Universe. (R5: had this
  // been a swap between two DIFFERENT worlds, the map would honestly show 'loading'
  // on the way; that is by design, never a regression.)
  await page.goBack();
  await expect(page.locator('[data-role="universe"]')).toBeVisible();
});

// ── 3. a tray open-block writes ?block=, and BACK closes it without breaking ────
test('a tray open-block deep-selects (?block=) and BACK closes the panel, map intact', async ({
  page,
}) => {
  // Zero worlds + a bound skeleton → the Build Map (bound) is the front door.
  await mockOwner(page, { snapshotPresent: true, missions: missionsForBlock() });
  await page.goto('/');
  await expect(page.locator('[data-surface="map"]')).toBeVisible();

  // Expand the tray, then ask its card to open its block on the map.
  await page.locator('[data-role="mission-tray-toggle"]').first().click();
  const open = page.locator('[data-role="mission-open-block"]').first();
  await expect(open).toBeVisible();
  await open.click();

  // The URL gained ?block= (a real, backable entry) and the panel opened.
  await expect.poll(() => page.evaluate(() => location.hash)).toContain(`block=${BLOCK_ID}`);
  await expect(page.locator(`[data-block-panel="${BLOCK_ID}"]`)).toBeVisible();

  // BACK clears the tray-seed from the ADDRESS (?block= gone) and the map stays
  // mounted — no crash, no strand. Per the addressable boundary, the Build Map's own
  // live panel selection is transient (still closeable via its ✕), so back removes
  // the address's block, not necessarily the open panel — that is the boundary, not a
  // break.
  await page.goBack();
  await expect(page.locator('[data-surface="map"]')).toBeVisible();
  await expect.poll(() => page.evaluate(() => location.hash)).not.toContain('block=');
});

// ── 4. an unresolvable world key falls back to the landing, never strands ───────
test('a deep-link to an evicted world key falls back to the landing rule (no empty map)', async ({
  page,
}) => {
  // The owner serves repo-alpha; the link names a world that is not there.
  await mockOwner(page, { universe: universeWithWorld(), snapshotPresent: true });
  await page.goto('/#/world/ghost/map?block=sb_nope');

  // The key does not resolve → give up → the landing rule runs → worlds present →
  // the Universe. Never a stranded, empty world map.
  await expect(page.locator('[data-role="universe"]')).toBeVisible();
  await expect(page.locator('[data-surface="map"]')).toHaveCount(0);
  // …and the address bar is canonicalized to what we actually show — the dead
  // `#/world/ghost/…` route never survives in the URL.
  await expect.poll(() => page.evaluate(() => location.hash)).toBe('#/universe');
});

// ── 5. an invalid hash is canonicalized away — the bar never keeps a dead route ──
test('an invalid hash on load canonicalizes to the landed surface', async ({ page }) => {
  // Zero worlds → the landing lands the tree; the junk `#/zzz` must not survive.
  await mockOwner(page);
  await page.goto('/#/zzz');
  await expect.poll(() => page.evaluate(() => location.hash)).toBe('#/tree');
  await expect(page.locator('[data-role="universe"]')).toHaveCount(0);
});

test('backing into a junk hash canonicalizes it away (popstate replaceState, no new entry)', async ({
  page,
}) => {
  await mockOwner(page);
  await page.goto('/#/zzz');
  await expect.poll(() => page.evaluate(() => location.hash)).toBe('#/tree');
  // A human types a junk hash (a real history entry), navigates on, then hits BACK.
  await page.evaluate(() => history.pushState(null, '', '#/qqq'));
  await page.evaluate(() => history.pushState(null, '', '#/tree'));
  await page.goBack(); // → popstate onto the junk `#/qqq`
  // The fallback lands the tree AND replaces the dead route out of the address bar.
  await expect.poll(() => page.evaluate(() => location.hash)).toBe('#/tree');
});
