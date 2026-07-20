import React from 'react';
import assert from 'node:assert/strict';
import test from 'node:test';
import { renderToStaticMarkup } from 'react-dom/server';
import type { OrganismManifestResponseV1 } from '../api/types';
import {
  ManifestStatusView,
  manifestFreshness,
  readManifestState,
  type ManifestLoadState,
} from './ManifestStatus';

const NOW = Date.parse('2026-07-18T12:00:30.000Z');

function manifestResponse(
  overrides: {
    sourceVersion?: string;
    binaryVersion?: string;
    bundleVersion?: string;
    activeMode?: string;
    issuanceFrozen?: boolean;
    generatedAt?: number;
  } = {},
): OrganismManifestResponseV1 {
  const sourceVersion = overrides.sourceVersion ?? '1.4.0';
  const binaryVersion = overrides.binaryVersion ?? '1.4.0';
  const bundleVersion = overrides.bundleVersion ?? '1.4.0';
  const versionsDrift = sourceVersion !== binaryVersion || sourceVersion !== bundleVersion;
  return {
    schema: 'm1nd-organism-manifest-response-v1',
    manifest: {
      schema: 'm1nd-organism-manifest-v1',
      organism_id: 'm1nd',
      repo_id: 'm1nd',
      brain_id: 'brain:test',
      project_root_fingerprint: 'sha256:root',
      source: { commit: 'abc123', dirty: false, version: sourceVersion },
      runtime: {
        owner_id: 'owner:test',
        binary_version: binaryVersion,
        binary_sha256: 'sha256:binary',
        started_at: NOW - 60_000,
      },
      graph: {
        generation: 7,
        snapshot_sha256: 'sha256:graph',
        node_count: 10,
        edge_count: 20,
      },
      architecture: {
        store_version: 3,
        skeleton_digest: 'sha256:skeleton',
        ratification_state: 'ratified',
      },
      ui: {
        bundle_version: bundleVersion,
        bundle_sha256: 'sha256:bundle',
        mode: 'embedded',
      },
      capabilities: { policy_version: 'UNAVAILABLE', enabled_effects: [] },
      autonomy: {
        supported_modes: ['HUMAN_GATED'],
        mechanically_proven_modes: [],
        active_mode: overrides.activeMode ?? 'UNKNOWN',
        activation_receipt_id: '',
        constitution_digest: '',
        constitution_epoch: 0,
        safety_kernel_digest: '',
        autonomy_epoch: 0,
        grants_digest: '',
        quorum_policy_digest: '',
        max_effective_tier_projection: 'NONE',
        issuance_frozen: overrides.issuanceFrozen ?? true,
        sentinel_safety_state: 'UNKNOWN',
      },
      schemas: {
        mission: 'm1nd-mission-letter-v0',
        receipt: 'm1nd-system-block-receipt-v0',
        checkpoint: 'UNAVAILABLE',
        light: 'm1nd-light-claim-v0',
        system_blocks: 'm1nd-system-block-store-v0',
      },
      authorities: {
        source: {
          revision: sourceVersion,
          digest: 'abc123',
          observed_at: NOW,
          freshness: 'FRESH',
          status: versionsDrift ? 'DRIFT' : 'AVAILABLE',
        },
        runtime_binary: {
          revision: binaryVersion,
          digest: 'sha256:binary',
          observed_at: NOW,
          freshness: 'FRESH',
          status: 'AVAILABLE',
        },
        graph: {
          revision: '7',
          digest: 'sha256:graph',
          observed_at: NOW,
          freshness: 'FRESH',
          status: 'AVAILABLE',
        },
        architecture: {
          revision: '3',
          digest: 'sha256:skeleton',
          observed_at: NOW,
          freshness: 'FRESH',
          status: 'AVAILABLE',
        },
        ui_bundle: {
          revision: bundleVersion,
          digest: 'sha256:bundle',
          observed_at: NOW,
          freshness: 'FRESH',
          status: 'AVAILABLE',
        },
        release_candidate: {
          revision: '',
          digest: '',
          observed_at: NOW,
          freshness: 'UNKNOWN',
          status: 'UNAVAILABLE',
        },
      },
      release_provenance: { release_candidate_digest: '', signature: '' },
      generated_at: overrides.generatedAt ?? NOW - 15_000,
      manifest_sha256: 'sha256:manifest',
    },
    verification: {
      coherence: versionsDrift ? 'DRIFT' : 'UNKNOWN',
      computed_manifest_sha256: 'sha256:manifest',
      issues: versionsDrift
        ? [
            {
              kind: 'DRIFT',
              authority_id: null,
              detail: `source/binary/bundle versions diverge: source=${sourceVersion}, binary=${binaryVersion}, bundle=${bundleVersion}`,
            },
          ]
        : [
            {
              kind: 'UNKNOWN',
              authority_id: 'autonomy_epoch',
              detail: 'autonomy_epoch is unavailable',
            },
          ],
    },
  };
}

