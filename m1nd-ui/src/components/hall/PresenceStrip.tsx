/*
 * PresenceStrip — the Hall's presence band (ORGANISM-INSIDE-PRD P1 ·
 * askGOD-verdict-P1). The human surface where the OWNER sees the team at one
 * glance: WHO is working (agent_id), WHERE (brain root · worktree), ON WHAT (the
 * charter's task), SINCE WHEN (a human age, always rendered — never a binary
 * "online"), a discreet MUTATION dot, and COLLISION as a CALM amber notice
 * (never a modal, never a block — verdict binding change 3).
 *
 * Honest by construction:
 *  - the roster only ever holds presences m1nd can SEE; the limitation is written
 *    on the surface (verdict risk "the inverse TTL lie") — a busy agent that
 *    hasn't called in stays invisible;
 *  - ages are always shown from `last_seen_ms`; 'fading' softens the dot past a
 *    display threshold but never claims expiry (the server GCs real ghosts);
 *  - empty is an honest sentence, not a blank.
 *
 * SOFT PROOF paper/ink — the collision amber is the house `verdict-reverify`
 * pastel (#c89b3c), never neon. Pure/presentational: the fetch lives in
 * `usePresences`; this component is a thin render over `lib/presence`.
 */
import type { PresenceEntry, PresenceCollision } from '../../types';
import { Icon } from '../../lib/icons/registry';
import { repoBasename } from '../../lib/hallSemantics';
import {
  compactAge,
  presenceLiveness,
  mutationSignal,
  type MutationSignal,
} from '../../lib/presence';

interface PresenceStripProps {
  presences: PresenceEntry[];
  collisions: PresenceCollision[];
  /** An honest error to surface instead of the roster (rare — presence fails open). */
  error?: string | null;
  /** Injected for deterministic tests; defaults to the wall clock. */
  nowMs?: number;
}

/** The full "last seen 12m ago" hover title behind the compact age chip. */
function seenTitle(lastSeenMs: number, nowMs: number): string {
  const s = Math.max(0, Math.floor((nowMs - lastSeenMs) / 1000));
  if (s < 5) return 'seen just now';
  return `last seen ${compactAge(lastSeenMs, nowMs)} ago`;
}

/** "repo-alpha" or "repo-alpha · lane-1" (brain root · worktree, when distinct). */
function whereLabel(entry: PresenceEntry): string {
  const brain = repoBasename(entry.root);
  const wt = entry.caller_root?.trim();
  if (wt && wt.length > 0 && wt !== entry.root) return `${brain} · ${repoBasename(wt)}`;
  return brain;
}

/** The calm amber sentence for one collision — plain language, no jargon. */
function collisionMessage(c: PresenceCollision): string {
  const ids = c.agent_ids.join(', ');
  const n = c.agent_ids.length;
  if (c.reason === 'declared_overlap') {
    return `${n} agents have overlapping declared work — ${ids} · ${repoBasename(c.brain_root)}`;
  }
  const place = repoBasename(c.caller_root?.trim() || c.brain_root);
  return `${n} agents writing in the same worktree — ${ids} · ${place}`;
}

const MUTATION_TITLE: Record<Exclude<MutationSignal, 'none'>, string> = {
  observed: 'writing — a mutating verb was observed',
  declared: 'declared intent to write (declared, not yet observed)',
};

function MutationDot({ signal, intent }: { signal: MutationSignal; intent?: string | null }) {
  if (signal === 'none') return null;
  const title = signal === 'declared' && intent ? `declared intent: ${intent}` : MUTATION_TITLE[signal];
  // Observed → a filled ink dot (the strong signal); declared → a hollow ring
  // (declared cloth). Discreet by design.
  const cls =
    signal === 'observed'
      ? 'bg-ink/60'
      : 'border border-ink/40';
  return (
    <span
      data-mutation-dot={signal}
      title={title}
      aria-label={title}
      className={`w-1.5 h-1.5 rounded-full inline-block shrink-0 ${cls}`}
    />
  );
}

