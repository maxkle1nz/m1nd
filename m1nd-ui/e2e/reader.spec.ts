/*
 * The advanced code reader — the browser proof (donor dossier "Fatia 1"; the
 * uiproof standard: deterministic `npx playwright test`, no agent in the loop).
 *
 * What is proven, in a REAL browser against the real app bundle (Shiki paints
 * client-side; every /api/* call is mocked in-page — nothing reaches a live owner):
 *   1. opening a member file shows real syntax HIGHLIGHT (paper/ink tokens, never a
 *      neon/violet color) over graph-driven line rows + the language pill;
 *   2. the graph-derived OUTLINE is clickable and scrolls to the symbol's line;
 *   3. click-to-DEFINITION follows a real edge across files and the breadcrumb
 *      returns (Rust call-edge jump);
 *   4. the honest ABSTAIN ("no target") and the ambiguous CANDIDATE list render;
 *   5. a graph FOLD range collapses the symbol's body.
 */
import { expect, test, type Page, type Route } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const readerSnapshot = JSON.parse(
  readFileSync(join(HERE, '..', 'src', '__fixtures__', 'reader_snapshot.json'), 'utf8'),
);

const NODE_COUNT = readerSnapshot.nodes.length;

const instance = {
  instance_id: 'inst_e2e_reader',
  workspace_root: '/work/repo-alpha',
  runtime_root: '/work/repo-alpha/.m1nd',
  graph_source: 'ingest',
  plasticity_state: 'stable',
  pid: 4243,
  bind: null,
  port: null,
  started_at_ms: 1_700_000_000_000,
  last_heartbeat_ms: Date.now(),
  mode: 'serve',
  status: 'running',
  owner_live: true,
  stale: false,
  conflicts: [],
};

