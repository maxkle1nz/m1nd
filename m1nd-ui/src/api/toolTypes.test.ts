/*
 * TrustVerdict regression pin — the UI union must mirror the Rust protocol
 * (m1nd-mcp/src/protocol/layers.rs: act | reverify | abstain | unprovable).
 * The assignment below is the compile-time guard: dropping a rung from the
 * union makes this file fail `tsc`; the runtime assert pins the count.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { TrustVerdict } from './toolTypes';

const ALL_RUNGS: readonly TrustVerdict[] = ['act', 'reverify', 'abstain', 'unprovable'] as const;

test('the trust ladder union carries all four rungs of the Rust protocol', () => {
  assert.equal(new Set(ALL_RUNGS).size, 4, 'four distinct rungs, none collapsed');
  assert.ok(ALL_RUNGS.includes('unprovable'), 'unprovable is a REAL answer the UI can type');
});