export default function PresenceStrip({ presences, collisions, error, nowMs = Date.now() }: PresenceStripProps) {
  return (
    <section
      data-role="presence-strip"
      aria-label="agents visible to m1nd"
      className="shrink-0 border-b border-ink/10 bg-bone/30 px-6 py-3"
    >
      {/* Header + the always-visible limitation (verdict: write it on the surface). */}
      <div className="flex items-baseline justify-between gap-3">
        <div className="flex items-baseline gap-2">
          <Icon name="agents" size={14} decorative className="text-ink-soft/80 self-center" />
          <span className="text-xs font-medium text-ink">Working now</span>
          <span
            data-role="presence-count"
            className="text-[10px] font-mono text-ink-soft tabular-nums"
          >
            {presences.length}
          </span>
        </div>
      </div>
      <p data-role="presence-limitation" className="text-[10px] text-ink-soft/70 mt-0.5">
        presence = activity visible to m1nd — a busy agent that hasn&apos;t called in stays invisible
      </p>

      {/* Collisions — calm amber, above the roster; never modal, never blocking. */}
      {collisions.length > 0 && (
        <div className="mt-2 space-y-1.5">
          {collisions.map((c, i) => (
            <div
              key={`${c.brain_root}:${c.caller_root ?? ''}:${i}`}
              data-role="presence-collision"
              data-collision-reason={c.reason}
              className="flex items-center gap-2 rounded-lg border border-verdict-reverify/40 bg-verdict-reverify-tint/50 px-3 py-1.5 text-xs text-ink"
            >
              <span className="w-1.5 h-1.5 rounded-full bg-verdict-reverify shrink-0" aria-hidden />
              <span>{collisionMessage(c)}</span>
            </div>
          ))}
        </div>
      )}

      {/* Roster · empty · error. */}
      {error ? (
        <div
          data-role="presence-error"
          className="mt-2 rounded-lg border border-state-failure/25 bg-state-failure-tint/40 px-3 py-1.5 text-xs text-ink font-mono"
        >
          {error}
        </div>
      ) : presences.length === 0 ? (
        <div data-role="presence-empty" className="mt-2 text-xs text-ink-soft/80">
          No agents visible to m1nd right now.
        </div>
      ) : (
        <div role="list" className="mt-2 flex flex-wrap gap-2">
          {presences.map((p) => {
            const liveness = presenceLiveness(p, nowMs);
            const signal = mutationSignal(p);
            const where = whereLabel(p);
            const task = p.task_ref?.trim() || null;
            const metaTitle = [
              p.root,
              p.caller_root && p.caller_root !== p.root ? `worktree ${p.caller_root}` : null,
              task ? `task: ${task}` : null,
            ]
              .filter(Boolean)
              .join('\n');
            return (
              <div
                key={`${p.agent_id}:${p.root}:${p.caller_root ?? ''}`}
                role="listitem"
                data-role="presence-row"
                data-presence-agent={p.agent_id}
                data-liveness={liveness}
                data-mutation={signal}
                className="inline-flex items-start gap-2 rounded-lg border border-ink/12 bg-warm-paper px-3 py-1.5 max-w-[22rem]"
              >
                {/* Liveness dot — act-green while active, unverified-grey while fading. */}
                <span
                  data-role="presence-liveness-dot"
                  className={`mt-1 w-1.5 h-1.5 rounded-full inline-block shrink-0 ${
                    liveness === 'active' ? 'bg-verdict-act' : 'bg-state-unverified'
                  }`}
                  aria-hidden
                />
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs font-medium text-ink truncate max-w-[14ch]" title={p.agent_id}>
                      {p.agent_id}
                    </span>
                    <MutationDot signal={signal} intent={p.mutation?.declared_intent} />
                    <span
                      data-role="presence-age"
                      className="text-[10px] font-mono text-ink-soft tabular-nums"
                      title={seenTitle(p.last_seen_ms, nowMs)}
                    >
                      {compactAge(p.last_seen_ms, nowMs)}
                    </span>
                  </div>
                  <div className="text-[10px] text-ink-soft truncate max-w-[18rem]" title={metaTitle}>
                    <span data-role="presence-where">{where}</span>
                    {task && <span data-role="presence-task"> · {task}</span>}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
