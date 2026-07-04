/*
 * TreeDrawer — the memory/feedback glossary at the pixel boundary (PRD §4A.3.1).
 *
 * UNIT C, field-diagnosed: the learn-history chip said "I haven't seen evidence
 * either way yet". But `evidence:` is L1GHT's word for MEMORY anchors — the panel
 * right below the chip. The collision read as "no memories" and confused a live
 * reader (Max). These tests pin the distinction permanently:
 *   - the FEEDBACK chip/line never contains the word "evidence";
 *   - the MEMORIES panel never contains the word "feedback";
 *   - the no-history copy is the new sentence, and each band has an honest variant.
 * Rendered with react-dom/server (no new deps), the repo's pattern.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import TreeDrawer, { feedbackLine } from './TreeDrawer';
import type { TreeRow } from '../../lib/tree';
import type { TrustBand } from '../../lib/softProof';

const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const noop = () => {};

function row(overrides: Partial<TreeRow> = {}): TreeRow {
  return {
    id: 'n1',
    name: 'auth.rs',
    kind: 'file',
    externalId: 'repo::file::src/auth.rs',
    path: 'src/auth.rs',
    depth: 1,
    children: [],
    postIts: [],
    subtreePostItCount: 0,
    mapped: true,
    ...overrides,
  };
}

// ── The pure feedback line — the human wording of learn history ───────────────

test('§4A.3.1: feedbackLine never says "evidence" (that word is memory\'s), for every band', () => {
  for (const band of ['low', 'medium', 'high', 'insufficient_evidence'] as TrustBand[]) {
    const line = feedbackLine(band);
    assert.doesNotMatch(line, /evidence/i, `the feedback line for "${band}" must not say "evidence": ${line}`);
    assert.ok(line.length > 0);
  }
});

test('§4A.3.1: the no-history feedback line is the new copy — feedback, confirmed/corrected, not evidence', () => {
  const line = feedbackLine('insufficient_evidence');
  assert.match(line, /no feedback yet/);
  assert.match(line, /confirmed or corrected/);
  assert.doesNotMatch(line, /evidence/i);
});

test('§4A.3.1: each history band has an honest confirmed / corrected / mixed variant', () => {
  assert.match(feedbackLine('low'), /confirmed/);
  assert.match(feedbackLine('high'), /corrected/);
  assert.match(feedbackLine('medium'), /confirmed and corrected/);
});

// ── The rendered drawer — the two panels stay in their lanes ──────────────────

test('§4A.3.1: the drawer FEEDBACK area never contains "evidence" (no-history case)', () => {
  const out = decode(html(<TreeDrawer row={row()} band="insufficient_evidence" onClose={noop} />));
  // The feedback line renders and carries the new copy.
  assert.match(out, /no feedback yet — no agent has confirmed or corrected/);
  assert.doesNotMatch(out, /evidence/i, 'the drawer must not surface the retired "evidence" copy');
});

test('§4A.3.1: the MEMORIES panel names memories, never "feedback"', () => {
  const out = decode(html(<TreeDrawer row={row()} band="low" onClose={noop} />));
  // The memories header is present and about memories anchored to the row…
  assert.match(out, /memories anchored here/);
  // …and the memories region never borrows the word "feedback" (glossary line).
  // (The feedback chip lives in the header; the memories panel copy is distinct.)
  const memoriesRegion = out.slice(out.indexOf('memories anchored here'));
  assert.doesNotMatch(memoriesRegion, /feedback/i, 'the memories panel must not say "feedback"');
});

test('§4A.3.1: the feedback line data-role is present and pinned to the header', () => {
  const out = html(<TreeDrawer row={row()} band="high" onClose={noop} />);
  assert.match(out, /data-role="feedback-line"/);
  assert.match(decode(out), /agents have corrected answers about this file/);
});
