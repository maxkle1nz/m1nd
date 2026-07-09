/*
 * useFileView — the Show Code viewer's read (HUMAN-VIEW-V2 F2/F8). Fetches a
 * member file's content (read-only) from `/api/file`. A `null` path is idle
 * (nothing selected). Read-only by construction: the viewer never calls a mutating
 * verb — it reads a file under the repo, capped and escape-checked on the owner.
 *
 * SSR-safe: the effect never runs during `renderToStaticMarkup`, so the viewer
 * renders its idle/loading placeholder deterministically (the F1 tests' posture —
 * no network in a static render).
 */
import { useEffect, useState } from 'react';
import { api } from '../api/client';

export type FileViewStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface FileViewState {
  status: FileViewStatus;
  path: string | null;
  content: string;
  /** The file's true on-disk size (the honest denominator for `truncated`). */
  bytes: number;
  /** True when the owner capped the content below the file's true size. */
  truncated: boolean;
  error: string | null;
}

const IDLE: FileViewState = {
  status: 'idle',
  path: null,
  content: '',
  bytes: 0,
  truncated: false,
  error: null,
};

export function useFileView(path: string | null, brain?: string | null): FileViewState {
  const [state, setState] = useState<FileViewState>(IDLE);

  useEffect(() => {
    if (!path) {
      setState(IDLE);
      return;
    }
    let mounted = true;
    setState({ ...IDLE, status: 'loading', path });
    (async () => {
      try {
        const r = await api.fileView(path, brain);
        if (mounted) {
          setState({
            status: 'ready',
            path,
            content: r.content,
            bytes: r.bytes,
            truncated: r.truncated,
            error: null,
          });
        }
      } catch (err) {
        if (mounted) {
          setState({
            status: 'error',
            path,
            content: '',
            bytes: 0,
            truncated: false,
            error: err instanceof Error ? err.message : 'failed to read the file',
          });
        }
      }
    })();
    return () => {
      mounted = false;
    };
  }, [path, brain]);

  return state;
}
