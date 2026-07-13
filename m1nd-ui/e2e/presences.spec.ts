/*
 * Presence strip — the browser proof (ORGANISM-INSIDE-PRD P1 · askGOD-verdict-P1;
 * the uiproof standard: deterministic `npx playwright test`, zero agents in the
 * loop). The suite boots its OWN Vite dev server (playwright.config) and mocks
 * every /api/* route IN-PAGE — nothing reaches a live owner (the served :1338 is
 * out of bounds for tests, AGENTS.md).
 *
 * What is proven, in a real browser against the real bundle:
 *  1. a live roster shows the team — who / where / on-what / a human age / a
 *     mutation dot — and the limitation is written on the surface;
 *  2. an arranged same-worktree collision surfaces as a CALM amber notice on the
 *     Hall (never a modal, never a block) — the P1 gate's arranged collision;
 *  3. the honest EMPTY state reads a sentence, not a blank.
 *
 * Neutral fixtures only (no-leak law): no real project/agent name of the owner.
 */
import { expect, test, type Page, type Route } from '@playwright/test';

const NODE_COUNT = 3210;
const now = () => Date.now();

// ── neutral owner envelopes (example names only) ──────────────────────────────
const instance = {
  instance_id: 'inst_e2e_pres',
  workspace_root: '/work/repo-alpha',
  runtime_root: '/work/repo-alpha/.m1nd',
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
  node_count: null,
  edge_count: null,
};

const selfEnvelope = {
  instance,
  graph_state: {
    node_count: NODE_COUNT,
    edge_count: 9_000,
    finalized: true,
    graph_generation: 1,
    plasticity_generation: 1,
    cache_generation: 1,
    ingest_root_count: 1,
    ingest_roots: ['/work/repo-alpha'],
    workspace_root: '/work/repo-alpha',
    runtime_root: '/work/repo-alpha/.m1nd',
  },
  active_agent_sessions: 0,
  queries_processed: 0,
  last_persist_secs_ago: 10,
  display_name: 'repo-alpha',
  project_root: '/work/repo-alpha',
};

const health = {
  status: 'ok',
  uptime_secs: 60,
  node_count: NODE_COUNT,
  edge_count: 9_000,
  queries_processed: 0,
  agent_sessions: [],
  domain: 'code',
  graph_generation: 1,
  plasticity_generation: 1,
};

const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

