/*
 * OwnerAlertsPanel — the owner's daemon-alert panel, a drawer-class surface beside
 * the Hall grid (ESC/✕ returns). This is where the Universe Landing's OWNER item
 * finally lands: clicking "N daemon alerts to acknowledge" opens the Hall on THIS
 * panel. Pure/presentational (no fetch) — the Hall owns the read + the ack writes and
 * passes them down, so the honesty at the pixel boundary is SSR-provable.
 *
 * The alerts are the BOUND session's (the same stock the Universe counts). Each unacked
 * alert shows its severity (matte tone, never neon), kind, message, and evidence, with a
 * per-alert "acknowledge"; a header "acknowledge all" opens a simple confirm first (a
 * blanket clear is never one careless click). Ack is presentation of a real WRITE:
 * `alerts_ack` flips the owner's alert and the count falls in lockstep.
 *
 * Copy law (SOFT PROOF): scoped strings only — never "done/proven/correct".
 */
import { useState } from 'react';
import { compactAge } from '../../lib/presence';
import { severityTone, unackedAlerts, type AlertTone, type DaemonAlert } from '../../lib/alerts';
import { Icon } from '../../lib/icons/registry';

export interface OwnerAlertsPanelProps {
  /** Every owner alert the Hall read; the panel shows the UNACKED ones (freshest first). */
  alerts: DaemonAlert[];
  /** Whether the daemon is announced (an honest "the watcher is quiet" when not). */
  active?: boolean;
  /** Acknowledge one alert (the Hall runs `alerts_ack` + refreshes). */
  onAck: (alertId: string) => void;
  /** Acknowledge every unacked alert at once (behind the confirm). */
  onAckAll: (alertIds: string[]) => void;
  /** ESC / ✕ closes the drawer back to the Hall grid. */
  onClose: () => void;
  /** SSR/test seam: open the "acknowledge all" confirm at mount (no click under SSR). */
  initialAckAllConfirmOpen?: boolean;
  /** Injectable clock for a deterministic created-age under test. */
  now?: number;
}

/** Severity tone → a matte chip class (all sanctioned non-violet families). */
const TONE_CHIP: Record<AlertTone, string> = {
  ink: 'text-ink-soft border-hairline bg-bone',
  amber: 'text-verdict-reverify border-verdict-reverify/40 bg-verdict-reverify-tint/40',
  clay: 'text-state-failure border-state-failure/40 bg-state-failure-tint/40',
};

/** One unacked alert — severity chip + kind + message + evidence + a per-alert ack. */
function AlertCard({ alert, onAck, now }: { alert: DaemonAlert; onAck: (id: string) => void; now: number }) {
  const tone = severityTone(alert.severity);
  const age = compactAge(alert.created_at_ms, now);
  return (
    <div
      data-role="owner-alert"
      data-alert-id={alert.alert_id}
      data-severity={alert.severity}
      className="rounded-r-lg border border-l-2 border-ink/10 bg-warm-paper px-3 py-2.5 shadow-contact"
    >
      <div className="flex items-center gap-2 flex-wrap">
        <span
          data-role="alert-severity"
          className={`text-[10px] font-mono px-1.5 py-0.5 rounded border ${TONE_CHIP[tone]}`}
        >
          {alert.severity}
        </span>
        <span className="text-[11px] font-mono text-ink-soft">{alert.kind}</span>
        <span className="text-[11px] font-mono text-ink-soft/80 ml-auto tabular-nums" data-role="alert-age">
          {age}
        </span>
      </div>

      <p className="mt-1.5 text-sm text-ink leading-snug" data-role="alert-message">
        {alert.message}
      </p>

      {alert.evidence.length > 0 && (
        <ul className="mt-1 space-y-0.5" data-role="alert-evidence">
          {alert.evidence.map((ev, i) => (
            <li key={i} className="text-[11px] font-mono text-ink-soft break-all">
              {ev}
            </li>
          ))}
        </ul>
      )}

      {(alert.suggested_tool || alert.file_path) && (
        <div className="mt-1 text-[11px] font-mono text-ink-soft/80 break-all" data-role="alert-suggestion">
          {alert.suggested_tool ? `try ${alert.suggested_tool}` : ''}
          {alert.suggested_tool && alert.file_path ? ' · ' : ''}
          {alert.file_path ?? ''}
        </div>
      )}

      <div className="mt-1.5">
        <button
          type="button"
          data-role="alert-ack"
          onClick={() => onAck(alert.alert_id)}
          title="acknowledge — clears this alert from the owner (the finding stays in history)"
          className="inline-flex items-center gap-1 rounded border border-ink/15 bg-bone px-2 py-0.5 text-[11px] font-mono text-ink-soft hover:text-ink hover:shadow-contact transition-shadow"
        >
          acknowledge
        </button>
      </div>
    </div>
  );
}

