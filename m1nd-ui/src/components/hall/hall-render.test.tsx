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
import { brainDisplayName } from '../../lib/hallSemantics';
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

test('INV-10: a countless brain renders absent-honest, never a fabricated 0', () => {
  // A fresh project brain before its first persist has no recorded counts. It
  // must render absent — "counts not recorded yet" (project language, NOT the
  // instance "not running") — never "0 nodes".
  const countless = { ...project, node_count: null, edge_count: null };
  const out = html(
    <BrainCard entry={countless} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />,
  );
  assert.match(out, /counts not recorded yet/);
  assert.match(out, /data-role="counts-absent"/);
  assert.doesNotMatch(out, /not running/, 'a project brain never reads instance "not running"');
  // The honesty tooth: no fabricated "0 nodes" / "0 edges".
  assert.doesNotMatch(out, /\b0 nodes\b/);
  assert.doesNotMatch(out, /\b0 edges\b/);
});

test('INV-10: a dormant SIBLING owner (no counts) still reads "counts unknown — not running"', () => {
  // The instance-language copy survives for a real owner instance (a sibling),
  // which genuinely can be not-running — only project brains change it.
  const sibling = { ...bound, brain_kind: 'sibling-owner-shape' as string, node_count: null, edge_count: null };
  const out = html(
    <BrainCard entry={sibling} isSelf={false} knownNodeCount={null} knownEdgeCount={null} selected={false} onSelect={noop} onOpen={noop} />,
  );
  assert.match(out, /counts unknown — not running/);
});

test('INV-10: every rendered card key traces to a real registry instance_id', () => {
  for (const entry of list.instances) {
    const out = html(
      <BrainCard entry={entry} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />,
    );
    assert.match(out, new RegExp(`data-brain-card="${entry.instance_id}"`));
  }
});

// ── INV-14 (§4A.8): no card labels a brain by implementation class ────────────
// The retired kind badge ("this brain" / "project" / "sibling" / "bound" /
// "hosted") leaked plumbing taxonomy onto the front door. INV-14 kills it from
// card faces; the class lives ONLY in the receipt's `binding:` line, and the one
// distinction a human needs at the Hall — "which am I looking at?" — is the
// viewing chip.

/** The class-word tokens that may NEVER appear on a Hall card face (§4A.8). */
const CLASS_WORDS = [/\bthis brain\b/i, /\bproject brain\b/i, /\bsibling\b/i, /\bhosted\b/i, /\bmedulla\b/i];

test('INV-14: no Hall card face labels a brain by implementation class (bound + hosted)', () => {
  for (const [entry, isSelf] of [
    [bound, true],
    [project, false],
  ] as const) {
    const face = visibleText(
      <BrainCard entry={entry} isSelf={isSelf} viewing={isSelf} selected={false} onSelect={noop} onOpen={noop} />,
    );
    for (const re of CLASS_WORDS) {
      assert.doesNotMatch(face, re, `card face must not carry the class word ${re} (INV-14)`);
    }
  }
});

test('INV-14: the class survives in the RECEIPT `binding:` line (bound → process-bound, hosted → owner-hosted)', () => {
  const boundReceipt = decode(
    html(
      <BrainReceiptDrawer entry={bound} isSelf self={self} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} />,
    ).replace(/<[^>]+>/g, ' '),
  );
  assert.match(boundReceipt, /binding/i, 'the receipt has a binding line');
  assert.match(boundReceipt, /process-bound/, 'the bound brain reads process-bound in the receipt');

  const projReceipt = decode(
    html(
      <BrainReceiptDrawer entry={project} isSelf={false} self={null} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} />,
    ).replace(/<[^>]+>/g, ' '),
  );
  assert.match(projReceipt, /owner-hosted/, 'the hosted brain reads owner-hosted in the receipt');
});

