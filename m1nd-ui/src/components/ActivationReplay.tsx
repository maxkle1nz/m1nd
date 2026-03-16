/**
 * ActivationReplay.tsx — Frame-by-frame activation animation panel.
 *
 * Features:
 *   - Play / Pause / Speed controls (0.25x, 0.5x, 1x, 2x, 4x)
 *   - Frame scrubber (range slider)
 *   - Frame-by-frame stepping (← → arrow buttons)
 *   - Jump to peak frame button
 *   - Node color updates driven by requestAnimationFrame
 *   - 5 animation phases: inactive → firing → propagating → settled → decaying
 */

import React, { useEffect, useRef, useCallback } from 'react';
import { useReplayStore } from '../stores/replayStore';
import { useGraphStore } from '../stores/graphStore';
import type { NodeAnimationState } from '../types';
import { lerpColor } from '../lib/colors';

// ---- Phase color mapping -----------------------------------------------

const PHASE_COLORS = {
  inactive:    '#1a1a2e',
  firing:      '#ff6b35',
  propagating: '#f59e0b',
  settled:     '#a78bfa',
  decaying:    '#2a2a3a',
};

function phaseColor(state: NodeAnimationState): string {
  switch (state.phase) {
    case 'inactive':    return PHASE_COLORS.inactive;
    case 'firing':      return lerpColor(PHASE_COLORS.inactive, PHASE_COLORS.firing, state.intensity);
    case 'propagating': return lerpColor(PHASE_COLORS.firing, PHASE_COLORS.propagating, state.intensity);
    case 'settled':     return lerpColor(PHASE_COLORS.propagating, PHASE_COLORS.settled, state.score);
    case 'decaying':    return lerpColor(PHASE_COLORS.settled, PHASE_COLORS.inactive, 0.5);
  }
}

// ---- Derive animation state from score at frame ------------------------

function scoreToAnimState(score: number, isSeed: boolean, prevScore: number): NodeAnimationState {
  if (score <= 0.02) {
    return prevScore > 0.02
      ? { phase: 'decaying' }
      : { phase: 'inactive' };
  }
  if (score > prevScore + 0.05) {
    // Rising — firing or propagating based on whether it's a seed
    return isSeed
      ? { phase: 'firing', intensity: Math.min(1, score) }
      : { phase: 'propagating', intensity: Math.min(1, score) };
  }
  // Stable
  return { phase: 'settled', score: Math.min(1, score) };
}

// ---- Speed options -----------------------------------------------------

const SPEED_OPTIONS = [0.25, 0.5, 1, 2, 4] as const;
const BASE_FRAME_MS = 50; // ms per frame at 1x speed

// ---- Component ---------------------------------------------------------

interface ActivationReplayProps {
  onBack?: () => void;
}

