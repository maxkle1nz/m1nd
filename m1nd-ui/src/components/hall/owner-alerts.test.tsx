/*
 * OwnerAlertsPanel — honesty at the pixel boundary (honest doors). Rendered with
 * react-dom/server: the unacked alerts list with per-alert ack, the "acknowledge all"
 * behind a simple confirm (SSR seam), the honest empty state, and the copy law.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import OwnerAlertsPanel from './OwnerAlertsPanel';
import type { DaemonAlert } from '../../lib/alerts';

const noop = () => {};
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visible = (out: string) => decode(out.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');

const NOW = Date.parse('2026-07-14T12:00:00Z');
const alert = (over: Partial<DaemonAlert>): DaemonAlert => ({
  alert_id: 'a1',
  severity: 'warning',
  kind: 'drift',
  message: 'a member drifted from its block',
  confidence: 0.8,
  evidence: ['src/foo.rs:42'],
  created_at_ms: NOW - 60_000,
  acked: false,
  ...over,
});

const render = (props: Partial<React.ComponentProps<typeof OwnerAlertsPanel>>) =>
  renderToStaticMarkup(
    React.createElement(OwnerAlertsPanel, {
      alerts: [],
      onAck: noop,
      onAckAll: noop,
      onClose: noop,
      now: NOW,
      ...props,
    }),
  );

test('the panel lists each unacked alert with its severity, message, and an ack', () => {
  const out = render({
    alerts: [
      alert({ alert_id: 'a1', severity: 'critical', message: 'trust risk climbing' }),
      alert({ alert_id: 'a2', severity: 'info', message: 'a quiet note' }),
    ],
  });
  assert.equal((out.match(/data-role="owner-alert"/g) ?? []).length, 2, 'both unacked alerts render');
  assert.match(out, /data-alert-id="a1"/);
  assert.match(out, /data-severity="critical"/);
  assert.equal((out.match(/data-role="alert-ack"/g) ?? []).length, 2, 'each alert has its own ack');
  const t = visible(out);
  assert.match(t, /trust risk climbing/);
  assert.match(t, /src\/foo\.rs:42/, 'evidence is shown verbatim');
});

test('an acked alert is not shown (the panel shows only what still awaits a hand)', () => {
  const out = render({
    alerts: [alert({ alert_id: 'done', acked: true }), alert({ alert_id: 'live' })],
  });
  assert.doesNotMatch(out, /data-alert-id="done"/);
  assert.match(out, /data-alert-id="live"/);
});

test('the empty state is honest — no fabricated bell', () => {
  const out = render({ alerts: [] });
  assert.match(out, /data-role="owner-alerts-empty"/);
  assert.match(visible(out), /No alerts awaiting your hand/);
  assert.doesNotMatch(out, /data-role="alert-ack-all"/, 'nothing to acknowledge → no ack-all');
});

test('"acknowledge all" is offered when alerts exist and asks first (the SSR seam confirm)', () => {
  const closed = render({ alerts: [alert({}), alert({ alert_id: 'a2' })] });
  assert.match(closed, /data-role="alert-ack-all"/, 'the trigger shows');
  assert.doesNotMatch(closed, /data-role="alert-ack-all-confirm"/, 'no confirm until opened');

  const open = render({ alerts: [alert({}), alert({ alert_id: 'a2' })], initialAckAllConfirmOpen: true });
  assert.match(open, /data-role="alert-ack-all-confirm"/, 'the confirm renders');
  assert.match(open, /data-role="alert-ack-all-go"/);
  assert.match(open, /data-role="alert-ack-all-cancel"/);
  assert.match(visible(open), /Acknowledge all 2\?/, 'the confirm names the real count');
});

test('an unannounced daemon reads quiet, never a fabricated alarm', () => {
  const out = render({ alerts: [], active: false });
  assert.match(visible(out), /the owner-daemon is quiet/);
});

test('COPY LAW: the alerts panel never says proven/done/correct', () => {
  const t = visible(
    render({ alerts: [alert({ severity: 'critical' })], initialAckAllConfirmOpen: true }),
  );
  assert.doesNotMatch(t, /\bproven\b/i);
  assert.doesNotMatch(t, /\bdone\b/i);
  assert.doesNotMatch(t, /\bcorrect\b/i);
});
