/*
 * ACCESSIBILITY — the m1nd shell's browser smoke, reported as its OWN proof.
 *
 * Why a separate lane (docs/benchmarks/G7-LIVE-CEREMONY.md §5): the PRD asks for
 * "UI unit · accessibility · browser fixture · browser LIVE" as FOUR separate
 * proofs. The ceremony measured the accessibility one at a literal zero — no
 * role/name assertion anywhere in `e2e/`, no axe dependency. G7 counts this lane
 * separately or not at all, so it lives in its own directory, under its own
 * config, behind its own npm script (`npm run test:e2e:a11y`), on its own CI
 * step. Folding it into `e2e/` would have closed nothing.
 *
 * WHAT THIS SMOKE CLAIMS — four things, in a real browser against the real bundle:
 *  1. the shell's landmarks exist and are UNIQUE (one banner, one navigation, one
 *     main), so a screen-reader user can jump to regions instead of reading chrome;
 *  2. every control the a11y tree exposes carries a non-empty accessible name —
 *     no nameless button anywhere on the surfaces walked;
 *  3. the door bar says WHERE YOU ARE (`aria-current="page"`), exactly once, on
 *     the door of the surface in view — not by which button happens to be missing;
 *  4. the primary flow (Universe L0 → open a world → its room) is walkable by
 *     KEYBOARD ALONE. This spec never calls `.click()`.
 *
 * WHAT IT DOES **NOT** CLAIM. This is a smoke, not a WCAG conformance audit and
 * not an axe rule sweep (the axe question was decided against adding a dependency
 * — see the CI step's comment). It says nothing about colour contrast, focus-ring
 * visibility, reading order, motion preferences, zoom/reflow, screen-reader
 * announcement quality, or the surfaces it does not walk (Threshold, Hall, tree
 * drawers, modals). A green run here means the shell's structure is sound on the
 * walked path — it does not mean the product is accessible.
 *
 * Like the rest of the fixture lanes it boots its OWN Vite dev server on a private
 * port and mocks every /api/* route IN-PAGE — nothing reaches a live owner (the
 * served :1338 is out of bounds for tests, AGENTS.md). Neutral fixtures only.
 */
import { expect, test, type Page, type Route } from '@playwright/test';

const now = () => Date.now();
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

