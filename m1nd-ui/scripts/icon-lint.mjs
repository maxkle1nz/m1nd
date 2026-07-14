#!/usr/bin/env node
/*
 * icon-lint — the §4A.7 precision-icon guard (HUMAN-LAYER-PRD §4A.7, INV-13).
 *
 * Sibling to violet-lint.mjs (same fs-walk mechanism, zero deps). Enforces the
 * three mechanical icon rules so "one icon per concept" cannot drift:
 *
 *   (a) `lucide-react` is imported by the icon REGISTRY only — any other source
 *       file importing it is a violation. One file maps concept → icon, so the
 *       one-icon-per-concept law is greppable.
 *   (b) `strokeWidth` (or `stroke-width` / `strokeWidth={…}`) is 1.5 everywhere
 *       an icon is drawn — any other value is a violation. (The registry sets it
 *       centrally; this catches a component trying to override it.)
 *   (c) `Sparkles` (and the "AI glitter" decoration family) is banned outright —
 *       named import, JSX use, or bare reference. The seek meaning-mode is a
 *       labeled text toggle, never a magic star.
 *
 * Exit 1 on any violation. Runs in CI next to violet-lint.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const UI_ROOT = join(HERE, '..');
const SRC = join(UI_ROOT, 'src');

// The ONE file allowed to import from 'lucide-react'. Everything else must go
// through the registry (import { Icon, ICON } from '.../lib/icons/registry').
const REGISTRY_FILE = ['src/lib/icons/registry.tsx'].map((p) => p.split('/').join(sep))[0];

// Deferred-legacy surfaces (the force-directed map + its edges/nodes) — the SAME
// set violet-lint exempts. These carry SVG `strokeWidth` for EDGE LINES (React
// Flow), not lucide icons, and are tree-shaken out of the shipped Slice-0 bundle;
// they get re-skinned at the §8 Slice-3 map mount. Icon rules don't apply to a
// surface that draws no registry icons — mirror the violet-lint deferral exactly.
const DEFERRED_LEGACY = new Set(
  [
    'src/components/GraphCanvas.tsx',
    'src/components/DetailPanel.tsx',
    'src/components/InstancesPanel.tsx',
    'src/components/ActivationReplay.tsx',
    'src/components/CommandPalette.tsx',
    'src/components/TopBar.tsx',
    'src/components/edges/GhostEdge.tsx',
    'src/components/edges/WeightedEdge.tsx',
    'src/components/nodes/ClassNode.tsx',
    'src/components/nodes/FileNode.tsx',
    'src/components/nodes/FunctionNode.tsx',
    'src/components/nodes/StructuralHoleNode.tsx',
    'src/lib/colors.ts',
    'src/lib/exportMermaid.ts',
    'src/stores/graphStore.ts',
  ].map((p) => p.split('/').join(sep)),
);

// First-class SVG DATA-VIZ surfaces (F30 Universe canvas): they draw discs,
// satellites, and orbital rings with their OWN measured stroke weights — never a
// registry icon. Rule (b) (icon stroke = 1.5) does not apply to a surface that
// draws no icons — the SAME rationale that exempts the force-directed map above,
// but scoped: rules (a) no-lucide and (c) no-Sparkles STAY enforced here (a viz
// surface must still never import lucide directly nor glitter with Sparkles).
const SVG_GRAPHICS = new Set(
  ['src/components/universe/UniverseView.tsx'].map((p) => p.split('/').join(sep)),
);

// Banned "AI glitter" decoration icons — a promise, not a fact (§4A.7).
const BANNED_ICONS = ['Sparkles', 'Sparkle', 'Stars', 'WandSparkles', 'Wand2'];

// Any import from lucide-react (default, named, namespace, or side-effect).
const LUCIDE_IMPORT = /from\s+['"]lucide-react['"]|require\(\s*['"]lucide-react['"]\s*\)/;

// A strokeWidth assignment whose value is not exactly 1.5. Matches:
//   strokeWidth={2}  strokeWidth="2"  stroke-width="1"  strokeWidth={1.25}
// Allows 1.5 (and 1.50). The registry's `ICON_STROKE = 1.5` constant is exempt
// because it is the single source of the value, asserted by a test.
const STROKE_ANY = /stroke-?[wW]idth\s*[=:]\s*[{"']?\s*([0-9.]+)/g;

const violations = [];

function walk(dir) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      if (name === 'node_modules' || name === 'dist') continue;
      walk(full);
      continue;
    }
    if (!/\.(tsx?)$/.test(name)) continue;
    lintFile(full);
  }
}

/** Blank out /* … *​/ block comments across the whole file so the lint never
 * trips on prose that documents its own rules (e.g. the registry naming the
 * banned icon in a comment). Preserves line numbers by keeping newlines. */
