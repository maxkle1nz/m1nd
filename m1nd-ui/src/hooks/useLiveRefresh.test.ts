/*
 * useLiveRefresh core — the live-refresh debounce (HUMAN-LAYER-PRD §5.3).
 *
 * Verifies the pure heart of the "Living Tree goes live" path with fake timers
 * (no React, no DOM — matches the repo's zero-extra-deps test style):
 *   - a BURST of graph_changed events collapses to ONE refetch (~500 ms debounce)
 *   - only the `graph_changed` SSE class drives a refetch (reads/persist do not)
 *   - a mocked EventSource-style feed of graph_changed triggers exactly one reload
 */
import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import {
  createGraphChangeDebouncer,
  isGraphChanged,
  GRAPH_CHANGED_DEBOUNCE_MS,
} from './liveRefreshCore';
import type { SseEvent } from '../types';

test('a burst of graph_changed events debounces to a single refresh', () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    let refreshes = 0;
    const d = createGraphChangeDebouncer(() => (refreshes += 1), GRAPH_CHANGED_DEBOUNCE_MS);

    // Simulate an ingest fanning out 5 graph_changed events within the window.
    for (let i = 0; i < 5; i++) {
      d.fire();
      mock.timers.tick(100); // 100 ms apart — inside the 500 ms window
    }
    assert.equal(refreshes, 0, 'no refresh while the burst is still arriving');
    assert.equal(d.pending, true);

    // Quiet period elapses → exactly one coalesced refresh.
    mock.timers.tick(GRAPH_CHANGED_DEBOUNCE_MS);
    assert.equal(refreshes, 1, 'the whole burst collapsed into one refetch');
    assert.equal(d.pending, false);
  } finally {
    mock.timers.reset();
  }
});

test('two bursts separated by a quiet gap yield two refreshes', () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    let refreshes = 0;
    const d = createGraphChangeDebouncer(() => (refreshes += 1), GRAPH_CHANGED_DEBOUNCE_MS);

    d.fire();
    mock.timers.tick(GRAPH_CHANGED_DEBOUNCE_MS);
    assert.equal(refreshes, 1);

    d.fire();
    mock.timers.tick(GRAPH_CHANGED_DEBOUNCE_MS);
    assert.equal(refreshes, 2);
  } finally {
    mock.timers.reset();
  }
});

test('cancel() disarms a pending refresh (teardown safety)', () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    let refreshes = 0;
    const d = createGraphChangeDebouncer(() => (refreshes += 1), GRAPH_CHANGED_DEBOUNCE_MS);
    d.fire();
    d.cancel();
    mock.timers.tick(GRAPH_CHANGED_DEBOUNCE_MS * 2);
    assert.equal(refreshes, 0, 'a cancelled debouncer never fires');
    assert.equal(d.pending, false);
  } finally {
    mock.timers.reset();
  }
});

test('only the graph_changed SSE class triggers a refetch', () => {
  const changed: SseEvent = { event_type: 'graph_changed', data: { event: 'memorize' } };
  const activation: SseEvent = {
    event_type: 'activation',
    data: { agent_id: 'a', query: 'x', activated: [], top_k: 0 },
  };
  const persist: SseEvent = { event_type: 'persist', data: { generation: 3 } };
  const ingest: SseEvent = {
    event_type: 'ingest',
    data: { agent_id: 'a', path: '/p', nodes_added: 1, edges_added: 1 },
  };

  assert.equal(isGraphChanged(changed), true);
  assert.equal(isGraphChanged(activation), false, 'activation is not a refetch trigger');
  assert.equal(isGraphChanged(persist), false, 'persist is not a refetch trigger');
  assert.equal(isGraphChanged(ingest), false, 'raw ingest event is not the refetch trigger');
});

test('a mock EventSource feed of graph_changed drives exactly one reload', () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    // A minimal EventSource stand-in: register named listeners, then emit.
    type Listener = (e: { data: string }) => void;
    const listeners = new Map<string, Listener[]>();
    const es = {
      addEventListener(type: string, fn: Listener) {
        const arr = listeners.get(type) ?? [];
        arr.push(fn);
        listeners.set(type, arr);
      },
      emit(type: string, data: unknown) {
        for (const fn of listeners.get(type) ?? []) fn({ data: JSON.stringify(data) });
      },
    };

    let reloads = 0;
    const d = createGraphChangeDebouncer(() => (reloads += 1), GRAPH_CHANGED_DEBOUNCE_MS);
    // Wire the way useSSE does: a named `graph_changed` listener parses + dispatches.
    es.addEventListener('graph_changed', (e) => {
      const parsed = JSON.parse(e.data);
      if (parsed && typeof parsed.event === 'string') d.fire();
    });

    es.emit('graph_changed', { event: 'ingest' });
    es.emit('graph_changed', { event: 'ingest' });
    es.emit('graph_changed', { event: 'memorize' });
    assert.equal(reloads, 0, 'still debouncing during the burst');

    mock.timers.tick(GRAPH_CHANGED_DEBOUNCE_MS);
    assert.equal(reloads, 1, 'three server events → one snapshot refetch');
  } finally {
    mock.timers.reset();
  }
});