export default function OwnerAlertsPanel({
  alerts,
  active = true,
  onAck,
  onAckAll,
  onClose,
  initialAckAllConfirmOpen = false,
  now = Date.now(),
}: OwnerAlertsPanelProps) {
  const pending = unackedAlerts(alerts);
  const [ackAllConfirmOpen, setAckAllConfirmOpen] = useState(initialAckAllConfirmOpen);

  return (
    <div
      className="w-full max-w-md h-full flex flex-col bg-porcelain border-l border-ink/10 shadow-card outline-none"
      data-role="owner-alerts-panel"
      data-surface="owner-alerts"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
    >
      {/* Header */}
      <div className="px-5 py-4 border-b border-ink/10 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Icon name="inbox" size={16} decorative className="text-ink-soft" />
            <h2 className="text-base text-ink font-semibold truncate" data-role="owner-alerts-title">
              Owner alerts
            </h2>
            {pending.length > 0 && (
              <span className="text-[11px] font-mono text-ink-soft tabular-nums">{pending.length}</span>
            )}
          </div>
          <p className="text-[11px] text-ink-soft mt-0.5">
            The owner-daemon's findings — acknowledging clears the bell; the finding stays in history.
          </p>
        </div>
        <button
          type="button"
          data-role="owner-alerts-close"
          onClick={onClose}
          className="text-xs text-ink-soft hover:text-ink px-2 py-1 shrink-0"
          title="ESC — back to the Hall"
        >
          ✕
        </button>
      </div>

      {/* Acknowledge-all — behind a simple confirm (a blanket clear asks first). */}
      {pending.length > 0 && (
        <div className="px-5 py-2 border-b border-ink/10 shrink-0">
          {!ackAllConfirmOpen ? (
            <button
              type="button"
              data-role="alert-ack-all"
              onClick={() => setAckAllConfirmOpen(true)}
              className="text-[11px] font-mono text-ink-soft hover:text-ink underline decoration-dotted underline-offset-2"
            >
              acknowledge all ({pending.length})
            </button>
          ) : (
            <div data-role="alert-ack-all-confirm" className="flex items-center gap-2 flex-wrap">
              <span className="text-[11px] text-ink">Acknowledge all {pending.length}?</span>
              <button
                type="button"
                data-role="alert-ack-all-go"
                onClick={() => {
                  setAckAllConfirmOpen(false);
                  onAckAll(pending.map((a) => a.alert_id));
                }}
                className="rounded border border-ink/15 bg-bone px-2 py-0.5 text-[11px] font-mono text-ink hover:shadow-contact transition-shadow"
              >
                acknowledge all
              </button>
              <button
                type="button"
                data-role="alert-ack-all-cancel"
                onClick={() => setAckAllConfirmOpen(false)}
                className="text-[11px] text-ink-soft hover:text-ink"
              >
                cancel
              </button>
            </div>
          )}
        </div>
      )}

      {/* Body */}
      <div className="flex-1 overflow-y-auto p-4 space-y-2">
        {pending.length === 0 ? (
          <div
            data-role="owner-alerts-empty"
            className="rounded-xl border border-ink/10 bg-bone/40 px-5 py-8 text-center text-sm text-ink-soft"
          >
            {active
              ? 'No alerts awaiting your hand.'
              : 'No alerts awaiting your hand — the owner-daemon is quiet.'}
          </div>
        ) : (
          pending.map((alert) => (
            <AlertCard key={alert.alert_id} alert={alert} onAck={onAck} now={now} />
          ))
        )}
      </div>
    </div>
  );
}
