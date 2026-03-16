import React from 'react';
import { BaseEdge, EdgeLabelRenderer, getStraightPath, type EdgeProps } from '@xyflow/react';

interface WeightedEdgeData {
  weight?: number;
  relation?: string;
}

export default function WeightedEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  data,
  selected,
}: EdgeProps) {
  const edgeData = data as WeightedEdgeData | undefined;
  const weight = edgeData?.weight ?? 0.5;
  // Stroke width 1–4 based on weight
  const strokeWidth = 1 + weight * 3;
  const color = selected ? '#a78bfa' : `rgba(100, 116, 139, ${0.3 + weight * 0.5})`;

  const [edgePath, labelX, labelY] = getStraightPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
  });

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        style={{ stroke: color, strokeWidth, transition: 'stroke 0.2s' }}
      />
      {selected && edgeData?.relation && (
        <EdgeLabelRenderer>
          <div
            className="absolute text-[9px] text-slate-400 bg-m1nd-surface px-1 rounded pointer-events-none"
            style={{
              transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
            }}
          >
            {edgeData.relation}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