/** Wire the mocked owner; `presenceBody()` supplies the /api/presences response. */
function mockOwner(page: Page, presenceBody: () => unknown) {
  return page.route(
    (url) => url.pathname.startsWith('/api/'),
    async (route) => {
      const path = new URL(route.request().url()).pathname;
      try {
        if (path === '/api/health') return await json(route, health);
        if (path === '/api/instance/self') return await json(route, selfEnvelope);
        if (path === '/api/instances') return await json(route, { instances: [instance] });
        if (path === '/api/presences') return await json(route, presenceBody());
        if (path === '/api/graph/snapshot') return await json(route, { version: 1, nodes: [], edges: [] });
        if (path === '/api/graph/stats') return await json(route, { node_count: NODE_COUNT, edge_count: 9_000 });
        if (path === '/api/runnerd/status') return await json(route, { runners: [] });
        if (path === '/api/mailbox') return await json(route, { missions: [] });
        if (path === '/api/tools') return await json(route, { tools: [], rest_brain_selector: false });
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(route, { result: { present: false, honest: 'no skeleton yet' } });
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

/** Land on the tree (brains>0, no skeleton), then open the Hall via the Brain Chip. */
async function openHall(page: Page) {
  await page.goto('/');
  const chip = page.locator('[data-role="brain-chip"]');
  await expect(chip).toBeVisible();
  await chip.click();
  await expect(page.locator('[data-role="presence-strip"]')).toBeVisible();
}

const ARTIFACTS = 'e2e/artifacts';

// ── 1. the live roster: the team at one glance ────────────────────────────────
test('a live roster shows who/where/on-what/age + the limitation on the surface', async ({ page }) => {
  await mockOwner(page, () => ({
    presences: [
      {
        agent_id: 'atlas',
        root: '/work/repo-alpha',
        caller_root: '/work/repo-alpha/.wt/lane-1',
        first_seen_ms: now() - 12 * 60_000,
        last_seen_ms: now() - 25_000,
        query_count: 9,
        mutation: { observed_at_ms: now() - 4_000 },
        task_ref: 'wire the presence beat',
      },
      {
        agent_id: 'beacon',
        root: '/work/repo-beta',
        first_seen_ms: now() - 30 * 60_000,
        last_seen_ms: now() - 6 * 60_000,
        query_count: 3,
        mutation: { declared_intent: 'refactor the reader' },
        task_ref: null,
      },
      {
        agent_id: 'cirrus',
        root: '/work/repo-alpha',
        first_seen_ms: now() - 3 * 60_000,
        last_seen_ms: now() - 8_000,
        query_count: 12,
        mutation: {},
      },
    ],
    collisions: [],
  }));
  await openHall(page);

  const strip = page.locator('[data-role="presence-strip"]');
  await expect(strip.locator('[data-role="presence-count"]')).toHaveText('3');
  await expect(strip.locator('[data-role="presence-limitation"]')).toContainText(
    'presence = activity visible to m1nd',
  );
  // atlas: observed mutation, active, where shows brain · worktree, task shown.
  const atlas = strip.locator('[data-presence-agent="atlas"]');
  await expect(atlas).toHaveAttribute('data-liveness', 'active');
  await expect(atlas).toHaveAttribute('data-mutation', 'observed');
  await expect(atlas).toContainText('repo-alpha · lane-1');
  await expect(atlas).toContainText('wire the presence beat');
  await expect(atlas.locator('[data-role="presence-age"]')).toHaveText(/\d+s/);
  // beacon: declared intent, fading (6m old) — the age is still shown.
  const beacon = strip.locator('[data-presence-agent="beacon"]');
  await expect(beacon).toHaveAttribute('data-liveness', 'fading');
  await expect(beacon).toHaveAttribute('data-mutation', 'declared');
  await expect(beacon.locator('[data-role="presence-age"]')).toHaveText(/\d+m/);
  // cirrus: read-only, no mutation dot.
  await expect(strip.locator('[data-presence-agent="cirrus"]')).toHaveAttribute('data-mutation', 'none');
  await expect(strip.locator('[data-role="presence-collision"]')).toHaveCount(0);

  await page.screenshot({ path: `${ARTIFACTS}/presence-01-live-roster.png`, fullPage: true });
});

// ── 2. the arranged collision: calm amber, pre-landing (the P1 gate) ──────────
test('an arranged same-worktree collision surfaces as a calm amber notice, never a modal', async ({ page }) => {
  const wt = '/work/repo-alpha/.wt/lane-1';
  await mockOwner(page, () => ({
    presences: [
      {
        agent_id: 'atlas',
        root: '/work/repo-alpha',
        caller_root: wt,
        first_seen_ms: now() - 8 * 60_000,
        last_seen_ms: now() - 10_000,
        query_count: 20,
        mutation: { observed_at_ms: now() - 2_000 },
        task_ref: 'edit the store',
      },
      {
        agent_id: 'beacon',
        root: '/work/repo-alpha',
        caller_root: wt,
        first_seen_ms: now() - 5 * 60_000,
        last_seen_ms: now() - 15_000,
        query_count: 14,
        mutation: { observed_at_ms: now() - 6_000 },
        task_ref: 'edit the store too',
      },
    ],
    collisions: [
      { brain_root: '/work/repo-alpha', caller_root: wt, agent_ids: ['atlas', 'beacon'], reason: 'same_worktree' },
    ],
  }));
  await openHall(page);

  const collision = page.locator('[data-role="presence-collision"]');
  await expect(collision).toHaveCount(1);
  await expect(collision).toHaveAttribute('data-collision-reason', 'same_worktree');
  await expect(collision).toContainText('2 agents writing in the same worktree — atlas, beacon · lane-1');
  // It is an inline notice, NOT a modal — no dialog, no full-screen overlay.
  await expect(page.locator('[role="dialog"]')).toHaveCount(0);
  await expect(page.locator('[data-role="presence-strip"]')).toBeVisible();

  await page.screenshot({ path: `${ARTIFACTS}/presence-02-collision-amber.png`, fullPage: true });
});

// ── 3. the honest empty state ─────────────────────────────────────────────────
test('an empty roster reads an honest sentence, not a blank', async ({ page }) => {
  await mockOwner(page, () => ({ presences: [], collisions: [] }));
  await openHall(page);

  const strip = page.locator('[data-role="presence-strip"]');
  await expect(strip.locator('[data-role="presence-empty"]')).toContainText(
    'No agents visible to m1nd right now',
  );
  await expect(strip.locator('[data-role="presence-count"]')).toHaveText('0');
  await expect(strip.locator('[data-role="presence-limitation"]')).toBeVisible();
  await expect(strip.locator('[data-role="presence-row"]')).toHaveCount(0);

  await page.screenshot({ path: `${ARTIFACTS}/presence-03-empty-honest.png`, fullPage: true });
});