const ActivationReplay = React.memo(function ActivationReplay({ onBack }: ActivationReplayProps) {
  const {
    isReplaying,
    query,
    frames,
    currentFrame,
    isPlaying,
    speed,
    totalDurationMs,
    setFrame,
    play,
    pause,
    stopReplay,
    setSpeed,
    nextFrame,
    prevFrame,
    jumpToPeak,
  } = useReplayStore();

  const { nodes, setNodes } = useGraphStore();

  // rAF loop state
  const rafRef = useRef<number | null>(null);
  const lastTickRef = useRef<number>(0);
  const prevScoresRef = useRef<Map<string, number>>(new Map());

  // Apply frame to graph store nodes
  const applyFrame = useCallback((frameIdx: number) => {
    const frame = frames[frameIdx];
    if (!frame) return;

    const scoreMap = new Map(frame.node_activations.map((na) => [na.nodeId, na]));

    const updatedNodes = nodes.map((node) => {
      const na = scoreMap.get(node.id);
      if (!na) return node;

      const prevScore = prevScoresRef.current.get(node.id) ?? 0;
      const animState = scoreToAnimState(na.score, na.isSeed, prevScore);
      const color = phaseColor(animState);

      return {
        ...node,
        style: {
          ...node.style,
          '--node-color': color,
          '--node-glow': na.score > 0.5 ? `0 0 8px ${color}88` : 'none',
        } as React.CSSProperties,
        data: {
          ...node.data,
          activation: na.score,
          animationState: animState,
        },
      };
    });

    // Update previous scores
    frame.node_activations.forEach((na) => {
      prevScoresRef.current.set(na.nodeId, na.score);
    });

    setNodes(updatedNodes);
  }, [frames, nodes, setNodes]);

  // rAF playback loop
  useEffect(() => {
    if (!isPlaying || frames.length === 0) {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      return;
    }

    const frameDurationMs = BASE_FRAME_MS / speed;

    const tick = (timestamp: number) => {
      if (lastTickRef.current === 0) lastTickRef.current = timestamp;
      const elapsed = timestamp - lastTickRef.current;

      if (elapsed >= frameDurationMs) {
        lastTickRef.current = timestamp;

        const nextFrameIdx = useReplayStore.getState().currentFrame + 1;
        if (nextFrameIdx >= frames.length) {
          // End of replay — pause at last frame
          useReplayStore.getState().pause();
          return;
        }

        useReplayStore.getState().setFrame(nextFrameIdx);
        applyFrame(nextFrameIdx);
      }

      rafRef.current = requestAnimationFrame(tick);
    };

    lastTickRef.current = 0;
    rafRef.current = requestAnimationFrame(tick);

    return () => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [isPlaying, speed, frames.length, applyFrame]);

  // Apply current frame when user scrubs manually
  useEffect(() => {
    if (!isPlaying) {
      applyFrame(currentFrame);
    }
  }, [currentFrame, isPlaying, applyFrame]);

  // Keyboard: space = play/pause, ← → = frame step
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (!isReplaying) return;
      if (e.code === 'Space') {
        e.preventDefault();
        isPlaying ? pause() : play();
      }
      if (e.code === 'ArrowRight') { e.preventDefault(); nextFrame(); }
      if (e.code === 'ArrowLeft')  { e.preventDefault(); prevFrame(); }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [isReplaying, isPlaying, play, pause, nextFrame, prevFrame]);

  if (!isReplaying) return null;

  const frameCount = frames.length;
  const progressPct = frameCount > 1 ? (currentFrame / (frameCount - 1)) * 100 : 0;
  const currentTimeMs = frames[currentFrame]?.timestamp_ms ?? 0;

  // Current frame stats
  const currentFrameData = frames[currentFrame];
  const activeNodeCount = currentFrameData
    ? currentFrameData.node_activations.filter((na) => na.score > 0.05).length
    : 0;
  const peakScore = currentFrameData
    ? Math.max(0, ...currentFrameData.node_activations.map((na) => na.score))
    : 0;

  return (
    <div className="fixed inset-x-0 bottom-0 z-40 bg-m1nd-surface border-t border-m1nd-border-medium shadow-2xl">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-m1nd-border-subtle">
        <div className="flex items-center gap-3">
          <span className="text-xs font-mono text-m1nd-accent font-semibold uppercase tracking-wider">
            Replay
          </span>
          <span className="text-xs text-zinc-400 font-mono truncate max-w-[200px]" title={query}>
            activate("{query}")
          </span>
        </div>
        <div className="flex items-center gap-3 text-xs text-zinc-500 font-mono">
          <span>
            {activeNodeCount} active
          </span>
          <span>
            peak {(peakScore * 100).toFixed(0)}%
          </span>
          <button
            onClick={() => { stopReplay(); onBack?.(); }}
            className="ml-2 text-zinc-500 hover:text-zinc-200 transition-colors"
            title="Close replay"
            aria-label="Close replay"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Progress bar + scrubber */}
      <div className="px-4 py-2">
        <div className="relative">
          {/* Background track */}
          <div className="h-1 bg-m1nd-border-medium rounded-full mb-1" />
          {/* Filled progress */}
          <div
            className="absolute top-0 left-0 h-1 bg-m1nd-accent rounded-full transition-none"
            style={{ width: `${progressPct}%` }}
          />
          {/* Scrubber input */}
          <input
            type="range"
            min={0}
            max={Math.max(0, frameCount - 1)}
            value={currentFrame}
            onChange={(e) => {
              const f = parseInt(e.target.value, 10);
              setFrame(f);
            }}
            className="absolute inset-0 w-full h-1 opacity-0 cursor-pointer"
            aria-label="Frame position"
          />
        </div>
        {/* Time display */}
        <div className="flex justify-between text-[10px] text-zinc-600 font-mono mt-0.5">
          <span>{currentTimeMs.toFixed(0)}ms</span>
          <span>
            frame {currentFrame + 1}/{frameCount}
          </span>
          <span>{totalDurationMs.toFixed(0)}ms</span>
        </div>
      </div>

      {/* Controls */}
      <div className="flex items-center justify-center gap-3 px-4 pb-3">
        {/* Prev frame */}
        <button
          onClick={prevFrame}
          disabled={currentFrame === 0}
          className="w-7 h-7 flex items-center justify-center rounded text-zinc-400 hover:text-zinc-200 hover:bg-m1nd-elevated disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          title="Previous frame (←)"
          aria-label="Previous frame"
        >
          ◀
        </button>

        {/* Play / Pause */}
        <button
          onClick={isPlaying ? pause : play}
          className="w-9 h-9 flex items-center justify-center rounded-full bg-m1nd-accent text-white hover:bg-violet-400 transition-colors shadow-lg"
          title={isPlaying ? 'Pause (Space)' : 'Play (Space)'}
          aria-label={isPlaying ? 'Pause' : 'Play'}
        >
          {isPlaying ? '⏸' : '▶'}
        </button>

        {/* Next frame */}
        <button
          onClick={nextFrame}
          disabled={currentFrame >= frameCount - 1}
          className="w-7 h-7 flex items-center justify-center rounded text-zinc-400 hover:text-zinc-200 hover:bg-m1nd-elevated disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          title="Next frame (→)"
          aria-label="Next frame"
        >
          ▶
        </button>

        {/* Separator */}
        <div className="w-px h-5 bg-m1nd-border-medium mx-1" />

        {/* Jump to peak */}
        <button
          onClick={jumpToPeak}
          className="px-2 py-1 text-[10px] font-mono rounded text-zinc-400 hover:text-m1nd-accent hover:bg-m1nd-elevated transition-colors"
          title="Jump to peak activation frame"
          aria-label="Jump to peak"
        >
          PEAK
        </button>

        {/* Separator */}
        <div className="w-px h-5 bg-m1nd-border-medium mx-1" />

        {/* Speed controls */}
        <div className="flex items-center gap-1">
          {SPEED_OPTIONS.map((s) => (
            <button
              key={s}
              onClick={() => setSpeed(s)}
              className={`px-1.5 py-0.5 text-[10px] font-mono rounded transition-colors ${
                speed === s
                  ? 'bg-m1nd-accent text-white'
                  : 'text-zinc-500 hover:text-zinc-200 hover:bg-m1nd-elevated'
              }`}
              title={`${s}x speed`}
              aria-label={`${s}x speed`}
              aria-pressed={speed === s}
            >
              {s}×
            </button>
          ))}
        </div>
      </div>
    </div>
  );
});

export default ActivationReplay;