function renderState(state: ManifestLoadState): string {
  return renderToStaticMarkup(<ManifestStatusView state={state} now={() => NOW} />);
}

test('component visibly projects source/binary/bundle drift and exact authority posture', () => {
  const response = manifestResponse({
    sourceVersion: '1.4.0',
    binaryVersion: '1.4.0',
    bundleVersion: '0.1.0',
    activeMode: 'UNKNOWN',
    issuanceFrozen: true,
  });
  const out = renderState({ kind: 'ready', response, receivedAt: NOW });

  assert.match(out, /data-manifest-coherence="DRIFT"/);
  assert.match(out, /data-manifest-version-drift="true"/);
  assert.match(out, /SRC\/BIN\/BND 1\.4\.0\/1\.4\.0\/0\.1\.0 · DRIFT/);
  assert.match(out, /MODE UNKNOWN/);
  assert.match(out, /FROZEN true/);
  assert.match(out, /GEN 15s FRESH/);
  assert.doesNotMatch(out, /FULL_AUTONOMY/);
  assert.doesNotMatch(out, /release (proven|verified)/i);
});

test('component renders unavailable honestly without retaining invented manifest facts', () => {
  const out = renderState({ kind: 'unavailable', detail: '404: manifest endpoint unavailable' });
  assert.match(out, /data-manifest-state="unavailable"/);
  assert.match(out, /MANIFEST UNAVAILABLE/);
  assert.doesNotMatch(out, /MODE HUMAN_GATED|MODE FULL_AUTONOMY|FROZEN (true|false)/);
  assert.doesNotMatch(out, /SRC\/BIN\/BND/);
});

test('active mode is rendered verbatim rather than upgraded by the UI', () => {
  const response = manifestResponse({ activeMode: 'HUMAN_GATED', issuanceFrozen: false });
  const out = renderState({ kind: 'ready', response, receivedAt: NOW });
  assert.match(out, /MODE HUMAN_GATED/);
  assert.match(out, /FROZEN false/);
  assert.doesNotMatch(out, /FULL_AUTONOMY/);
});

test('generated_at freshness exposes fresh, stale, and future-clock states deterministically', () => {
  assert.deepEqual(manifestFreshness(NOW - 5_000, NOW), {
    label: 'GEN 5s FRESH',
    state: 'fresh',
  });
  assert.deepEqual(manifestFreshness(NOW - 180_000, NOW), {
    label: 'GEN 3m STALE',
    state: 'stale',
  });
  assert.deepEqual(manifestFreshness(NOW + 7_000, NOW), {
    label: 'GEN +7s CLOCK',
    state: 'clock_drift',
  });
});

test('one poll preserves the exact response reference and failures become unavailable', async () => {
  const response = manifestResponse();
  const ready = await readManifestState(async () => response, () => NOW);
  assert.equal(ready.kind, 'ready');
  if (ready.kind !== 'ready') assert.fail('expected ready state');
  assert.equal(ready.response, response, 'poll does not clone or overwrite manifest facts');
  assert.equal(ready.receivedAt, NOW);

  const unavailable = await readManifestState(
    async () => {
      throw new Error('owner unavailable');
    },
    () => NOW,
  );
  assert.deepEqual(unavailable, { kind: 'unavailable', detail: 'owner unavailable' });
});

test('one poll passes the viewed brain and abort signal to the manifest loader', async () => {
  const response = manifestResponse();
  const controller = new AbortController();
  const viewedBrain = '/workspace/repo-beta';
  let receivedBrain: string | null | undefined;
  let receivedSignal: AbortSignal | undefined;

  const ready = await readManifestState(
    async (brain, signal) => {
      receivedBrain = brain;
      receivedSignal = signal;
      return response;
    },
    () => NOW,
    controller.signal,
    viewedBrain,
  );

  assert.equal(ready.kind, 'ready');
  assert.equal(receivedBrain, viewedBrain);
  assert.equal(receivedSignal, controller.signal);
});