const selfEnvelope = {
  instance,
  graph_state: {
    node_count: NODE_COUNT,
    edge_count: readerSnapshot.edges.length,
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
  edge_count: readerSnapshot.edges.length,
  queries_processed: 0,
  agent_sessions: [],
  domain: 'code',
  graph_generation: 1,
  plasticity_generation: 1,
};

/** One ratified block whose membership holds both real code files. */
const block = {
  block_id: 'sb_repo_alpha_reader',
  name: 'Reader Core',
  purpose: 'the store + its graph',
  kind: 'scanned',
  state: 'ratified',
  boundary_version: 1,
  contract_version: 1,
  membership_source: 'ratified',
  membership: [
    { path: 'repo-alpha/src/store.rs', role: 'primary' },
    { path: 'repo-alpha/src/graph.rs', role: 'primary' },
  ],
  sockets: { inputs: [], outputs: [], external: [] },
  receipt_contract: { version: 1, required: [], optional: [], waived: [], declared_by: null, declared_at: null },
  receipts: [],
  layout: { x: null, y: null, locked: false, algorithm_seed: null, version: 1 },
  unmapped_residue: [],
};

const blockStore = {
  schema: 'm1nd-system-block-store-v0',
  store_version: 3,
  skeleton: {
    skeleton_id: 'sk_repo_alpha_seed_2026_07',
    version: 1,
    state: 'ratified',
    ratification: { method: 'human-ui', ratifier: 'owner', ratified_at: '2026-07-12T00:00:00Z', commit: '' },
  },
  blocks: [block],
  unmapped_policy: { visible: true, default_action: 'leave_unmapped_until_ratified' },
};

// Real code so Shiki has keywords to paint and the line spans line up with the
// fixture's provenance (store.rs: Store@5 open@14 save@21 close@29; graph.rs:
// Kind@3 Graph@9 insert@18 validate@26 Node@33 insert@39).
const STORE_RS = `// repo-alpha store — a tiny graph-backed store.
use crate::graph::Graph;
use std::io;

pub struct Store {
    graph: Graph,
    path: String,
    dirty: bool,
    revision: u32,
    capacity: usize,
    name: String,
}

pub fn open(path: &str) -> Store {
    let graph = Graph::new();
    let mut store = Store { graph, path: path.into(), dirty: false, revision: 0, capacity: 16, name: String::new() };
    store.graph.insert(0);
    store
}

pub fn save(store: &mut Store) -> io::Result<()> {
    store.graph.insert(store.revision);
    store.revision += 1;
    store.dirty = false;
    let _ = &store.name;
    Ok(())
}

pub fn close(store: Store) {
    let file = std::fs::File::open(&store.path);
    drop(file);
    drop(store);
}
`;

const GRAPH_RS = `// repo-alpha graph — the core structure.

pub enum Kind {
    Leaf,
    Branch,
}


pub struct Graph {
    nodes: Vec<Node>,
    edges: usize,
    root: Option<usize>,
    kind: Kind,
    dirty: bool,
    name: String,
}

pub fn insert(g: &mut Graph, id: usize) {
    validate(id);
    g.edges += 1;
    g.nodes.push(Node::new(id));
    g.dirty = true;
    let _ = &g.name;
}

fn validate(id: usize) -> bool {
    let ok = id < 1024;
    let _ = ok;
    ok
}


pub struct Node {
    id: usize,
    weight: f64,
    label: String,
}

pub fn insert(n: &mut Node, w: f64) {
    n.weight = w;
    let _ = &n.label;
    let _ = n.id;
}
`;

const FILES: Record<string, string> = {
  'repo-alpha/src/store.rs': STORE_RS,
  'repo-alpha/src/graph.rs': GRAPH_RS,
};

const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

/** Wire the whole mocked owner — the reader's graph, its ratified map, and the two
 *  member files. Only /api/* is intercepted; every dev-server asset (incl. the lazy
 *  Shiki chunks) passes through so the highlight is REAL. */
function mockOwner(page: Page) {
  return page.route(
    (url) => url.pathname.startsWith('/api/'),
    async (route) => {
      const u = new URL(route.request().url());
      const path = u.pathname;
      try {
        if (path === '/api/health') return await json(route, health);
        if (path === '/api/instance/self') return await json(route, selfEnvelope);
        if (path === '/api/instances') return await json(route, { instances: [instance] });
        if (path === '/api/graph/snapshot') return await json(route, readerSnapshot);
        if (path === '/api/graph/stats')
          return await json(route, { node_count: NODE_COUNT, edge_count: readerSnapshot.edges.length });
        if (path === '/api/runnerd/status') return await json(route, { runners: [] });
        if (path === '/api/mailbox') return await json(route, { missions: [] });
        if (path === '/api/events')
          return await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
        if (path === '/api/file') {
          const p = u.searchParams.get('path') ?? '';
          const content = FILES[p];
          if (content == null) return await json(route, { error: 'not_found', message: `no ${p}` }, 404);
          return await json(route, { path: p, content, bytes: content.length, truncated: false, max_bytes: 262144 });
        }
        if (path === '/api/tools/system_blocks_snapshot')
          return await json(route, {
            result: { present: true, store_version: 3, block_count: 1, store: blockStore },
          });
        if (path.startsWith('/api/tools/')) return await json(route, { result: {} });
        return await json(route, {});
      } catch {
        /* the page may abort a request mid-flight — ignore */
      }
    },
  );
}

/** Land on the reader with `path` open. A ratified skeleton auto-leads the shell to
 *  the Build Map (App.tsx: the map is the front door when a ratified skeleton is
 *  present), so the block card is already rendered — click it → Show Code → select
 *  the file in the Files-by-role rail. */
async function openReaderOn(page: Page, path: string) {
  await page.goto('/');
  await page.locator('[data-role="block-card"]').first().click();
  // The panel button and the modal share data-role="show-code" (app convention) —
  // click the BUTTON, assert the MODAL by its unique data-block-showcode.
  await page.locator('button[data-role="show-code"]').click();
  await expect(page.locator('[data-block-showcode]')).toBeVisible();
  await page.locator(`[data-role="file-path"][data-path="${path}"]`).click();
  await expect(page.locator('[data-role="reader-code"]')).toBeVisible();
}

const ARTIFACTS = 'e2e/artifacts';

test('a member file opens with REAL paper/ink highlight, a graph outline and a language pill', async ({
  page,
}) => {
  await mockOwner(page);
  await openReaderOn(page, 'repo-alpha/src/store.rs');

  await expect(page.locator('[data-role="reader-langpill"]')).toHaveText('Rust');

  // The outline is the graph's symbols, in line order.
  const outline = page.locator('[data-role="symbol-outline"]');
  await expect(outline).toBeVisible();
  await expect(outline.locator('[data-role="outline-entry"]')).toHaveCount(4); // Store/open/save/close
  await expect(page.locator('[data-role="freshness-dot"]').first()).toBeVisible();

  // The paint is REAL: colored token spans appear once Shiki's lazy chunk loads.
  const colored = page.locator('[data-role="reader-code"] span[style*="color"]');
  await expect(colored.first()).toBeVisible({ timeout: 15_000 });
  const colors = await page.$$eval('[data-role="reader-code"] span[style*="color"]', (els) =>
    (els as HTMLElement[]).map((e) => e.style.color).filter(Boolean),
  );
  expect(colors.length).toBeGreaterThan(0);
  // Nothing glows: no violet (rgb 124,58,237) and no neon cyan/green in the paint.
  for (const c of colors) {
    expect(c.replace(/\s/g, '')).not.toMatch(/124,58,237|0,245,255|0,255,136|0,255,255/);
  }
  await page.screenshot({ path: `${ARTIFACTS}/reader-01-highlight-outline.png`, fullPage: true });
});

test('clicking an outline symbol scrolls the code to its line (graph-driven navigation)', async ({
  page,
}) => {
  await mockOwner(page);
  await openReaderOn(page, 'repo-alpha/src/store.rs');

  // `save` is at line 21 — click it, the line becomes the current row.
  const saveEntry = page.locator('[data-role="outline-entry"][data-symbol-line="21"]');
  await saveEntry.locator('[data-role="outline-goto"]').click();
  await expect(page.locator('[data-role="code-line"][data-line="21"]')).toHaveAttribute('data-current', 'true');
});

test('click-to-definition follows a Rust call-edge across files, and back returns', async ({ page }) => {
  await mockOwner(page);
  await openReaderOn(page, 'repo-alpha/src/store.rs');

  // `open` (line 14) calls Graph::insert — a single grounded jump into graph.rs:18.
  const openEntry = page.locator('[data-role="outline-entry"][data-symbol-line="14"]');
  await openEntry.locator('[data-role="def-jump"]').click();

  await expect(page.locator('[data-role="reader-path"]')).toHaveText('repo-alpha/src/graph.rs');
  await expect(page.locator('[data-role="reader-breadcrumb"]')).toBeVisible();
  await expect(page.locator('[data-role="code-line"][data-line="18"]')).toHaveAttribute('data-current', 'true');
  await page.screenshot({ path: `${ARTIFACTS}/reader-02-jumped-to-def.png`, fullPage: true });

  // Back returns to store.rs (the breadcrumb closes).
  await page.locator('[data-role="reader-back"]').click();
  await expect(page.locator('[data-role="reader-path"]')).toHaveText('repo-alpha/src/store.rs');
  await expect(page.locator('[data-role="reader-breadcrumb"]')).toHaveCount(0);
});

test('honest degradation renders: an ambiguous receiver lists candidates, a dangling ref abstains', async ({
  page,
}) => {
  await mockOwner(page);
  await openReaderOn(page, 'repo-alpha/src/store.rs');

  // `close` (line 29) references std::fs::File — external, no grounded target.
  const closeEntry = page.locator('[data-role="outline-entry"][data-symbol-line="29"]');
  await expect(closeEntry.locator('[data-role="def-abstain"]')).toBeVisible();

  // `save` (line 21) is ambiguous (two same-name `insert` defs) — a candidate list.
  const saveEntry = page.locator('[data-role="outline-entry"][data-symbol-line="21"]');
  await saveEntry.locator('[data-role="def-candidates-toggle"]').click();
  await expect(saveEntry.locator('[data-role="def-candidate"]')).toHaveCount(2);
  await page.screenshot({ path: `${ARTIFACTS}/reader-03-abstain-candidates.png`, fullPage: true });

  // Picking a candidate navigates (honest human choice, never a fabricated jump).
  await saveEntry.locator('[data-role="def-candidate"]').first().click();
  await expect(page.locator('[data-role="reader-path"]')).toHaveText('repo-alpha/src/graph.rs');
});

test('a graph fold range collapses the symbol body (fold ranges from the spans)', async ({ page }) => {
  await mockOwner(page);
  await openReaderOn(page, 'repo-alpha/src/store.rs');

  // The `open` function spans lines 14–19 — fold it from the gutter caret.
  const foldCaret = page.locator('[data-role="code-line"][data-line="14"] [data-role="code-fold"]');
  await expect(foldCaret).toBeVisible();
  await foldCaret.click();
  await expect(page.locator('[data-role="code-fold-placeholder"]').first()).toBeVisible();
  // A hidden interior line (15) is no longer rendered.
  await expect(page.locator('[data-role="code-line"][data-line="15"]')).toHaveCount(0);
});
