/**
 * buildReplayFrames.ts — Convert an activate tool response into frame-by-frame
 * animation data for the ActivationReplay component.
 *
 * The activate response contains a ranked list of nodes with activation scores
 * and propagation hops. We model 5 phases per node:
 *   inactive → firing → propagating → settled → decaying
 *
 * Frame generation strategy:
 * - Seed nodes fire first (frame 0–2)
 * - Each propagation hop adds ~2 frames
 * - All nodes settle at peak frame
 * - Decay spreads backward from leaf nodes
 * - Total duration is configurable, default 3000ms
 */

import type { ReplayFrame } from '../stores/replayStore';

export interface ActivateResponseNode {
  id: string;
  label: string;
  score: number;
  is_seed?: boolean;
  hop?: number;          // propagation depth from seed (0 = seed)
  edges?: Array<{ target: string; weight: number }>;
}

export interface ActivateResponse {
  nodes: ActivateResponseNode[];
  query: string;
  elapsed_ms?: number;
}

/** Interpolate activation score for a node at frame t given its hop distance. */
function scoreAtFrame(
  node: ActivateResponseNode,
  frameIndex: number,
  totalFrames: number,
  maxHop: number,
): number {
  const hop = node.hop ?? 0;
  // Seed nodes (hop=0) fire at frame 0; deeper hops fire proportionally later
  const riseStart = Math.floor((hop / Math.max(maxHop, 1)) * (totalFrames * 0.4));
  const peakFrame = riseStart + Math.floor(totalFrames * 0.2);
  const decayStart = Math.floor(totalFrames * 0.7);

  if (frameIndex < riseStart) return 0;
  if (frameIndex <= peakFrame) {
    const t = (frameIndex - riseStart) / Math.max(peakFrame - riseStart, 1);
    return node.score * t;
  }
  if (frameIndex < decayStart) return node.score;
  const t = (frameIndex - decayStart) / Math.max(totalFrames - decayStart, 1);
  return node.score * (1 - t * 0.8);
}

/**
 * Build animation frames from an activate result.
 *
 * @param response  The raw activate tool response (nodes with scores + hop depths)
 * @param frameCount  Total number of frames (default 60)
 * @param totalDurationMs  Total animation duration in ms (default 3000)
 */
export function buildReplayFrames(
  response: ActivateResponse,
  frameCount = 60,
  totalDurationMs = 3000,
): { frames: ReplayFrame[]; totalDurationMs: number } {
  const { nodes } = response;

  if (!nodes || nodes.length === 0) {
    return { frames: [], totalDurationMs: 0 };
  }

  const msPerFrame = totalDurationMs / frameCount;
  const maxHop = Math.max(...nodes.map((n) => n.hop ?? 0), 1);

  // Pre-compute edge ID mappings
  const edgeIds = new Map<string, string>();
  nodes.forEach((node) => {
    (node.edges ?? []).forEach((edge) => {
      const id = `e-${node.id}-${edge.target}`;
      edgeIds.set(`${node.id}:${edge.target}`, id);
    });
  });

  const frames: ReplayFrame[] = Array.from({ length: frameCount }, (_, fi) => {
    const timestamp_ms = fi * msPerFrame;

    // Node activations at this frame
    const node_activations = nodes.map((node) => ({
      nodeId: node.id,
      score: scoreAtFrame(node, fi, frameCount, maxHop),
      isSeed: node.is_seed ?? node.hop === 0,
    }));

    // Edge signals — active when both endpoints have non-zero activation
    const activeNodeScores = new Map(
      node_activations.filter((na) => na.score > 0.05).map((na) => [na.nodeId, na.score]),
    );

    const edge_signals: ReplayFrame['edge_signals'] = [];
    nodes.forEach((node) => {
      (node.edges ?? []).forEach((edge) => {
        const sourceScore = activeNodeScores.get(node.id) ?? 0;
        const targetScore = activeNodeScores.get(edge.target) ?? 0;
        if (sourceScore > 0.05 && targetScore > 0.05) {
          const id = edgeIds.get(`${node.id}:${edge.target}`) ?? `e-${node.id}-${edge.target}`;
          edge_signals.push({ edgeId: id, signal: Math.min(sourceScore, targetScore) * edge.weight });
        }
      });
    });

    // Ghost edges: pairs with very low scores (structural speculation)
    const ghost_edges: ReplayFrame['ghost_edges'] = [];
    const peakPhaseStart = Math.floor(frameCount * 0.4);
    const peakPhaseEnd = Math.floor(frameCount * 0.7);
    if (fi >= peakPhaseStart && fi <= peakPhaseEnd) {
      const lowActivated = node_activations.filter((na) => na.score > 0 && na.score < 0.3);
      // Connect low-activation nodes to nearest seed — speculative edges
      const seeds = node_activations.filter((na) => na.isSeed && na.score > 0.5);
      lowActivated.slice(0, 5).forEach((la) => {
        seeds.slice(0, 2).forEach((seed) => {
          ghost_edges.push({ source: seed.nodeId, target: la.nodeId });
        });
      });
    }

    // Structural holes: nodes with zero activation but high pagerank hints
    // We approximate them as nodes that never fired across all frames
    // (computed lazily at frame 0 only to avoid repeating heavy computation)
    const structural_holes: ReplayFrame['structural_holes'] = [];

    return {
      timestamp_ms,
      node_activations,
      edge_signals,
      ghost_edges,
      structural_holes,
    };
  });

  // Annotate structural holes on the peak frame (frame with max total activation)
  const peakFrameIdx = frames.reduce((maxIdx, frame, idx) => {
    const total = frame.node_activations.reduce((s, n) => s + n.score, 0);
    const prevTotal = frames[maxIdx].node_activations.reduce((s, n) => s + n.score, 0);
    return total > prevTotal ? idx : maxIdx;
  }, 0);

  // Nodes that never exceeded 0.1 activation = structural holes
  const neverFired = nodes.filter((node) =>
    !frames.some((f) => (f.node_activations.find((na) => na.nodeId === node.id)?.score ?? 0) > 0.1),
  );
  frames[peakFrameIdx].structural_holes = neverFired.slice(0, 10).map((n) => ({
    id: n.id,
    label: n.label,
  }));

  return { frames, totalDurationMs };
}

/**
 * Quick summary: which frame has the peak total activation?
 */
export function findPeakFrame(frames: ReplayFrame[]): number {
  if (frames.length === 0) return 0;
  return frames.reduce((maxIdx, frame, idx) => {
    const total = frame.node_activations.reduce((s, n) => s + n.score, 0);
    const prevTotal = frames[maxIdx].node_activations.reduce((s, n) => s + n.score, 0);
    return total > prevTotal ? idx : maxIdx;
  }, 0);
}
