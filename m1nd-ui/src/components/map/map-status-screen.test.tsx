/*
 * MapStatusScreen — the Build Map's non-ready screens render honestly (5-stati).
 * Pure and prop-only, so both branches render under renderToStaticMarkup with no
 * DOM/timer — the repo's build-map.test.tsx pattern.
 *
 * The teeth (the cold-load gap this fix closes): the CALM loading screen offers no
 * escape (it is transient), but the SLOW screen — the one a hung read promotes to
 * after ~10s — must SAY the wait is long, name the likely cause (engine
 * unreachable), and carry Retry, so an unreachable engine can never look like a
 * silent forever-spinner. The error screen shows the failure + its detail + Retry.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MapErrorScreen, MapLoadingScreen } from './MapStatusScreen';

const html = (el: React.ReactElement) => renderToStaticMarkup(el);
const decode = (s: string) =>
  s.replace(/&#x27;/g, "'").replace(/&amp;/g, '&').replace(/&gt;/g, '>').replace(/&lt;/g, '<');
const visibleText = (el: React.ReactElement) => decode(html(el).replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');
const noop = () => {};

test('the calm loading screen is a plain wait — no note, no escape (it is transient)', () => {
  const out = html(<MapLoadingScreen slow={false} onRetry={noop} />);
  assert.match(out, /data-role="build-map-loading"/);
  assert.doesNotMatch(out, /data-role="build-map-loading-slow"/, 'not slow yet');
  assert.doesNotMatch(out, /data-role="retry"/, 'no retry while the wait is still normal');
  assert.match(visibleText(<MapLoadingScreen slow={false} onRetry={noop} />), /Loading repository map…/);
});

test('the SLOW loading screen SAYS the wait is long, names the cause, and offers Retry', () => {
  const el = <MapLoadingScreen slow onRetry={noop} />;
  const out = html(el);
  // Never a silent forever-spin: past the threshold the screen promotes honestly.
  assert.match(out, /data-role="build-map-loading-slow"/, 'the slow state is reached');
  assert.match(out, /data-role="retry"/, 'a hung read must offer a way out');
  const text = visibleText(el);
  assert.match(text, /taking longer than usual/i, 'it SAYS the wait is long');
  assert.match(text, /unreachable/i, 'it names the likely cause honestly');
  assert.match(text, /Retry/);
});

test('the error screen shows the failure, its detail, and Retry', () => {
  const el = <MapErrorScreen error="connection refused" onRetry={noop} />;
  const out = html(el);
  assert.match(out, /data-role="build-map-error"/);
  assert.match(out, /data-role="retry"/, 'a failed read is recoverable, never a dead end');
  const text = visibleText(el);
  assert.match(text, /Failed to load map/);
  assert.match(text, /connection refused/, 'the honest detail is shown, never swallowed');
});

test('the error screen holds Retry even with no detail string', () => {
  const out = html(<MapErrorScreen error={null} onRetry={noop} />);
  assert.match(out, /data-role="retry"/);
});
