/*
 * SOFT PROOF render smoke — the PRD Slice-0 render gate, honesty invariants at
 * the pixel boundary. Rendered with react-dom/server (no new test deps).
 *
 * Required by the mission:
 *   - a post-it renders author + age
 *   - a stale post-it renders the flipped state
 *   - violet (abstain) appears ONLY on unknown/abstain elements
 * plus INV-02 (abstain never animates, no numeral) and INV-04 (unknown, not faked).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import TrustDot from './TrustDot';
import PostItChip from './PostItChip';
import VerdictChip from './VerdictChip';
import GapCard from './GapCard';

const html = (el: React.ReactElement) => renderToStaticMarkup(el);

/** Decode the few HTML entities react-dom/server emits (e.g. &#x27; for '). */
const decode = (s: string) =>
  s
    .replace(/&#x27;/g, "'")
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"');

/** The visible text of the render (tags + attributes stripped). */
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' '));

// ── Post-it: author + age on the front face (PRD §3.3) ────────────────────────
test('post-it card renders author and age', () => {
  const out = html(
    <PostItChip
      variant="card"
      claim="The snapshot is the single source of tree structure"
      sourceAgent="agent-refactor"
      ageMs={2 * 60 * 60 * 1000}
      state="fresh"
    />,
  );
  assert.match(out, /agent-refactor/, 'author chip present');
  assert.match(out, /2h ago/, 'age chip present');
  assert.match(out, /data-postit-state="fresh"/);
});

// ── INV-04: provenance absent → "unknown", never faked, violet-outlined ───────
test('INV-04: a provenance-less post-it renders unknown, no relative-time string, violet outline', () => {
  const out = html(
    <PostItChip variant="card" claim="legacy claim" sourceAgent={null} ageMs={null} state="unknown" />,
  );
  assert.match(out, /author unknown/);
  assert.match(out, /age unknown/);
  assert.match(out, /data-abstain="true"/, 'unknown post-it is abstain-class (violet)');
  // No fabricated relative time.
  assert.doesNotMatch(out, /\d+[smhd] ago/);
  assert.doesNotMatch(out, /\bnow\b/);
});

// ── Stale post-it: flipped face-down, "the code this cites changed" ───────────
test('stale post-it renders the flipped state, not the raw claim', () => {
  const el = (
    <PostItChip
      variant="chip"
      claim="a claim whose code moved"
      sourceAgent="agent-audit"
      ageMs={1000}
      state="stale"
    />
  );
  const out = html(el);
  assert.match(out, /data-postit-state="stale"/);
  // The visible body shows the flip label — not the original claim text (which
  // legitimately remains in the title tooltip for accessibility).
  const bodyMatch = out.match(/<span class="text-\[11px\][^"]*"[^>]*>([^<]*)<\/span>/);
  assert.ok(bodyMatch, 'chip has a body span');
  assert.equal(bodyMatch![1], 'the code this cites changed');
  assert.notEqual(bodyMatch![1], 'a claim whose code moved');
});

// ── INV-02: abstain never animates and carries no numeral ─────────────────────
test('INV-02: abstain verdict chip has no animation class and no numeral', () => {
  const el = <VerdictChip verdict="abstain" />;
  const out = html(el);
  assert.match(out, /data-abstain="true"/);
  assert.doesNotMatch(out, /animate|tremor-breath/, 'abstain must not animate');
  // No confidence numeral in the VISIBLE text (HTML entities like &#x27; decoded).
  assert.doesNotMatch(visibleText(el), /\d/, 'abstain chip carries no confidence numeral');
});

test('INV-02: the insufficient_evidence trust dot is abstain-class and never breathes', () => {
  // Even when asked to breathe, the unknown dot must not.
  const out = html(<TrustDot band="insufficient_evidence" breathing />);
  assert.match(out, /data-abstain="true"/);
  assert.doesNotMatch(out, /tremor-breath/, 'unknown dot must never carry the breath');
  // A non-abstain churning dot may breathe.
  const low = html(<TrustDot band="low" breathing />);
  assert.match(low, /tremor-breath/);
  assert.doesNotMatch(low, /data-abstain/);
});

// ── Violet quarantine at render: only abstain/unknown elements carry it ───────
test('violet (iris/abstain) marker appears ONLY on unknown/abstain renders', () => {
  // Non-abstain surfaces: no data-abstain.
  assert.doesNotMatch(html(<TrustDot band="low" />), /data-abstain/);
  assert.doesNotMatch(html(<TrustDot band="medium" />), /data-abstain/);
  assert.doesNotMatch(html(<VerdictChip verdict="act" />), /data-abstain/);
  assert.doesNotMatch(html(<VerdictChip verdict="reverify" />), /data-abstain/);
  assert.doesNotMatch(
    html(<PostItChip variant="chip" claim="x" sourceAgent="a" ageMs={1} state="fresh" />),
    /data-abstain/,
  );
  // Abstain surfaces: they DO carry it.
  assert.match(html(<TrustDot band="insufficient_evidence" />), /data-abstain="true"/);
  assert.match(html(<VerdictChip verdict="abstain" />), /data-abstain="true"/);
  assert.match(html(<GapCard gap="No durable memory for foo yet" />), /data-abstain="true"/);
});

// ── INV-07: a gap card renders exactly one action affordance ──────────────────
test('INV-07: gap card renders action language + exactly one action button', () => {
  const el = <GapCard gap="The graph is empty or unbound — no codebase context is available" />;
  const out = html(el);
  assert.match(visibleText(el), /I haven't read this repo yet/);
  const buttons = out.match(/data-role="gap-action"/g) ?? [];
  assert.equal(buttons.length, 1, 'exactly one action affordance');
  // No epistemic vocabulary in the rendered gap.
  assert.doesNotMatch(visibleText(el).toLowerCase(), /binding|calibration|closure|abstain|provenance|epistemic/);
});
