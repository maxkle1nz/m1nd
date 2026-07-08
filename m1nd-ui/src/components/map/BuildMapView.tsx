/*
 * BuildMapView — the 'map' surface (HUMAN-VIEW-V2 F1). Wires useBuildMap and maps
 * its status onto the screens: loading (§1.3), error + Retry (§1.3), and the map
 * or the honest empty screen (delegated to BuildMap). The whole surface is a
 * read-only projection — the front door of the product.
 */
import { useBuildMap } from '../../hooks/useBuildMap';
import BuildMap from './BuildMap';

export interface BuildMapViewProps {
  /** Open the Living Tree (kept one click away — PRD: the deterministic surface
   *  is never killed). */
  onOpenTree?: () => void;
  enabled?: boolean;
}

export default function BuildMapView({ onOpenTree, enabled = true }: BuildMapViewProps) {
  const { status, snapshot, rollup, error, reload } = useBuildMap(enabled);

  if (status === 'loading') {
    return (
      <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-loading">
        <div className="text-sm text-ink-soft">Loading repository map…</div>
      </div>
    );
  }

  if (status === 'error') {
    return (
      <div className="flex-1 flex items-center justify-center bg-porcelain" data-role="build-map-error">
        <div className="text-center space-y-3 px-6">
          <div className="text-sm text-state-failure">Failed to load map</div>
          {error && <div className="text-xs text-ink-soft font-mono max-w-md break-words">{error}</div>}
          <button
            type="button"
            data-role="retry"
            onClick={reload}
            className="px-3 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // ready | empty — BuildMap renders the canvas or the honest empty screen.
  return <BuildMap snapshot={snapshot ?? { present: false }} rollup={rollup} onOpenTree={onOpenTree} />;
}