test('INV-14: exactly one card wears the viewing chip, and its name === the top-bar chip name', () => {
  // Render the whole grid as the Hall does: the viewing chip goes on the self
  // (bound) brain — the tree renders the bound graph today.
  const selfId = self.instance.instance_id;
  let viewingCount = 0;
  let viewingName = '';
  for (const entry of list.instances) {
    const isSelf = entry.instance_id === selfId;
    const out = html(
      <BrainCard entry={entry} isSelf={isSelf} viewing={isSelf} selected={false} onSelect={noop} onOpen={noop} />,
    );
    if (out.includes('data-role="viewing-chip"')) {
      viewingCount += 1;
      viewingName = brainDisplayName(entry);
    }
  }
  assert.equal(viewingCount, 1, 'exactly one card wears the viewing chip');

  // The Brain Chip (top bar) names the same brain from the same envelope.
  const chipName = visibleText(
    <BrainChip
      displayName={self.display_name ?? null}
      projectPath={self.project_root ?? null}
      nodeCount={null}
      healthy
      onClick={noop}
    />,
  );
  assert.match(chipName, new RegExp(viewingName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')), 'viewing chip name === Brain Chip name');
});

test('INV-14: the viewing chip carries the Eye icon and the word "viewing"', () => {
  const out = html(<BrainCard entry={bound} isSelf viewing selected={false} onSelect={noop} onOpen={noop} />);
  assert.match(out, /data-role="viewing-chip"/);
  assert.match(out, /data-icon="viewing"/, 'the viewing chip uses the Eye (viewing) icon from the registry');
  assert.match(visibleText(<BrainCard entry={bound} isSelf viewing selected={false} onSelect={noop} onOpen={noop} />), /viewing/);
  // A non-viewed card wears NO chip.
  const notViewed = html(<BrainCard entry={project} isSelf={false} viewing={false} selected={false} onSelect={noop} onOpen={noop} />);
  assert.doesNotMatch(notViewed, /data-role="viewing-chip"/);
});

// ── INV-11: no affordance without a surface (disabled-with-tooltip) ───────────
/** The opening `<button …>` tag for the given data-role (attributes only, no class body noise). */
const openTag = (out: string, role: string) => out.match(new RegExp(`<button[^>]*data-role="${role}"[^>]*>`))?.[0] ?? '';
/** React renders a disabled button with a bare/`=""` `disabled` ATTRIBUTE — distinct from the `disabled:` Tailwind class substring. */
const hasDisabledAttr = (tag: string) => /\sdisabled(=""|>|\s)/.test(tag);

test('§4A.9: a hosted project brain renders Open DISABLED against an owner with no selector stamp — retired residue gone', () => {
  // No `restSelector` → the pre-2H owner posture: Open stays disabled, but the
  // retired "REST brain routing (not built yet)" residue is deleted (INV-11 exit).
  const out = html(<BrainCard entry={project} isSelf={false} restSelector={false} selected={false} onSelect={noop} onOpen={noop} />);
  assert.ok(hasDisabledAttr(openTag(out, 'open-brain')), 'Open is disabled without the stamp');
  assert.doesNotMatch(out, /REST brain routing/i, 'the retired residue text is gone');
});

test('§4A.9: a hosted project brain renders Open ENABLED once the owner advertises the selector', () => {
  const out = html(<BrainCard entry={project} isSelf={false} restSelector selected={false} onSelect={noop} onOpen={noop} />);
  assert.ok(!hasDisabledAttr(openTag(out, 'open-brain')), 'Open is enabled with the stamp');
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
test('§4A.5: the Brain Chip renders the PROJECT name + node count from the self envelope', () => {
  const out = html(
    <BrainChip
      displayName={self.display_name ?? null}
      projectPath={self.project_root ?? null}
      nodeCount={self.graph_state.node_count}
      healthy
      onClick={noop}
    />,
  );
  assert.match(out, /data-role="brain-chip"/);
  assert.match(out, /data-healthy="true"/);
  // The count is tabular (comma-grouped, §4A.7 INV-13) — "6,569 nodes" — and the
  // Waypoints glyph precedes it. Assert on VISIBLE text (the count + "nodes" sit
  // in sibling spans around the icon, so match the tag-stripped text).
  const chipCountText = visibleText(
    <BrainChip displayName={self.display_name ?? null} projectPath={self.project_root ?? null} nodeCount={self.graph_state.node_count} healthy onClick={noop} />,
  ).replace(/\s+/g, ' ');
  assert.match(chipCountText, new RegExp(`${self.graph_state.node_count.toLocaleString()} nodes`));
  assert.match(out, /data-icon="graph"/, 'the Waypoints graph glyph precedes the node count');
  // The PROJECT name is in view — "m1nd", the repo, NOT the agent-memory sidecar
  // that graph_state.workspace_root carries (the exact leak Max saw).
  const chipText = visibleText(
    <BrainChip displayName={self.display_name ?? null} projectPath={self.project_root ?? null} nodeCount={null} healthy onClick={noop} />,
  );
  assert.match(chipText, /m1nd/);
  assert.doesNotMatch(chipText, /agent-memory/);
  assert.doesNotMatch(chipText, /\bclaude\b/);
});

test('§4A.5: a degraded chip wears the honesty (data-healthy=false), and a null brain says "no brain"', () => {
  const degraded = html(
    <BrainChip displayName={self.display_name ?? null} projectPath={self.project_root ?? null} nodeCount={null} healthy={false} onClick={noop} />,
  );
  assert.match(degraded, /data-healthy="false"/);
  const none = visibleText(<BrainChip displayName={null} projectPath={null} nodeCount={null} healthy={false} onClick={noop} />);
  assert.match(none, /no brain/);
});

// ── The naming guard (Brain Chip law, §4A.3/§4A.5) ────────────────────────────
// No Hall-rendered brain name may be a runtime dir name ("claude") or the
// "agent-memory" sidecar while a project_root exists. This is the INV pin of
// Max's screenshot verdict: the Hall shows PROJECTS, never plumbing. It runs on
// the REAL captured /api/instances fixture (both brains), across every surface
// that renders a brain's headline: the card, the receipt, and the chip.

/** The headline (name) region of a card — the bold title span text. */
const cardName = (entry: (typeof list.instances)[number], isSelf: boolean) =>
  visibleText(<BrainCard entry={entry} isSelf={isSelf} selected={false} onSelect={noop} onOpen={noop} />);

test('naming guard: no card headline is a plumbing name or a fingerprint hash', () => {
  for (const entry of list.instances) {
    const isSelf = entry.instance_id === self.instance.instance_id;
    const name = (entry.display_name ?? '').trim();
    // Every enriched entry in the fixture has a project_root → its name must be
    // the project basename, never a runtime/sidecar token.
    assert.ok(entry.project_root, `fixture entry ${entry.instance_id} must carry a project_root`);
    assert.notEqual(name, 'claude', 'a card name must never be the runtime dir "claude"');
    assert.notEqual(name, 'agent-memory', 'a card name must never be the "agent-memory" sidecar');
    // Max's screenshot: a project card wore "68c5ce186f6efcd2" — the fingerprint
    // store-dir hash. A name may never be a bare 16+ char hex fingerprint.
    assert.ok(
      !(name.length >= 16 && /^[0-9a-f]+$/i.test(name)),
      `a card name must never be a fingerprint hash: "${name}"`,
    );
    // The rendered card must actually show that project name.
    assert.match(cardName(entry, isSelf), new RegExp(name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
});

test('naming guard: the bound card shows "m1nd" (its repo), not "agent-memory"/"claude"', () => {
  const text = cardName(bound, true);
  assert.match(text, /m1nd/);
  assert.doesNotMatch(text, /agent-memory/);
  // "claude" may appear nowhere as the brain's identity.
  assert.equal(bound.display_name, 'm1nd');
});

test('naming guard: the project card shows "Cerrybubbles1", not the fingerprint store dir', () => {
  const text = cardName(project, false);
  assert.match(text, /Cerrybubbles1/);
  // The fingerprint hash (the pre-fix leak) must not be the headline.
  assert.doesNotMatch(text, /68c5ce186f6efcd2/);
  assert.equal(project.display_name, 'Cerrybubbles1');
});

// ── Project-brain semantics: a project brain is NOT an instance (§4A.3) ───────
// Max's live screenshot: the Cherry project card wore "counts unknown — not
// running" + a "stale lock" badge — instance language on an in-process brain.

test('project card shows its RECORDED counts, never "not running"', () => {
  const out = html(<BrainCard entry={project} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />);
  // The real stored counts (2089/7323 in the fixture) render from the entry.
  assert.match(out, new RegExp(`${project.node_count} nodes`));
  assert.match(out, new RegExp(`${project.edge_count} edges`));
  // "not running" is instance language — never on a project card.
  assert.doesNotMatch(out, /not running/);
  assert.doesNotMatch(out, /counts unknown/);
});

test('project card carries NO lock/instance badge (locks are an owner concept)', () => {
  // The fixture's project entry has a stale_lock conflict (a real captured
  // field) — it must be filtered out for kind=project.
  assert.ok(project.conflicts.includes('stale_lock'), 'fixture precondition: project has a stale_lock conflict');
  const out = html(<BrainCard entry={project} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />);
  assert.doesNotMatch(out, /stale.?lock/i, 'a project card must not show a lock badge');
  assert.equal((out.match(/data-role="conflict-chip"/g) ?? []).length, 0, 'no conflict chips on the project card');
});

test('project card wears a calm live dot, not a stale/failure dot from instance status', () => {
  // The fixture project entry has status:"stale" (an instance field) — it must
  // NOT drive the project card's dot to the stale band.
  assert.equal(project.status, 'stale', 'fixture precondition: instance status is stale');
  const out = html(<BrainCard entry={project} isSelf={false} selected={false} onSelect={noop} onOpen={noop} />);
  assert.match(out, /data-liveness="live"/, 'a present project brain reads live, not stale');
  assert.doesNotMatch(out, /data-liveness="stale"/);
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
