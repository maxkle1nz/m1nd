/*
 * Card anatomy v2 render — the GOLD fields on the OPEN brain's face + the DEPTH
 * receipt (HUMAN-LAYER-PRD §4A.3.1). Props built from REAL fixtures via the
 * cardV2 logic; rendered with react-dom/server. Anti-scope enforced: no
 * timeseries/gauge/animated-percent in the DOM.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import BrainCard from './BrainCard';
import BrainCardGold from './BrainCardGold';
import BrainReceiptDrawer from './BrainReceiptDrawer';
import type { GraphSnapshot } from '../../lib/snapshot';
import type { AmIStaleOutput } from '../../api/toolTypes';
import type { InstanceListResponse, InstanceSelfResponse } from '../../types';
import {
  freshnessG1,
  calibrationG2,
  compoundingG3,
  alivenessG4,
  lastClaimD1,
  honestGapsD2,
  type CalibrationBlock,
} from '../../lib/cardV2';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&quot;/g, '"');
const visible = (el: React.ReactElement) => decode(renderToStaticMarkup(el).replace(/<[^>]+>/g, ' '));

const snap = load<GraphSnapshot>('snapshot.compact.json');
const stale = load<AmIStaleOutput>('am_i_stale.json');
const predict = load<{ calibration: CalibrationBlock }>('predict_calibration.json');
const self = load<InstanceSelfResponse>('instance_self.json');
const north = load<{ context?: { coverage?: { visited?: number; total?: number } }; honest_gaps?: string[] }>('north_warm.json');
const list = load<InstanceListResponse>('instances.json');
const bound = list.instances.find((e) => e.brain_kind == null)!;
const project = list.instances.find((e) => e.brain_kind === 'project')!;

const gold = {
  g1: freshnessG1(stale),
  g2: calibrationG2(predict.calibration),
  g3: compoundingG3(snap),
  g4: alivenessG4({ active_agent_sessions: self.active_agent_sessions, queries_processed: self.queries_processed }),
};
const noop = () => {};

test('§4A.3.1: the OPEN brain card renders the four GOLD stats rows (G1..G4)', () => {
  const out = renderToStaticMarkup(React.createElement(BrainCardGold, { ...gold, onReread: noop, onCalibrate: noop }));
  assert.match(out, /data-role="g1-freshness"/);
  assert.match(out, /data-role="g2-calibration"/);
  assert.match(out, /data-role="g3-compounding"/);
  assert.match(out, /data-role="g4-aliveness"/);
  // Each carries its concept icon from the registry.
  assert.match(out, /data-icon="freshness"/);
  assert.match(out, /data-icon="calibration"/);
  assert.match(out, /data-icon="memory"/);
  assert.match(out, /data-icon="agents"/);
});

test('§4A.3.1 G4: aliveness shows the real sessions + queries from the self envelope', () => {
  const text = visible(React.createElement(BrainCardGold, { ...gold }));
  assert.match(text, new RegExp(`${self.active_agent_sessions} agent`));
  assert.match(text, new RegExp(`${self.queries_processed} quer`));
});

test('§4A.3.1 G4 (interim honesty): the count is qualified "across all brains" — owner-wide until §9.5.1 partition', () => {
  // active_agent_sessions/queries_processed are OWNER-GLOBAL `health` counters,
  // not partitioned per brain (sessions on other hosted brains inflate this card).
  // The caption must carry the owner-wide qualifier so it never claims a per-brain
  // attribution it cannot back, and must NOT imply the count is this brain's alone.
  const text = visible(React.createElement(BrainCardGold, { ...gold }));
  assert.match(text, /across all brains/, 'G4 must qualify the owner-wide counter');
  assert.doesNotMatch(text, /this session/, 'the retired "this session" copy overclaimed per-brain scope');
});

test('§4A.3.1 G2: an uncalibrated open card shows [Calibrate once] + the engine cap; calibrated shows measured ✓', () => {
  // Calibrated (the real fixture) → "measured here ✓", no Calibrate button.
  const cal = renderToStaticMarkup(React.createElement(BrainCardGold, { ...gold, onCalibrate: noop }));
  if (predict.calibration.calibrated) {
    assert.match(decode(cal.replace(/<[^>]+>/g, ' ')), /measured here ✓/);
    assert.doesNotMatch(cal, /data-role="calibrate"/, 'no calibrate button when already measured');
  }
  // Uncalibrated → the button + the verbatim cap.
  const unc = renderToStaticMarkup(
    React.createElement(BrainCardGold, { ...gold, g2: calibrationG2({ calibrated: false }), onCalibrate: noop }),
  );
  assert.match(unc, /data-role="calibrate"/, 'the Calibrate once action is offered');
  assert.match(decode(unc.replace(/<[^>]+>/g, ' ')), /capped at reverify \(act UNREACHABLE\)/);
});

test('§4A.3.1 G1: a changed-since-read card offers [Re-read]; all-current does not', () => {
  const changed = renderToStaticMarkup(
    React.createElement(BrainCardGold, {
      ...gold,
      g1: freshnessG1({ checked: 2, stale: [{ path: 'a.rs', reason: 'changed' }], fresh: [] }),
      onReread: noop,
    }),
  );
  assert.match(changed, /data-role="reread"/, '[Re-read] is offered when files changed');
  assert.match(changed, /data-icon="ingest"/, 'the ingest (RefreshCw) icon is on Re-read');
  // All-current → no re-read button.
  const current = renderToStaticMarkup(React.createElement(BrainCardGold, { ...gold, onReread: noop }));
  if (gold.g1?.allCurrent) assert.doesNotMatch(current, /data-role="reread"/);
});

test('§4A.3.1 anti-scope: the GOLD block has NO percentage/gauge/sparkline/animation', () => {
  const out = renderToStaticMarkup(React.createElement(BrainCardGold, { ...gold }));
  // No aggregate health percentage on the face (the receipt holds exact numbers).
  assert.doesNotMatch(out, /brain health/i);
  assert.doesNotMatch(out, /sparkline|animate|animation-/i);
  // The face captions carry no bare "NN%" (percentages live in the receipt row).
  assert.doesNotMatch(visible(React.createElement(BrainCardGold, { g1: gold.g1, g2: gold.g2, g3: gold.g3, g4: gold.g4 })), /\d+%/);
});

test('§4A.3.1: the OPEN (self) BrainCard mounts the GOLD block; a hosted card does NOT', () => {
  const selfCard = renderToStaticMarkup(
    <BrainCard entry={bound} isSelf viewing gold={gold} onReread={noop} onCalibrate={noop} selected={false} onSelect={noop} onOpen={noop} knownNodeCount={self.graph_state.node_count} knownEdgeCount={self.graph_state.edge_count} />,
  );
  assert.match(selfCard, /data-role="card-gold"/, 'the open brain carries the GOLD block');

  const hostedCard = renderToStaticMarkup(
    <BrainCard entry={project} isSelf={false} gold={null} selected={false} onSelect={noop} onOpen={noop} />,
  );
  assert.doesNotMatch(hostedCard, /data-role="card-gold"/, 'a hosted brain shows the fields absent-honest (no GOLD block)');
});

test('§4A.3.1 DEPTH: the OPEN brain receipt shows D1 last claim + D2 honest gaps + the exact calibration row', () => {
  const depth = {
    d1: lastClaimD1(snap),
    d2: honestGapsD2({ coverage: north.context?.coverage, ghostEdges: 12, gaps: north.honest_gaps }),
    calibrationReceipt: calibrationG2(predict.calibration)?.receipt ?? null,
  };
  const out = renderToStaticMarkup(
    <BrainReceiptDrawer entry={bound} isSelf self={self} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} depth={depth} />,
  );
  assert.match(out, /data-role="card-depth"/);
  assert.match(out, /data-role="d1-last-claim"/, 'D1 last claim present');
  assert.match(out, /data-role="d2-honest-gaps"/, 'D2 honest gaps present');
  const text = decode(out.replace(/<[^>]+>/g, ' '));
  assert.match(text, /files visited/, 'coverage line rendered');
  assert.match(text, /guessed links/, 'ghost-edge count rendered');
  if (predict.calibration.calibrated) assert.match(text, /τ /, 'the exact calibration row is in the receipt');
});

test('§4A.3.1: a hosted brain receipt shows NO card-depth section (absent-honest)', () => {
  const out = renderToStaticMarkup(
    <BrainReceiptDrawer entry={project} isSelf={false} self={null} onClose={noop} onOpen={noop} onSave={noop} saving={false} onDeleted={noop} depth={null} />,
  );
  assert.doesNotMatch(out, /data-role="card-depth"/);
});
