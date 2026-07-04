/*
 * TreeControls + GroupHeader render — the §4A.10 instruments surface.
 * The lens picker (dir/kind/layer), the name|meaning toggle (NO sparkle), the six
 * filter chips, the density toggle — each present with its registry icon, counts
 * tabular. Rendered with react-dom/server.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import TreeControls from './TreeControls';
import GroupHeader from './GroupHeader';

const noop = () => {};

const base = {
  lens: 'directory' as const,
  onLens: noop,
  searchMode: 'name' as const,
  onSearchMode: noop,
  query: '',
  onQuery: noop,
  onSubmitMeaning: noop,
  activeFilters: new Set<never>(),
  onToggleFilter: noop,
  density: 'comfortable' as const,
  onDensity: noop,
};

test('§4A.10: the lens picker offers directory | kind | layer, each with its registry icon', () => {
  const out = renderToStaticMarkup(React.createElement(TreeControls, base));
  assert.match(out, /data-role="lens-picker"/);
  assert.match(out, /data-lens="directory"/);
  assert.match(out, /data-lens="kind"/);
  assert.match(out, /data-lens="layer"/);
  // The lens glyphs come from the registry (FolderTree / Shapes / Layers).
  assert.match(out, /data-icon="groupDir"/);
  assert.match(out, /data-icon="groupKind"/);
  assert.match(out, /data-icon="layer"/);
  // The active lens is marked.
  assert.match(out, /data-lens="directory"[^>]*data-active="true"/);
});

test('§4A.10: the search mode is a labeled name|meaning TEXT toggle — no sparkle', () => {
  const out = renderToStaticMarkup(React.createElement(TreeControls, base));
  assert.match(out, /data-role="search-mode"/);
  assert.match(out, /data-mode="name"/);
  assert.match(out, /data-mode="meaning"/);
  // The honesty absence: no glitter icon in the toggle.
  assert.doesNotMatch(out, /sparkle/i);
  assert.doesNotMatch(out, /data-icon="verdictAbstain".*data-mode/, 'no magic star on the mode toggle');
});

test('§4A.10: the filter bar shows all six chips, each a real field', () => {
  const out = renderToStaticMarkup(React.createElement(TreeControls, base));
  assert.match(out, /data-role="filter-bar"/);
  for (const key of ['kind', 'language', 'trust', 'hasMemory', 'changed', 'churning']) {
    assert.match(out, new RegExp(`data-filter="${key}"`), `the ${key} chip is present`);
  }
  assert.match(out, /data-icon="filter"/, 'the filter concept icon is present');
});

test('§4A.10: an active filter chip is marked pressed', () => {
  const out = renderToStaticMarkup(
    React.createElement(TreeControls, { ...base, activeFilters: new Set(['trust']) as Set<'trust'> }),
  );
  assert.match(out, /data-filter="trust"[^>]*data-active="true"/);
});

test('§4A.10: the filter bar is hidden in meaning mode (the panel owns honesty there)', () => {
  const out = renderToStaticMarkup(React.createElement(TreeControls, { ...base, searchMode: 'meaning' as const }));
  assert.doesNotMatch(out, /data-role="filter-bar"/, 'no filter chips while meaning-searching');
});

test('§4A.10: the density toggle is present (a preference, not a mode)', () => {
  const out = renderToStaticMarkup(React.createElement(TreeControls, base));
  assert.match(out, /data-role="density-toggle"/);
});

test('§4A.10: a group header renders the lens icon + name + a tabular right-aligned count (INV-13)', () => {
  const out = renderToStaticMarkup(
    React.createElement(GroupHeader, {
      label: 'entry_points · L1',
      count: 462,
      icon: 'layer',
      expanded: true,
      height: 28,
      onToggle: noop,
    }),
  );
  assert.match(out, /data-role="group-header"/);
  assert.match(out, /data-icon="layer"/);
  assert.match(out, /entry_points · L1/);
  // The count is tabular mono, right-aligned (via StatValue), and comma-grouped.
  assert.match(out, /data-role="stat-value"/);
  assert.match(out, /tabular-nums/);
  assert.match(out, /462/);
});