// ── neutral owner envelopes (example names only, no-leak law) ────────────────
const instance = {
  instance_id: 'inst_e2e_a11y',
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

/** One world — enough for the Universe L0 to lead and for a room to open. */
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

function mockOwner(page: Page) {
  const universe = universeWithWorld();
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
        if (path === '/api/tools')
          return await json(route, { tools: [], rest_brain_selector: true });
        if (path === '/api/mailbox') return await json(route, { missions: [] });
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(route, { result: { present: false, honest: 'no skeleton yet' } });
        if (path === '/api/graph/snapshot')
          return await json(route, { version: 1, nodes: [], edges: [] });
        if (path === '/api/graph/stats')
          return await json(route, { node_count: 3210, edge_count: 9000 });
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

/** Roles that a human must be able to tell apart by ear. A control the a11y tree
 *  exposes under one of these with no name is a defect — "button" announced with
 *  nothing after it is a dead end for anyone not looking at the pixels. */
const NAMED_ROLES = ['button', 'link', 'textbox', 'checkbox', 'combobox', 'tab'] as const;

/** Playwright's own role + accessible-name engine on both sides of the count, so
 *  the comparison is apples to apples: every exposed control must also match a
 *  name containing at least one non-space character. */
async function expectEveryControlNamed(page: Page, where: string) {
  const tree = await page.locator('body').ariaSnapshot();
  for (const role of NAMED_ROLES) {
    const exposed = await page.getByRole(role).count();
    const named = await page.getByRole(role, { name: /\S/ }).count();
    expect(
      named,
      `${where}: ${exposed - named} of ${exposed} "${role}" control(s) reach the a11y tree with NO accessible name.\nThe tree as rendered:\n${tree}`,
    ).toBe(exposed);
  }
}

/** Tab from the document start until `selector` holds focus. Bounded, so a
 *  keyboard trap or an unreachable control fails loudly instead of hanging. */
async function tabUntilFocused(page: Page, selector: string, maxTabs = 25) {
  for (let i = 0; i < maxTabs; i += 1) {
    await page.keyboard.press('Tab');
    const reached = await page.evaluate((sel) => {
      const active = document.activeElement;
      return active instanceof Element && active.closest(sel) != null;
    }, selector);
    if (reached) return i + 1;
  }
  throw new Error(`keyboard reach FAILED: ${selector} never took focus within ${maxTabs} tabs`);
}

// ── 1. landmarks: present, named, and exactly one of each ────────────────────

test('the shell exposes one banner, one navigation and one main landmark', async ({ page }) => {
  await mockOwner(page);
  await page.goto('/');

  await expect(page.getByRole('main', { name: 'm1nd workspace' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'm1nd surfaces' })).toBeVisible();
  await expect(page.getByRole('banner')).toBeVisible();

  // Uniqueness is the half that rots first: a second <main> or a duplicated nav
  // sends "jump to main content" somewhere arbitrary.
  await expect(page.getByRole('banner')).toHaveCount(1);
  await expect(page.getByRole('navigation')).toHaveCount(1);
  await expect(page.getByRole('main')).toHaveCount(1);
});

// ── 2. every exposed control carries an accessible name ─────────────────────

test('every control on the Universe L0 and on the Build Map has an accessible name', async ({
  page,
}) => {
  await mockOwner(page);
  await page.goto('/');

  await expect(page.locator('[data-role="universe"]')).toBeVisible();
  await expectEveryControlNamed(page, 'the Universe L0');

  // The same question on a second, structurally different surface — reached the
  // way a keyboard user would reach it.
  const mapDoor = page.getByRole('button', { name: 'Build Map' });
  await mapDoor.focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('[data-role="build-map-empty"]')).toBeVisible();
  await expectEveryControlNamed(page, 'the Build Map');
});

// ── 3. the door bar answers "where am I?" programmatically ──────────────────

test('the door bar marks exactly one current door, and it is the surface in view', async ({
  page,
}) => {
  await mockOwner(page);
  await page.goto('/');

  const nav = page.getByRole('navigation', { name: 'm1nd surfaces' });
  const current = nav.locator('[aria-current="page"]');

  await expect(page.locator('[data-role="universe"]')).toBeVisible();
  await expect(current).toHaveCount(1);
  await expect(current).toHaveAttribute('data-role', 'open-universe');

  const mapDoor = page.getByRole('button', { name: 'Build Map' });
  await mapDoor.focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('[data-role="build-map-empty"]')).toBeVisible();

  // The mark MOVED with the surface — a door bar that always says the same thing
  // is worse than one that says nothing.
  await expect(current).toHaveCount(1);
  await expect(current).toHaveAttribute('data-role', 'open-map');
});

// ── 4. the primary flow walks on the keyboard alone ─────────────────────────

test('the primary flow — L0 → open a world → its room — is reachable by keyboard alone', async ({
  page,
}) => {
  await mockOwner(page);
  await page.goto('/');
  await expect(page.locator('[data-role="universe"]')).toBeVisible();

  // No mouse anywhere in this test: Tab until the world disc holds focus, then
  // open it the way a keyboard user does.
  await tabUntilFocused(page, '[data-role="universe-world"]');
  await expect(page.locator('[data-world-name="repo-alpha"]')).toBeFocused();
  await page.keyboard.press('Enter');

  // The room opened: the L0 is gone and the working surface's tray is up.
  await expect(page.locator('[data-role="universe"]')).toHaveCount(0);
  await expect(page.locator('[data-role="mission-tray"]')).toBeVisible();

  // …and the way home is on the keyboard too.
  const home = page.getByRole('button', { name: 'm1nd' });
  await home.focus();
  await expect(home).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('[data-role="universe"]')).toBeVisible();
});
