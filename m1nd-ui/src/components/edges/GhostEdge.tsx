import React from 'react';
import { BaseEdge, getStraightPath, type EdgeProps } from '@xyflow/react';

export default function GhostEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  selected,
}: EdgeProps) {
  const [edgePath] = getStraightPath({ sourceX, sourceY, targetX, targetY });

  return (
    <BaseEdge
      id={id}
      path={edgePath}
      style={{
        stroke: selected ? '#7c3aed' : '#3b3b5c',
        strokeWidth: 1,
        strokeDasharray: '4 4',
        opacity: selected ? 0.8 : 0.4,
        transition: 'stroke 0.2s, opacity 0.2s',
      }}
    />
  );
}
