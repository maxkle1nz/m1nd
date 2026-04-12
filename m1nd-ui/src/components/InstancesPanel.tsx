import React, { useEffect, useMemo, useState } from 'react';
import { api, ApiError } from '../api/client';
import type { InstanceListResponse, InstanceRegistryEntry, InstanceSelfResponse } from '../types';

interface InstancesPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

function formatAgo(lastHeartbeatMs: number): string {
  const delta = Math.max(0, Date.now() - lastHeartbeatMs);
  const secs = Math.floor(delta / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  return `${hours}h ago`;
}

function statusColor(status: string, stale: boolean): string {
  if (stale || status === 'stale') return '#f59e0b';
  if (status === 'degraded') return '#f59e0b';
  if (status === 'running') return '#00ff88';
  if (status === 'starting') return '#00f5ff';
  return '#64748b';
}

function shortPath(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts.length <= 3 ? path : `.../${parts.slice(-3).join('/')}`;
}

function errorDetail(error: unknown, fallback: string): string {
  if (error instanceof ApiError) return error.detail;
  if (error instanceof Error) return error.message;
  return fallback;
}

export default function InstancesPanel({ isOpen, onClose }: InstancesPanelProps) {
  const [selfInfo, setSelfInfo] = useState<InstanceSelfResponse | null>(null);
  const [instances, setInstances] = useState<InstanceRegistryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [savingId, setSavingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    let mounted = true;

    const refresh = async () => {
      try {
        const [selfResp, listResp] = await Promise.all([
          api.instanceSelf(),
          api.instances(),
        ]);
        if (!mounted) return;
        setSelfInfo(selfResp);
        setInstances(listResp.instances);
        setError(listResp.error ?? null);
        } catch (err) {
          if (!mounted) return;
          setError(errorDetail(err, 'Failed to load instances'));
        }
      };

    refresh();
    const id = setInterval(refresh, 5000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, [isOpen]);

  const summary = useMemo(() => {
    const conflicts = instances.filter((entry) => entry.conflicts.length > 0).length;
    const stale = instances.filter((entry) => entry.stale).length;
    return {
      total: instances.length,
      conflicts,
      stale,
    };
  }, [instances]);

  if (!isOpen) return null;

  const handleSave = async (instance: InstanceRegistryEntry) => {
    setSavingId(instance.instance_id);
    try {
      await api.saveInstanceState(instance.instance_id);
    } catch (err) {
      setError(errorDetail(err, 'Save failed'));
    } finally {
      setSavingId(null);
    }
  };

  const handleDelete = async (instance: InstanceRegistryEntry) => {
    setDeletingId(instance.instance_id);
    try {
      await api.deleteInstanceState(instance.instance_id);
        const refreshed = await api.instances();
        setInstances(refreshed.instances);
        setError(refreshed.error ?? null);
      } catch (err) {
        setError(errorDetail(err, 'Delete failed'));
      } finally {
        setDeletingId(null);
      }
  };

  return (
    <>
      <div className="fixed inset-0 bg-black/60 z-40" onClick={onClose} aria-hidden />
      <aside className="fixed top-0 right-0 z-50 h-screen w-[420px] max-w-full bg-m1nd-surface border-l border-m1nd-border-strong shadow-2xl flex flex-col">
        <div className="px-5 py-4 border-b border-m1nd-border-subtle">
          <div className="flex items-start justify-between gap-4">
            <div>
              <div className="text-[10px] uppercase tracking-widest text-slate-500 font-mono mb-2">
                command center
              </div>
              <h2 className="text-lg text-slate-100 font-semibold">Active m1nd instances</h2>
              <p className="text-xs text-slate-500 mt-1">
                One place to inspect running runtimes, spot conflicts, and save state.
              </p>
            </div>
            <button
              onClick={onClose}
              className="text-slate-500 hover:text-slate-200 transition-colors font-mono text-sm"
            >
              close
            </button>
          </div>
        </div>

        <div className="px-5 py-4 border-b border-m1nd-border-subtle grid grid-cols-3 gap-3 text-center">
          <div className="rounded-lg border border-m1nd-border-medium bg-m1nd-elevated p-3">
            <div className="text-lg font-mono text-slate-100">{summary.total}</div>
            <div className="text-[10px] uppercase tracking-widest text-slate-500">instances</div>
          </div>
          <div className="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3">
            <div className="text-lg font-mono text-amber-300">{summary.conflicts}</div>
            <div className="text-[10px] uppercase tracking-widest text-amber-500/70">conflicts</div>
          </div>
          <div className="rounded-lg border border-indigo-500/20 bg-indigo-500/5 p-3">
            <div className="text-lg font-mono text-indigo-300">{summary.stale}</div>
            <div className="text-[10px] uppercase tracking-widest text-indigo-400/70">stale</div>
          </div>
        </div>

        {selfInfo && (
          <div className="px-5 py-4 border-b border-m1nd-border-subtle bg-m1nd-base/40">
            <div className="flex items-center gap-2 mb-2">
              <span className="w-2 h-2 rounded-full" style={{ backgroundColor: '#00ff88' }} />
              <span className="text-xs font-mono uppercase tracking-widest text-slate-500">this instance</span>
            </div>
            <div className="text-sm text-slate-200 font-mono truncate">{shortPath(selfInfo.instance.workspace_root)}</div>
            <div className="mt-2 flex flex-wrap gap-3 text-[11px] text-slate-500 font-mono">
              <span>{selfInfo.graph_state.node_count} nodes</span>
              <span>{selfInfo.graph_state.edge_count} edges</span>
              <span>{selfInfo.active_agent_sessions} sessions</span>
              <span>{selfInfo.queries_processed} queries</span>
            </div>
          </div>
        )}

        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-3">
          {error && (
            <div className="rounded-lg border border-amber-500/25 bg-amber-500/8 px-3 py-2 text-xs text-amber-200 font-mono">
              {error}
            </div>
          )}

            {instances.map((instance) => {
              const baseUrl = instance.bind && instance.port
                ? `http://${instance.bind === '0.0.0.0' ? '127.0.0.1' : instance.bind}:${instance.port}`
                : null;
              const color = statusColor(instance.status, instance.stale);
              const isSelf = selfInfo?.instance.instance_id === instance.instance_id;
              return (
              <div
                key={instance.instance_id}
                className="rounded-xl border border-m1nd-border-medium bg-m1nd-base/70 p-4"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="w-2 h-2 rounded-full" style={{ backgroundColor: color }} />
                      <span className="text-sm text-slate-100 font-semibold">
                        {instance.workspace_root.split('/').filter(Boolean).slice(-1)[0] || instance.instance_id}
                      </span>
                      {isSelf && (
                        <span className="text-[10px] font-mono px-2 py-0.5 rounded border border-cyan-500/20 bg-cyan-500/10 text-cyan-300">
                          self
                        </span>
                      )}
                    </div>
                    <div className="text-[11px] text-slate-500 font-mono break-all">
                      {shortPath(instance.workspace_root)}
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="text-[10px] uppercase tracking-widest font-mono" style={{ color }}>
                      {instance.status}
                    </div>
                    <div className="text-[10px] text-slate-600 font-mono mt-1">
                      {formatAgo(instance.last_heartbeat_ms)}
                    </div>
                  </div>
                </div>

                <div className="mt-3 flex flex-wrap gap-2">
                  {instance.conflicts.map((conflict) => (
                    <span
                      key={conflict}
                      className="text-[10px] font-mono px-2 py-1 rounded border border-amber-500/20 bg-amber-500/10 text-amber-300"
                    >
                      {conflict.replace(/_/g, ' ')}
                    </span>
                  ))}
                  {instance.bind && instance.port && (
                    <span className="text-[10px] font-mono px-2 py-1 rounded border border-m1nd-border-medium text-slate-500">
                      {instance.bind}:{instance.port}
                    </span>
                  )}
                </div>

                <div className="mt-4 flex flex-wrap gap-2">
                  {baseUrl && (
                    <>
                      <button
                        onClick={() => window.open(baseUrl, '_blank', 'noopener,noreferrer')}
                        className="px-3 py-1.5 text-xs bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-m1nd-accent hover:text-m1nd-accent transition-colors"
                      >
                        Open
                      </button>
                      <button
                        onClick={() => window.open(`${baseUrl}/api/graph/stats`, '_blank', 'noopener,noreferrer')}
                        className="px-3 py-1.5 text-xs bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-m1nd-accent hover:text-m1nd-accent transition-colors"
                      >
                        Stats
                      </button>
                      <button
                        onClick={() => handleSave(instance)}
                        disabled={savingId === instance.instance_id}
                        className="px-3 py-1.5 text-xs bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-emerald-500 hover:text-emerald-300 transition-colors disabled:opacity-50"
                      >
                        {savingId === instance.instance_id ? 'Saving...' : 'Save state'}
                      </button>
                    </>
                  )}
                    <button
                      onClick={() => handleDelete(instance)}
                      disabled={deletingId === instance.instance_id || (instance.owner_live ?? false)}
                      className="px-3 py-1.5 text-xs bg-m1nd-elevated border border-m1nd-border-medium text-slate-300 rounded hover:border-rose-500 hover:text-rose-300 transition-colors disabled:opacity-50"
                      title={(instance.owner_live ?? false) ? 'Stop the instance before deleting its state' : 'Delete persisted state for this instance'}
                    >
                      {deletingId === instance.instance_id ? 'Deleting...' : 'Delete state'}
                    </button>
                </div>
              </div>
            );
          })}

          {instances.length === 0 && !error && (
            <div className="rounded-xl border border-m1nd-border-medium bg-m1nd-base/60 px-4 py-6 text-center text-sm text-slate-500">
              No registered instances yet.
            </div>
          )}
        </div>
      </aside>
    </>
  );
}
