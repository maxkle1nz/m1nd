import { create } from 'zustand';

export interface ReplayFrame {
  timestamp_ms: number;
  node_activations: { nodeId: string; score: number; isSeed: boolean }[];
  edge_signals: { edgeId: string; signal: number }[];
  ghost_edges: { source: string; target: string }[];
  structural_holes: { id: string; label: string }[];
}

export interface ReplayStore {
  isReplaying: boolean;
  query: string;
  frames: ReplayFrame[];
  currentFrame: number;
  isPlaying: boolean;
  speed: number;
  totalDurationMs: number;

  startReplay: (query: string, frames: ReplayFrame[], durationMs: number) => void;
  stopReplay: () => void;
  setFrame: (frame: number) => void;
  play: () => void;
  pause: () => void;
  setSpeed: (speed: number) => void;
  nextFrame: () => void;
  prevFrame: () => void;
  jumpToPeak: () => void;
}

export const useReplayStore = create<ReplayStore>((set, get) => ({
  isReplaying: false,
  query: '',
  frames: [],
  currentFrame: 0,
  isPlaying: false,
  speed: 1,
  totalDurationMs: 0,

  startReplay: (query, frames, durationMs) => set({
    isReplaying: true, query, frames, currentFrame: 0,
    isPlaying: true, totalDurationMs: durationMs,
  }),
  stopReplay: () => set({ isReplaying: false, isPlaying: false, frames: [], currentFrame: 0 }),
  setFrame: (frame) => set({ currentFrame: frame }),
  play: () => set({ isPlaying: true }),
  pause: () => set({ isPlaying: false }),
  setSpeed: (speed) => set({ speed }),
  nextFrame: () => set((s) => ({ currentFrame: Math.min(s.currentFrame + 1, s.frames.length - 1) })),
  prevFrame: () => set((s) => ({ currentFrame: Math.max(s.currentFrame - 1, 0) })),
  jumpToPeak: () => {
    const { frames } = get();
    if (frames.length === 0) return;
    const peakFrame = frames.reduce((maxIdx, frame, idx) => {
      const maxScore = Math.max(...frame.node_activations.map((n) => n.score));
      const prevMax = Math.max(...frames[maxIdx].node_activations.map((n) => n.score));
      return maxScore > prevMax ? idx : maxIdx;
    }, 0);
    set({ currentFrame: peakFrame });
  },
}));
