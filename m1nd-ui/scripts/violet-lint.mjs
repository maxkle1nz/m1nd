#!/usr/bin/env node
/*
 * violet-lint — the SOFT PROOF violet quarantine (HUMAN-LAYER-PRD §6.2).
 *
 * "The abstain wears the brand color, and nothing else wears it."
 *
 * Fails CI on:
 *   (a) any RAW violet hex (#7C3AED / #A78BFA / #EDE9FE / #4C1D95, any case)
 *       appearing outside the two sanctioned token-definition files; and
 *   (b) any reference to the `iris` Tailwind family (bg-iris, text-iris,
 *       border-iris, ring-iris, iris-veil, iris-wisteria, iris-deep, …) from a
 *       source file that is NOT on the abstain/unknown allow-list.
 *
 * Also bans the retired cyberpunk palette outright (§6.1): cyans, neon green,
 * blue-black substrates, saturated alarm red, light-emission gradients.
 *
 * Zero dependencies — walks src/ with fs. Exit 1 on any violation.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const UI_ROOT = join(HERE, '..');
const SRC = join(UI_ROOT, 'src');

// Files permitted to DEFINE the violet hex values: the token sources, plus the
// one sanctioned semantics module that decides the abstain→violet mapping.
const TOKEN_FILES = new Set(
  ['src/index.css', 'tailwind.config.ts', 'src/lib/softProof.ts'].map((p) =>
    p.split('/').join(sep),
  ),
);

// Deferred legacy surfaces — the force-directed map + instances panel. These are
// NOT part of Slice 0 (they are tree-shaken out of the shipped bundle) and carry
// the retired cyberpunk palette until the PRD §8 Slice 3 re-skin mounts them at
// rung 2. Explicitly exempt so the Slice-0 quarantine over the NEW surface is
// enforceable now; removing an entry here is the Slice-3 acceptance signal.
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

// Components allowed to REFERENCE the `iris` Tailwind family. These are the
// abstain / honest-unknown surfaces from §6.2. Everything else must be violet-free.
const ABSTAIN_ALLOW = new Set(
  [
    'src/components/soft/VerdictChip.tsx',
    'src/components/soft/TrustDot.tsx',
    'src/components/soft/PostItChip.tsx',
    'src/components/soft/GapCard.tsx',
    'src/components/soft/FreshnessBanner.tsx',
    'src/lib/softProof.ts',
  ].map((p) => p.split('/').join(sep)),
);

// Raw violet hexes that may only appear in TOKEN_FILES.
const VIOLET_HEX = /#(?:7c3aed|a78bfa|ede9fe|4c1d95)\b/i;

// The `iris` Tailwind family, referenced as utility classes or the raw name.
// Matches: bg-iris, text-iris, border-iris-veil, ring-iris/40, iris-wisteria, etc.
const IRIS_CLASS = /\b(?:bg|text|border|ring|from|to|via|fill|stroke|outline|shadow|decoration|divide|accent|caret)-iris(?:-[a-z]+)?\b|\biris-(?:wisteria|veil|deep)\b/;

// Retired cyberpunk palette — banned everywhere (§6.1).
const BANNED_HEX = /#(?:00f5ff|00ffff|0ff|00ff88|050814|09090b|0c0c10)\b/i;
const BANNED_CYAN_NAMED = /\b(?:bg|text|border|ring)-(?:cyan|teal|fuchsia|sky)-\d/;

const violations = [];

function walk(dir) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      if (name === 'node_modules' || name === 'dist' || name === '__fixtures__') continue;
      walk(full);
      continue;
    }
    if (!/\.(tsx?|css)$/.test(name)) continue;
    if (name.endsWith('.test.ts') || name.endsWith('.test.tsx')) continue;
    lintFile(full);
  }
}

function lintFile(full) {
  const rel = relative(UI_ROOT, full);
  if (DEFERRED_LEGACY.has(rel)) return; // Slice-3 debt, see note above.
  const text = readFileSync(full, 'utf8');
  const lines = text.split('\n');
  lines.forEach((line, i) => {
    const ln = i + 1;
    // Strip line comments so the doc-strings in this policy don't self-trip.
    const code = line.replace(/\/\/.*$/, '');

    if (VIOLET_HEX.test(code) && !TOKEN_FILES.has(rel)) {
      violations.push(`${rel}:${ln} — raw violet hex outside token files: ${line.trim()}`);
    }
    if (IRIS_CLASS.test(code) && !ABSTAIN_ALLOW.has(rel) && !TOKEN_FILES.has(rel)) {
      violations.push(
        `${rel}:${ln} — 'iris' (violet) class used by a non-abstain component: ${line.trim()}`,
      );
    }
    if (BANNED_HEX.test(code)) {
      violations.push(`${rel}:${ln} — banned cyberpunk hex (retired palette): ${line.trim()}`);
    }
    if (BANNED_CYAN_NAMED.test(code)) {
      violations.push(`${rel}:${ln} — banned neon/cyan Tailwind color: ${line.trim()}`);
    }
  });
}

walk(SRC);

if (violations.length > 0) {
  console.error('✗ violet-lint FAILED — the violet quarantine is broken:\n');
  for (const v of violations) console.error('  ' + v);
  console.error(
    `\n${violations.length} violation(s). Violet (#7C3AED family / iris-* classes) is reserved` +
      ' for abstain / insufficient_evidence surfaces only (PRD §6.2).',
  );
  process.exit(1);
}

console.log('✓ violet-lint passed — violet is quarantined to abstain/unknown surfaces only.');
