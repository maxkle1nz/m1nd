/*
 * reader/paperInkTheme — "if the highlight looks hacker-neon, the theme is wrong;
 * paper and ink is the law." This proves the theme is built ONLY from the house
 * tokens: calm palette, nothing neon, and NO violet (violet is reserved for
 * abstain, enforced elsewhere by violet-lint — here we prove the theme never even
 * reaches for it).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  BRICK,
  INK,
  INK_SOFT,
  OCHRE,
  PAPER,
  PAPER_INK_PALETTE,
  PAPER_INK_THEME_NAME,
  SAGE,
  SOCKET_BLUE,
  collectThemeColors,
  paperInkTheme,
} from './paperInkTheme';

const VIOLET = /#(?:7c3aed|a78bfa|ede9fe|4c1d95)\b/i; // the quarantined iris family
const NEON = /#(?:00f5ff|00ffff|0ff|00ff88|050814|09090b|0c0c10)\b/i; // retired cyberpunk

test('every color the theme emits is in the calm paper/ink palette (no drift)', () => {
  const allowed = new Set(PAPER_INK_PALETTE);
  for (const c of collectThemeColors()) {
    assert.ok(allowed.has(c), `theme color ${c} is outside the paper/ink palette`);
  }
});

test('the theme reaches for NO violet and NO neon — nothing glows', () => {
  const blob = JSON.stringify(paperInkTheme);
  assert.doesNotMatch(blob, VIOLET, 'violet is reserved for abstain — never in the code theme');
  assert.doesNotMatch(blob, NEON, 'the retired cyberpunk palette is banned');
});

test('the ground is warm-paper and the ink is the house ink', () => {
  assert.equal(paperInkTheme.bg, PAPER);
  assert.equal(paperInkTheme.bg, '#fcfaf6'); // --warm-paper
  assert.equal(paperInkTheme.fg, INK);
  assert.equal(paperInkTheme.fg, '#2b2836'); // --ink
  assert.equal(paperInkTheme.type, 'light');
  assert.equal(paperInkTheme.name, PAPER_INK_THEME_NAME);
});

test('the token mapping is quiet: keywords socket-blue, strings sage, numbers ochre, comments muted italic', () => {
  const scopeSetting = (scope: string) =>
    paperInkTheme.settings.find((s) => s.scope?.includes(scope))?.settings;

  assert.equal(scopeSetting('keyword')?.foreground, SOCKET_BLUE);
  assert.equal(scopeSetting('entity.name.type')?.foreground, SOCKET_BLUE);
  assert.equal(scopeSetting('string')?.foreground, SAGE);
  assert.equal(scopeSetting('constant.numeric')?.foreground, OCHRE);

  const comment = scopeSetting('comment');
  assert.equal(comment?.foreground, INK_SOFT);
  assert.equal(comment?.fontStyle, 'italic');

  // broken/invalid tokens wear the honest brick — a real, in-palette state color.
  assert.equal(scopeSetting('invalid')?.foreground, BRICK);
});
