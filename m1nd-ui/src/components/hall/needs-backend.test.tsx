/*
 * INV-11 completeness sweep — no affordance without a surface (§4A.6).
 * Every [needs-backend] item renders disabled-with-tooltip naming the residue,
 * never a fake button. Fixtures: real captured envelopes (INV-01).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BrainCard from './BrainCard';
import BrainReceiptDrawer from './BrainReceiptDrawer';
import BrainPalette from './BrainPalette';
import type { InstanceListResponse, InstanceSelfResponse } from '../../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) => s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const hasDisabledAttr = (tag: string) => /\sdisabled(=""|>|\s)/.test(tag);
const roleTag = (out: string, role: string, extra = '') => out.match(new RegExp(`<button[^>]*data-role="${role}"[^>]*${extra}[^>]*>`))?.[0] ?? '';

const list = load<InstanceListResponse>('instances.json');
const self = load<InstanceSelfResponse>('instance_self.json');
const bound = list.instances.find((e) => e.brain_kind == null)!;
const project = list.instances.find((e) => e.brain_kind === 'project')!;
const noop = () => {};

// ── Residue (1) RETIRED → SHIPPED (§4A.9, 2H): per-brain Open. The old "REST
// brain routing (not built yet)" residue is gone. The honesty is now feature-
// detected: WITHOUT the owner's `rest_brain_selector` stamp, hosted Open stays
// disabled (INV-11 posture) but NEVER names the retired residue; WITH the stamp,
// hosted Open is a real, enabled action.
test('§4A.9: hosted-brain Open is disabled WITHOUT the selector stamp — and never names the retired residue', () => {
  const card = html(<BrainCard entry={project} isSelf={false} restSelector={false} selected={false} onSelect={noop} onOpen={noop} />);
  assert.ok(hasDisabledAttr(roleTag(card, 'open-brain')), 'card Open disabled without the stamp');
  assert.doesNotMatch(card, /REST brain routing/, 'the retired residue text is gone');

  const palette = html(
    <BrainPalette isOpen instances={list.instances} selfId={bound.instance_id} onClose={noop} onOpenBound={noop} restSelector={false} />,
  );
  assert.ok(hasDisabledAttr(roleTag(palette, 'brain-jump', 'data-openable="false"')), 'palette jump disabled without the stamp');
  assert.doesNotMatch(palette, /REST brain routing/, 'the retired residue text is gone from the palette');
});

test('§4A.9: hosted-brain Open is ENABLED once the owner advertises the selector stamp', () => {
  const card = html(
    <BrainCard entry={project} isSelf={false} restSelector selected={false} onSelect={noop} onOpen={noop} />,
  );
  assert.ok(!hasDisabledAttr(roleTag(card, 'open-brain')), 'card Open is enabled with the stamp');
  assert.match(roleTag(card, 'open-brain'), /Open this brain in the tree/, 'the tooltip is the real Open action');

  const palette = html(
    <BrainPalette
      isOpen
      instances={list.instances}
      selfId={bound.instance_id}
      onClose={noop}
      onOpenBound={noop}
      onOpenBrain={noop}
      restSelector
    />,
  );
  assert.ok(
    hasDisabledAttr(roleTag(palette, 'brain-jump', 'data-openable="true"')) === false,
    'palette hosted jump is enabled with the stamp',
  );
});

// ── Residue (2): §9.5.1 registry fields — counts NOW reported, calibration open ─
// Counts are no longer part of the residue for a project brain (warm/manifest
// counts ship 2026-07-04, per Max's screenshot). What REMAINS [needs-backend]
// is per-brain calibration + attached-session counts — still named honestly.
test('INV-11 residue #2: a project brain reports its counts; calibration/attached stays not-reported', () => {
  const card = html(<BrainCard entry={project} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />);
  // Counts now render (the residue's count half is closed) — never "not running".
  assert.match(card, new RegExp(`${project.node_count} nodes`));
  assert.doesNotMatch(card, /not running/);
  assert.doesNotMatch(card, /\b0 nodes\b/);

  const receipt = html(
    <BrainReceiptDrawer entry={project} isSelf={false} self={self} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} />,
  );
  assert.match(receipt, /data-role="needs-backend-fields"/, 'the calibration/attached residue is named');
  assert.match(decode(receipt), /not reported for this brain yet/);
});

// ── Residue (4): in-UI stop — shown as a disabled rung with the CLI ───────────
test('INV-11 residue #4 (in-UI stop): the Stop rung is disabled and shows the copyable CLI', () => {
  const receipt = html(
    <BrainReceiptDrawer entry={bound} isSelf self={self} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} />,
  );
  const text = decode(receipt.replace(/<[^>]+>/g, ' '));
  // Stop rung present, disabled, CLI shown.
  assert.match(text, /Stop/);
  assert.match(text, /m1nd brain stop/);
  const stopTag = receipt.match(/<button[^>]*disabled[^>]*>\s*Stop\s*<\/button>|<button[^>]*>\s*Stop\s*<\/button>/)?.[0] ?? '';
  assert.ok(hasDisabledAttr(stopTag) || receipt.includes('data-role="disabled-rung"'), 'stop is a disabled rung');
});

// ── Residue (5): eject V2 — a disabled rung naming V2 ─────────────────────────
test('INV-11 residue #5 (eject V2): the Eject rung is disabled and named as V2', () => {
  const receipt = html(
    <BrainReceiptDrawer entry={bound} isSelf self={self} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} />,
  );
  const text = decode(receipt.replace(/<[^>]+>/g, ' '));
  assert.match(text, /Eject/);
  assert.match(text, /V2/);
  // Exactly two disabled rungs on the receipt: stop + eject.
  assert.equal((receipt.match(/data-role="disabled-rung"/g) ?? []).length, 2);
});

// ── The clobber ban IS a needs-backend guard: no fake foreign-ingest affordance
test('INV-11 clobber ban: the disabled rungs never fake an action (no enabled destructive button among them)', () => {
  const receipt = html(
    <BrainReceiptDrawer entry={bound} isSelf self={self} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} />,
  );
  // Every disabled-rung button carries the `disabled` attribute (no enabled fake).
  for (const m of receipt.matchAll(/<div[^>]*data-role="disabled-rung"[^>]*>([\s\S]*?)<\/div>/g)) {
    const btn = m[1].match(/<button[^>]*>/)?.[0] ?? '';
    assert.ok(hasDisabledAttr(btn), 'a needs-backend rung must be a disabled button, never a fake enabled one');
  }
});
