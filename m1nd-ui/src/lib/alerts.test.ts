/*
 * Owner alerts semantics — the pure helpers behind the Hall's owner-alerts panel
 * (honest doors). DOM-free: severity → matte tone, and the unacked/freshest-first
 * view the panel renders.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { severityTone, unackedAlerts, type DaemonAlert } from './alerts';

const alert = (over: Partial<DaemonAlert>): DaemonAlert => ({
  alert_id: 'a1',
  severity: 'warning',
  kind: 'drift',
  message: 'a member drifted',
  confidence: 0.8,
  evidence: [],
  created_at_ms: 1000,
  acked: false,
  ...over,
});

test('severityTone maps the real emitted severities to non-violet tones', () => {
  assert.equal(severityTone('critical'), 'clay');
  assert.equal(severityTone('warning'), 'amber');
  assert.equal(severityTone('info'), 'ink');
});

test('severityTone is case-tolerant and defaults an unknown severity to calm ink', () => {
  assert.equal(severityTone('CRITICAL'), 'clay');
  assert.equal(severityTone('  Warning '), 'amber');
  assert.equal(severityTone('mystery'), 'ink', 'unknown → ink, never a fabricated urgency');
});

test('unackedAlerts drops acked alerts and orders freshest-first', () => {
  const alerts: DaemonAlert[] = [
    alert({ alert_id: 'old', created_at_ms: 100 }),
    alert({ alert_id: 'acked', created_at_ms: 999, acked: true }),
    alert({ alert_id: 'new', created_at_ms: 500 }),
  ];
  const pending = unackedAlerts(alerts);
  assert.deepEqual(
    pending.map((a) => a.alert_id),
    ['new', 'old'],
    'acked is gone; the rest are newest-first',
  );
});

test('unackedAlerts tolerates a non-array body (a partial owner) — empty, never a throw', () => {
  assert.deepEqual(unackedAlerts(undefined as unknown as never[]), []);
  assert.deepEqual(unackedAlerts(null as unknown as never[]), []);
});

test('unackedAlerts never mutates its input', () => {
  const alerts = [alert({ alert_id: 'a', created_at_ms: 1 }), alert({ alert_id: 'b', created_at_ms: 2 })];
  const before = alerts.map((a) => a.alert_id);
  unackedAlerts(alerts);
  assert.deepEqual(alerts.map((a) => a.alert_id), before, 'the original order is untouched');
});
