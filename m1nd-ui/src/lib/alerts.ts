/*
 * Owner daemon alerts — the pure heart behind the Hall's owner-alerts panel
 * (honest doors: the Landing's owner item finally lands somewhere). No React, no
 * fetch: the wire types (the TypeScript mirror of `session.rs` `DaemonAlert` +
 * `daemon_handlers.rs` list/ack outcomes) and the presentational helpers the panel
 * renders. The alerts read/ack is BOUND-session scope by design — the SAME stock the
 * Universe's `owner.alerts_pending` counts (http_server.rs `universe_body`), so the
 * panel calls `alerts_list`/`alerts_ack` WITHOUT the `?brain=` selector (a `?brain=`
 * would ack a project brain's own alerts, never the owner's).
 *
 * Copy law: scoped, auditable strings only — never "done/proven/correct".
 */

/** One owner daemon alert — the mirror of `session.rs` `DaemonAlert`. Optional
 *  fields are absent (never rendered) when the emitter had nothing. */
export interface DaemonAlert {
  alert_id: string;
  severity: string;
  kind: string;
  message: string;
  confidence: number;
  evidence: string[];
  suggested_tool?: string | null;
  suggested_target?: string | null;
  file_path?: string | null;
  node_id?: string | null;
  created_at_ms: number;
  acked: boolean;
  acked_at_ms?: number | null;
}

/** The `alerts_list` outcome (`daemon_handlers.rs` `handle_alerts_list`): the
 *  alerts (unacked by default), an honest count, and whether the daemon is active. */
export interface AlertsListResponse {
  alerts: DaemonAlert[];
  total: number;
  active: boolean;
}

/** The `alerts_ack` outcome (`daemon_handlers.rs` `handle_alerts_ack`) — how many
 *  of the requested ids were actually flipped, and when. */
export interface AlertsAckResponse {
  acked: number;
  requested: number;
  acked_at_ms: number;
}

/** Severity → a matte, non-violet tone (SOFT PROOF): critical = clay failure,
 *  warning = amber reverify, everything else (info/low) = calm ink. Tolerant of an
 *  unknown severity (defaults to ink) — never a crash, never a fabricated urgency. */
export type AlertTone = 'ink' | 'amber' | 'clay';
export function severityTone(severity: string): AlertTone {
  const s = severity.trim().toLowerCase();
  if (s === 'critical' || s === 'high' || s === 'error') return 'clay';
  if (s === 'warning' || s === 'warn' || s === 'medium') return 'amber';
  return 'ink';
}

/** The unacked alerts, freshest first (the panel only ever shows what still awaits a
 *  hand — an acked alert is set aside). Pure; never mutates the input. Tolerant of a
 *  non-array (a partial/legacy owner body) — degrades to empty, never throws. */
export function unackedAlerts(alerts: DaemonAlert[]): DaemonAlert[] {
  if (!Array.isArray(alerts)) return [];
  return alerts
    .filter((a) => !a.acked)
    .slice()
    .sort((a, b) => b.created_at_ms - a.created_at_ms || a.alert_id.localeCompare(b.alert_id));
}
