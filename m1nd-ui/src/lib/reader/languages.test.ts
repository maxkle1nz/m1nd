/*
 * reader/languages — the ext→grammar map + the per-language click-to-def policy.
 * The teeth: unknown extensions degrade to plain text honestly, and call edges are
 * trusted for Rust only (the PATHOS gap encoded as data).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { extOf, languageForPath, relationClass, relationNavigable } from './languages';

test('extOf reads the lowercase extension (no dot), or empty', () => {
  assert.equal(extOf('a/b/graph.rs'), 'rs');
  assert.equal(extOf('Panel.TSX'), 'tsx');
  assert.equal(extOf('Makefile'), '');
  assert.equal(extOf('a/.gitignore'), ''); // dotfile: no extension
  assert.equal(extOf('deep/mod.rs'), 'rs');
});

test('languageForPath maps the v1 grammars (rust/ts/tsx/js/py/go/json/md/toml/bash)', () => {
  assert.equal(languageForPath('src/graph.rs').shikiLang, 'rust');
  assert.equal(languageForPath('ui/panel.ts').shikiLang, 'typescript');
  assert.equal(languageForPath('ui/panel.tsx').shikiLang, 'tsx');
  assert.equal(languageForPath('x.js').shikiLang, 'javascript');
  assert.equal(languageForPath('x.py').shikiLang, 'python');
  assert.equal(languageForPath('x.go').shikiLang, 'go');
  assert.equal(languageForPath('x.json').shikiLang, 'json');
  assert.equal(languageForPath('README.md').shikiLang, 'markdown');
  assert.equal(languageForPath('Cargo.toml').shikiLang, 'toml');
  assert.equal(languageForPath('run.sh').shikiLang, 'bash');
});

test('an unknown extension degrades to plain text, honestly (never a fake grammar)', () => {
  const p = languageForPath('data.parquet');
  assert.equal(p.shikiLang, null);
  assert.equal(p.label, 'plain text');
  assert.equal(languageForPath(null).shikiLang, null);
  assert.equal(languageForPath('').shikiLang, null);
});

test('call edges are trusted for Rust only (the honest per-language degradation)', () => {
  assert.equal(languageForPath('a.rs').callEdgesTrusted, true);
  for (const p of ['a.ts', 'a.tsx', 'a.js', 'a.py', 'a.go']) {
    assert.equal(languageForPath(p).callEdgesTrusted, false, `${p} does not trust call edges (PATHOS gap)`);
  }
});

test('relationClass folds synonyms into def / call / other', () => {
  assert.equal(relationClass('def'), 'def');
  assert.equal(relationClass('IMPORT'), 'def');
  assert.equal(relationClass('uses'), 'def');
  assert.equal(relationClass('call'), 'call');
  assert.equal(relationClass('method_call'), 'call');
  assert.equal(relationClass('contains'), 'other');
  assert.equal(relationClass('has_state'), 'other');
});

test('relationNavigable: def navigates everywhere; call only where call edges are trusted', () => {
  const rust = languageForPath('a.rs');
  const ts = languageForPath('a.ts');
  // def-class: navigable in both.
  assert.equal(relationNavigable('def', rust), true);
  assert.equal(relationNavigable('import', ts), true);
  // call-class: Rust yes, TS no.
  assert.equal(relationNavigable('call', rust), true);
  assert.equal(relationNavigable('call', ts), false);
  // other-class: never.
  assert.equal(relationNavigable('contains', rust), false);
});
