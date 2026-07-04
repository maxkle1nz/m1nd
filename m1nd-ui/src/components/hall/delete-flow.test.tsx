/*
 * The calm two-step delete — INV-09 proof (HUMAN-LAYER-PRD §4A.4).
 *
 * INV-09: the destructive call NEVER executes with fewer than two distinct
 * confirmations (consequence acknowledge + typed exact name); the server's
 * live-instance refusal renders verbatim; the consequence card always lists what
 * survives beside what dies. Proven three ways:
 *   1) canFireDelete (the structural floor) is false below two confirmations.
 *   2) each step renders statically (react-dom/server) with the right affordances.
 *   3) the live-refusal string is verbatim from the real captured envelope.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ForgetStepView, canFireDelete, liveRefusalLine, type ForgetStep } from './ForgetRuntimeFlow';
import { brainDisplayName, nameMatches } from '../../lib/hallSemantics';
import type { InstanceListResponse } from '../../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) => s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&#x2019;/g, '’');
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' '));
const hasDisabledAttr = (tag: string) => /\sdisabled(=""|>|\s)/.test(tag);
const roleTag = (out: string, role: string) => out.match(new RegExp(`<button[^>]*data-role="${role}"[^>]*>`))?.[0] ?? '';

const list = load<InstanceListResponse>('instances.json');
const dormant = list.instances.find((e) => e.brain_kind == null)!; // shape reused; treated as dormant below
// The delete confirm target is the PROJECT name (§4A.4) — "m1nd", the same name
// the card shows — never the runtime dir the workspace_root would basename to.
const basename = brainDisplayName(dormant);
const noop = () => {};

const view = (over: Partial<React.ComponentProps<typeof ForgetStepView>>) =>
  html(
    <ForgetStepView
      step="idle"
      entry={dormant}
      isLive={false}
      basename={basename}
      typed=""
      matches={false}
      deleting={false}
      refusal={null}
      onReset={noop}
      onContinueToConsequence={noop}
      onContinueToConfirm={noop}
      onTyped={noop}
      onFire={noop}
      {...over}
    />,
  );

// ── INV-09 (1): the structural floor — call unreachable below two confirmations
test('INV-09: canFireDelete is FALSE at idle and at the consequence step (before typing)', () => {
  // idle — the trigger hasn't even been clicked.
  assert.equal(canFireDelete({ step: 'idle', isLive: false, nameMatches: false, deleting: false }), false);
  // consequence acknowledged, but the name not yet typed → still no fire.
  assert.equal(canFireDelete({ step: 'consequence', isLive: false, nameMatches: false, deleting: false }), false);
  assert.equal(canFireDelete({ step: 'consequence', isLive: false, nameMatches: true, deleting: false }), false, 'even a match at the wrong step cannot fire');
});

test('INV-09: canFireDelete requires BOTH the confirm step AND an exact name match', () => {
  // confirm step but no match → no fire.
  assert.equal(canFireDelete({ step: 'confirm', isLive: false, nameMatches: false, deleting: false }), false);
  // confirm step + match → the ONLY firing state.
  assert.equal(canFireDelete({ step: 'confirm', isLive: false, nameMatches: true, deleting: false }), true);
  // a live brain never fires, even with a match.
  assert.equal(canFireDelete({ step: 'confirm', isLive: true, nameMatches: true, deleting: false }), false);
  // mid-flight never double-fires.
  assert.equal(canFireDelete({ step: 'confirm', isLive: false, nameMatches: true, deleting: true }), false);
});

// ── INV-09 (2): the consequence card lists survivors beside what dies ─────────
test('INV-09: the consequence card names what DIES and what SURVIVES', () => {
  const text = visibleText(
    <ForgetStepView
      step="consequence"
      entry={dormant}
      isLive={false}
      basename={basename}
      typed=""
      matches={false}
      deleting={false}
      refusal={null}
      onReset={noop}
      onContinueToConsequence={noop}
      onContinueToConfirm={noop}
      onTyped={noop}
      onFire={noop}
    />,
  );
  assert.match(text, /Dies:/);
  assert.match(text, /Survives:/);
  assert.match(text, /agent-memory\/|brain\.json/, 'survivors name the committed memory');
  assert.match(text, /rebuilds on the next read/, 'the recoverability is stated');
  // The consequence card offers a two-outcome choice, never Yes/No.
  const out = view({ step: 'consequence' });
  assert.match(out, /Keep it/);
  assert.match(out, /Continue/);
});

// ── INV-09 (2): the confirm button is disabled until the typed name matches ───
test('INV-09: the forget button stays DISABLED for empty/partial/wrong names', () => {
  for (const bad of ['', ' ', basename.slice(0, -1), basename.toUpperCase() + 'x']) {
    const matches = nameMatches(bad, basename);
    assert.equal(matches, false, `precondition: "${bad}" must not match`);
    const out = view({ step: 'confirm', typed: bad, matches });
    assert.ok(hasDisabledAttr(roleTag(out, 'forget-runtime')), `forget disabled for "${bad}"`);
  }
});

test('INV-09: the forget button ENABLES only on an exact match', () => {
  const out = view({ step: 'confirm', typed: basename, matches: nameMatches(basename, basename) });
  assert.ok(!hasDisabledAttr(roleTag(out, 'forget-runtime')), 'exact match enables the forget button');
});

// ── INV-09 (3): a live brain gets the refusal verbatim, and NO delete path ────
test('INV-09: a live brain renders the PermissionDenied refusal verbatim and no forget button', () => {
  const out = html(
    <ForgetStepView
      step="consequence"
      entry={dormant}
      isLive
      basename={basename}
      typed=""
      matches={false}
      deleting={false}
      refusal={null}
      onReset={noop}
      onContinueToConsequence={noop}
      onContinueToConfirm={noop}
      onTyped={noop}
      onFire={noop}
    />,
  );
  const expected = liveRefusalLine(dormant.instance_id, dormant.pid);
  assert.match(out, /data-role="live-refusal"/);
  assert.ok(decode(out).includes(expected), 'the refusal line is present verbatim');
  // No destructive affordance for a live brain.
  assert.doesNotMatch(out, /data-role="forget-runtime"/);
  assert.doesNotMatch(out, /data-role="name-input"/);
  // The one fix is shown, copyable.
  assert.match(decode(out), /m1nd brain stop/);
});

// ── The live refusal string matches the REAL captured wire envelope ───────────
test('INV-09: liveRefusalLine reproduces the real delete_refusal.json refusal, verbatim', () => {
  const refusal = load<{ error: string; message?: string; detail?: string }>('delete_refusal.json');
  const wire = refusal.message ?? refusal.detail ?? '';
  // The wire message is the M1ndError Display (an "I/O error: " kind-prefix wraps
  // the refusal). The human-facing pre-check line renders the refusal CONTENT
  // itself; the actual server-path refusal (the fire() catch) renders `err.detail`
  // — the full wire string — verbatim. Assert our synthesized line equals the
  // core refusal (prefix stripped) for the captured ids, so the two never drift.
  const core = wire.replace(/^.*?error:\s*/i, '');
  const m = core.match(/live instance (\S+) \(pid (\d+)\)/);
  assert.ok(m, 'the captured envelope carries the live-instance refusal shape');
  const [, id, pid] = m!;
  assert.equal(liveRefusalLine(id, Number(pid)), core, 'our line reproduces the captured refusal content verbatim');
  // And the captured wire message DOES contain our line (INV-09: renders verbatim
  // from the server on the real call path).
  assert.ok(wire.includes(liveRefusalLine(id, Number(pid))), 'the wire refusal contains our verbatim line');
});

// ── Nothing animates on the destructive surface (§4A.4 / §6.3) ────────────────
test('§4A.4: the delete surface never animates (no animation/glow classes)', () => {
  const consequence = view({ step: 'consequence' });
  const confirm = view({ step: 'confirm', typed: basename, matches: true });
  for (const out of [consequence, confirm]) {
    assert.doesNotMatch(out, /\banimate-|\btremor-breath\b|drop-shadow|animation:/, 'no motion on the delete flow');
  }
});