function stripBlockComments(text) {
  return text.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '));
}

function lintFile(full) {
  const rel = relative(UI_ROOT, full);
  if (DEFERRED_LEGACY.has(rel)) return; // Slice-3 debt; not an icon surface.
  const isRegistry = rel === REGISTRY_FILE;
  const isTest = /\.test\.(ts|tsx)$/.test(rel);
  // The lint's own red-case fixtures live under __lint_fixtures__ and MUST trip
  // the rules on demand (a test invokes the linter against them). Skip them in
  // the normal source sweep so they never fail the real gate.
  if (rel.includes('__lint_fixtures__')) return;

  const text = stripBlockComments(readFileSync(full, 'utf8'));
  const lines = text.split('\n');
  lines.forEach((line, i) => {
    const ln = i + 1;
    const code = line.replace(/\/\/.*$/, '');

    // (a) lucide-react imported outside the registry.
    if (LUCIDE_IMPORT.test(code) && !isRegistry) {
      violations.push(
        `${rel}:${ln} — 'lucide-react' imported outside the icon registry (use lib/icons/registry): ${line.trim()}`,
      );
    }

    // (b) strokeWidth ≠ 1.5 (registry ICON_STROKE constant is exempt; tests too,
    //     since a test may assert a specific value in a string; an SVG data-viz
    //     surface draws non-icon strokes and is exempt from THIS rule only).
    if (!isRegistry && !isTest && !SVG_GRAPHICS.has(rel)) {
      let m;
      STROKE_ANY.lastIndex = 0;
      while ((m = STROKE_ANY.exec(code)) != null) {
        const val = parseFloat(m[1]);
        if (val !== 1.5) {
          violations.push(`${rel}:${ln} — strokeWidth ${m[1]} ≠ 1.5 (icons are hairline 1.5): ${line.trim()}`);
        }
      }
    }

    // (c) banned decoration icons — anywhere (import or use) in real source.
    //     Tests are exempt: the icon-lint's OWN test must NAME `Sparkles` in its
    //     assertion strings to prove the guard bites. The real guard is rule (a)
    //     (no lucide import outside the registry) — a test cannot smuggle a real
    //     Sparkles into the bundle without importing it, which (a) already blocks.
    if (!isTest) {
      for (const banned of BANNED_ICONS) {
        const re = new RegExp(`\\b${banned}\\b`);
        if (re.test(code)) {
          violations.push(`${rel}:${ln} — banned decoration icon '${banned}' ("AI glitter", §4A.7): ${line.trim()}`);
        }
      }
    }
  });
}

walk(SRC);

if (violations.length > 0) {
  console.error('✗ icon-lint FAILED — the precision-icon language is broken:\n');
  for (const v of violations) console.error('  ' + v);
  console.error(
    `\n${violations.length} violation(s). Icons import through lib/icons/registry ONLY, ` +
      'stroke is 1.5 everywhere, and Sparkles/decoration is banned (PRD §4A.7).',
  );
  process.exit(1);
}

console.log('✓ icon-lint passed — one icon per concept, stroke 1.5, registry-only imports, no Sparkles.');
