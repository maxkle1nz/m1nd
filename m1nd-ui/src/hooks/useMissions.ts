/*
 * useMissions — the mission tray's read (HUMAN-VIEW-V2 F2.5 §2b, §3). Polls
 * `GET /api/mailbox?kind=mission` for the viewed brain and hands the tray the
 * per-mission heads. Read-only (§3e): the hook NEVER posts — the only write in the
 * F2.5b surface is the compose flow's `direct` mode (PacketCompose).
 *
 * §4A.9 (multi-brain owner): `brainRoot` routes the read through the `?brain=`
 * selector, exactly like the map — the tray reads the brain the human is viewing,
 * never a stale bound box. `null` = the bound brain (no selector — byte-compatible).
 *
 * Poll cadence (declared): ONE fetch on mount so the collapsed strip has real
 * per-phase counts, then an ~8s interval ONLY while the tray is EXPANDED; collapsed
 * pauses the interval (§3a strip is a last-known glance, not a live drain). Expanding
 * refetches immediately. The fetch heart is extracted as `loadMissions` so the
 * routing + the degraded-owner branch are provable DOM-free.
 */
import { useCallback, useEffect, useState } from 'react';
import { api } from '../api/client';
import type { MissionHead } from '../lib/missions';

/** loading → the first fetch is in flight · ready → heads served · unsupported →
 *  the owner has no `kind=mission` read (a pre-F2.5a owner: needs an update) ·
 *  error → the read failed (network/404). */
export type MissionsStatus = 'loading' | 'ready' | 'unsupported' | 'error';

export interface MissionsData {
  missions: MissionHead[];
  status: MissionsStatus;
  error: string | null;
  reload: () => void;
}

/** The sinks `loadMissions` writes through — injectable so the fetch heart is
 *  testable without a DOM (a test passes recorders). */
export interface MissionsSinks {
  isMounted: () => boolean;
  setMissions: (m: MissionHead[]) => void;
  setStatus: (s: MissionsStatus) => void;
  setError: (e: string | null) => void;
}

/**
 * The tray's fetch heart. `brainRoot` rides the `?brain=` selector (§4A.9). A
 * response WITHOUT a `missions` array means the owner ignored `kind=mission` (a
 * pre-F2.5a owner returned the field-report caixinha) — that is `unsupported`, the
 * honest "mission letters need an updated owner", never an empty tray pretending
 * there are no missions. A thrown fetch (network/404) is `error`.
 */
export async function loadMissions(brainRoot: string | null, sinks: MissionsSinks): Promise<void> {
  try {
    const resp = await api.missionHeads(brainRoot);
    if (!sinks.isMounted()) return;
    if (!Array.isArray(resp.missions)) {
      // The owner does not speak kind=mission — degrade honestly, do not blank.
      sinks.setStatus('unsupported');
      return;
    }
    sinks.setMissions(resp.missions);
    sinks.setError(null);
    sinks.setStatus('ready');
  } catch (err) {
    if (!sinks.isMounted()) return;
    sinks.setError(err instanceof Error ? err.message : 'failed to read mission letters');
    sinks.setStatus('error');
  }
}

export function useMissions(
  enabled: boolean,
  brainRoot: string | null = null,
  expanded: boolean = false,
): MissionsData {
  const [missions, setMissions] = useState<MissionHead[]>([]);
  const [status, setStatus] = useState<MissionsStatus>('loading');
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => setTick((t) => t + 1), []);

  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    const sinks: MissionsSinks = {
      isMounted: () => mounted,
      setMissions,
      setStatus,
      setError,
    };
    void loadMissions(brainRoot, sinks);
    // Live only while expanded — the collapsed strip keeps the last snapshot.
    const id = expanded ? setInterval(() => void loadMissions(brainRoot, sinks), 8000) : null;
    return () => {
      mounted = false;
      if (id != null) clearInterval(id);
    };
  }, [enabled, brainRoot, expanded, tick]);

  return { missions, status, error, reload };
}
