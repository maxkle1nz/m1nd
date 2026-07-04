/*
 * Hall render tests — honesty invariants at the pixel boundary (§4A, INV-10/11).
 * Rendered with react-dom/server (no new deps). Fixtures are REAL captured
 * envelopes from the live :1338 owner (instances.json, instance_self.json).
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
import BrainChip from './BrainChip';
import type { InstanceListResponse, InstanceSelfResponse } from '../../types';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) => s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' '));

const list = load<InstanceListResponse>('instances.json');
const self = load<InstanceSelfResponse>('instance_self.json');
const bound = list.instances.find((e) => e.brain_kind == null)!;
const project = list.instances.find((e) => e.brain_kind === 'project')!;

const noop = () => {};

// ── INV-10: the Hall renders only owner-reported brains; absent counts absent ──
test('INV-10: the bound (self) card shows its real node/edge counts from graph_state', () => {
  const out = html(
    <BrainCard
      entry={bound}
      isSelf
      knownNodeCount={self.graph_state.node_count}
      knownEdgeCount={self.graph_state.edge_count}
      selected={false}
      onSelect={noop}
      onOpen={noop}
    />,
  );
  assert.match(out, new RegExp(`${self.graph_state.node_count} nodes`));
  assert.match(out, new RegExp(`${self.graph_state.edge_count} edges`));
  assert.doesNotMatch(out, /counts unknown/);
});

test('INV-10: a hosted/dormant brain with unknown counts renders "counts unknown", never 0', () => {
  const out = html(
    <BrainCard
      entry={project}
      isSelf={false}
      knownNodeCount={null}
      knownEdgeCount={null}
      selected={false}
      onSelect={noop}
      onOpen={noop}
    />,
  );
  assert.match(out, /counts unknown — not running/);
  assert.match(out, /data-role="counts-absent"/);
  // The honesty tooth: no fabricated "0 nodes" / "0 edges".
  assert.doesNotMatch(out, /\b0 nodes\b/);
  assert.doesNotMatch(out, /\b0 edges\b/);
});

test('INV-10: every rendered card key traces to a real registry instance_id', () => {
  for (const entry of list.instances) {
    const out = html(
      <BrainCard entry={entry} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />,
    );
    assert.match(out, new RegExp(`data-brain-card="${entry.instance_id}"`));
  }
});

test('kind badge: the hosted brain reads "project", the bound reads "this brain"', () => {
  const projCard = html(<BrainCard entry={project} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />);
  assert.match(visibleText(<BrainCard entry={project} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />), /project/);
  assert.ok(projCard.includes('project'));
  const boundText = visibleText(<BrainCard entry={bound} isSelf selected={false} onSelect={noop} onOpen={noop} />);
  assert.match(boundText, /this brain/);
});

// ── INV-11: no affordance without a surface (disabled-with-tooltip) ───────────
/** The opening `<button …>` tag for the given data-role (attributes only, no class body noise). */
const openTag = (out: string, role: string) => out.match(new RegExp(`<button[^>]*data-role="${role}"[^>]*>`))?.[0] ?? '';
/** React renders a disabled button with a bare/`=""` `disabled` ATTRIBUTE — distinct from the `disabled:` Tailwind class substring. */
const hasDisabledAttr = (tag: string) => /\sdisabled(=""|>|\s)/.test(tag);

test('INV-11: a hosted project brain (no REST routing) renders Open DISABLED with a residue tooltip', () => {
  const out = html(<BrainCard entry={project} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />);
  // The project brain has no bound port → cannot open in place.
  assert.ok(hasDisabledAttr(openTag(out, 'open-brain')), 'Open is disabled for the hosted brain');
  assert.match(out, /REST brain routing/i, 'the residue is named in the tooltip');
});

test('INV-11: the bound brain CAN be opened (Open is enabled)', () => {
  const out = html(<BrainCard entry={bound} isSelf selected={false} onSelect={noop} onOpen={noop} />);
  assert.ok(!hasDisabledAttr(openTag(out, 'open-brain')), 'the bound brain Open is enabled');
});

// ── INV-11: the receipt drawer carries the stop + eject disabled rungs ────────
test('INV-11: the receipt shows Stop and Eject as disabled rungs naming their residue', () => {
  const out = html(
    <BrainReceiptDrawer
      entry={bound}
      isSelf
      self={self}
      onClose={noop}
      onOpen={noop}
      onSave={noop}
      saving={false}
      onDeleted={noop}
    />,
  );
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /Stop/);
  assert.match(text, /Eject/);
  assert.match(text, /m1nd brain stop/, 'the stop CLI is shown copyable');
  assert.match(text, /V2/, 'eject is named as V2');
  // Both rungs are disabled buttons.
  const disabledCount = (out.match(/data-role="disabled-rung"/g) ?? []).length;
  assert.equal(disabledCount, 2, 'exactly the stop + eject rungs are disabled');
});

// ── The Brain Chip: the brain name is always in view (§4A.5) ───────────────────
test('§4A.5: the Brain Chip renders the brain name + node count from the self envelope', () => {
  const out = html(
    <BrainChip workspaceRoot={bound.workspace_root} nodeCount={self.graph_state.node_count} healthy onClick={noop} />,
  );
  assert.match(out, /data-role="brain-chip"/);
  assert.match(out, /data-healthy="true"/);
  assert.match(out, new RegExp(`${self.graph_state.node_count} nodes`));
  // The basename is in view.
  assert.match(visibleText(<BrainChip workspaceRoot={bound.workspace_root} nodeCount={null} healthy onClick={noop} />), /claude/);
});

test('§4A.5: a degraded chip wears the honesty (data-healthy=false), and a null brain says "no brain"', () => {
  const degraded = html(<BrainChip workspaceRoot={bound.workspace_root} nodeCount={null} healthy={false} onClick={noop} />);
  assert.match(degraded, /data-healthy="false"/);
  const none = visibleText(<BrainChip workspaceRoot={null} nodeCount={null} healthy={false} onClick={noop} />);
  assert.match(none, /no brain/);
});

// ── The receipt is a read-only receipt, not a dashboard (§4A.3, R6) ───────────
test('the receipt shows the binding facts (roots, pid, born) as a read-only receipt', () => {
  const text = decode(
    html(
      <BrainReceiptDrawer entry={bound} isSelf self={self} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} />,
    ).replace(/<[^>]+>/g, ' '),
  );
  assert.match(text, /the binding/);
  assert.match(text, /runtime root/);
  assert.match(text, /pid/);
  assert.match(text, new RegExp(String(bound.pid)));
});
