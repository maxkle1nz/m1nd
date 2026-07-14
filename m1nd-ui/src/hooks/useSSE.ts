import { useEffect, useRef } from 'react';
import type { SseEvent } from '../types';
import { API_BASE } from '../api/client';

// The SSE stream rides the SAME base as every other request (client.ts `API_BASE`):
// same-origin by default so the EventSource goes through the Vite dev proxy to
// whichever owner `M1ND_API` targets — not a hardcoded `localhost:1337` that
// cross-origins (and breaks) the moment the dev proxy points elsewhere.

interface UseSSEOptions {
  onEvent?: (event: SseEvent) => void;
  onError?: (err: Event) => void;
  enabled?: boolean;
}

/**
 * Subscribe to server-sent events at /api/events.
 * Listens for named event types: activation, learn, ingest, persist, graph_changed,
 * scan_progress. `graph_changed` (PRD §5.3) fires when an agent mutates the shared
 * graph — the Living Tree uses it to refresh live. `scan_progress`
 * (docs/uml/scan-loading.md slice 2) narrates the `skeleton_candidate` scan's real
 * phases — the Build Map's wait panel subscribes while a scan is in flight.
 * Reconnects automatically on error (exponential backoff, max 30s).
 */
export function useSSE({ onEvent, onError, enabled = true }: UseSSEOptions = {}) {
  const esRef = useRef<EventSource | null>(null);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const retryCountRef = useRef(0);

  useEffect(() => {
    if (!enabled) return;

    function connect() {
      const es = new EventSource(`${API_BASE}/api/events`);
      esRef.current = es;

      const EVENT_TYPES = [
        'activation',
        'learn',
        'ingest',
        'persist',
        'graph_changed',
        'scan_progress',
      ] as const;

      // Listen for named event types (SSE server sends event: <type>)
      for (const eventType of EVENT_TYPES) {
        es.addEventListener(eventType, (e: MessageEvent) => {
          try {
            const data = JSON.parse(e.data);
            const sseEvent = { event_type: eventType, data } as SseEvent;
            onEvent?.(sseEvent);
            retryCountRef.current = 0;
          } catch {
            // ignore parse errors
          }
        });
      }

      // Fallback: unnamed messages (event_type embedded in data)
      es.onmessage = (e) => {
        try {
          const parsed = JSON.parse(e.data);
          if (parsed.event_type && parsed.data) {
            onEvent?.(parsed as SseEvent);
          }
          retryCountRef.current = 0;
        } catch {
          // ignore parse errors
        }
      };

      es.onerror = (err) => {
        onError?.(err);
        es.close();
        esRef.current = null;

        // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 30s cap
        const delay = Math.min(1000 * Math.pow(2, retryCountRef.current), 30_000);
        retryCountRef.current += 1;
        retryRef.current = setTimeout(connect, delay);
      };
    }

    connect();

    return () => {
      esRef.current?.close();
      esRef.current = null;
      if (retryRef.current) clearTimeout(retryRef.current);
    };
  }, [enabled]); // eslint-disable-line react-hooks/exhaustive-deps
}
