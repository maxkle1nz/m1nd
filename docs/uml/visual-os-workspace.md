# Visual OS Workspace — Tiled Layout and Split Panes (UML atlas)

This sheet details the structural layout, component tree, sequence of operations, state transitions, and invariants of the upgraded **m1nd-ui** Visual OS workspace layout (F3-SplitPane). 

The layout transitions from a drawer/modal-overlay model to a grid-pane environment. It introduces resizable split panes, a bottom terminal for event logs, an interactive status bar, and real-time wire evidence beads.

---

## 1. Component Architecture & Code Anchors

The following diagram maps the front-end layout components to their files and lines in the codebase:

```mermaid
flowchart TB
    App["App.tsx\n(Shell & Rung Router)\nm1nd-ui/src/App.tsx :299-410"]
    
    subgraph Layout["Workspace Tiling (NEW Split-Pane Layer)"]
        WSL["WorkspaceLayout.tsx\n(CSS Grid & Resize Manager)\nm1nd-ui/src/components/WorkspaceLayout.tsx :1-250"]
        DH["DragHandle.tsx\n(Mouse/Touch Resize Handle)\nm1nd-ui/src/components/soft/DragHandle.tsx :1-80"]
    end
    
    subgraph MapPane["Left Pane: Build Map Visualizer"]
        BMV["BuildMapView.tsx\n(Block Store Connector)\nm1nd-ui/src/components/map/BuildMapView.tsx :36-210"]
        BM["BuildMap.tsx\n(Canvas Drawer)\nm1nd-ui/src/components/map/BuildMap.tsx :124-340"]
        WIRES["Wires.tsx\n(SVG Wire & Evidence Beads)\nm1nd-ui/src/components/map/Wires.tsx :1-130"]
    end

    subgraph InspectorPane["Right Pane: In-Context Inspector"]
        BP["BlockPanel.tsx\n(Receipts & Socket Details)\nm1nd-ui/src/components/map/BlockPanel.tsx :40-290"]
        SC["ShowCode.tsx\n(File Code Viewer)\nm1nd-ui/src/components/map/ShowCode.tsx :80-360"]
    end

    subgraph Shelf["Bottom Pane: Interactive Terminal Shelf"]
        TL["TerminalLogs.tsx\n(Real-Time SSE Event Log)\nm1nd-ui/src/components/TerminalLogs.tsx :1-180"]
        SB["StatusBar.tsx\n(Stats & Shortcut HUD)\nm1nd-ui/src/components/soft/StatusBar.tsx :1-100"]
    end

    App --> WSL
    WSL --> DH
    WSL --> MapPane
    WSL --> InspectorPane
    WSL --> Shelf
    
    BMV --> BM
    BM --> WIRES
    BP --> SC
    TL --> SB
```

---

## 2. Sequence Diagram: Drag Resize & Event Propagation

The sequence below illustrates how pane resize operations are processed locally, and how real-time server events propagate to the terminal log panel:

```mermaid
sequenceDiagram
    participant U as Owner
    participant WSL as WorkspaceLayout
    participant DH as DragHandle
    participant SSE as Server-Sent Events (Cérebro)
    participant TL as TerminalLogs
    participant SB as StatusBar

    %% Pane resizing interaction
    U->>DH: Drag mouse on separator
    DH->>WSL: onResizeStart(paneIndex)
    loop drag mouse
        DH->>WSL: onResizeUpdate(newOffsetPixels)
        WSL->>WSL: calculatePercentageGrid()
        WSL-->>U: Render fluid panel dimensions
    end
    DH->>WSL: onResizeEnd()
    WSL->>WSL: saveLayoutToLocalStorage()
    
    %% Real-time Log Stream propagation
    SSE->>TL: onMessage(Rust cérebro log event)
    TL->>TL: appendLogLine()
    TL->>SB: updateMetricsState(8668 nodes, 31727 edges)
    SB-->>U: Redraw status indicators & counters
```

---

## 3. State & Flow Diagram: Layout Locking & Wire Evidence Beads

The system handles panel visibility, layout locks, and wire evidence beads dynamically:

```mermaid
stateDiagram-v2
    [*] --> DefaultTiledLayout : App Mount
    
    state "Workspace Resizing States" as Resize {
        DefaultTiledLayout --> Dragging : MouseDown on DragHandle
        Dragging --> Dragging : MouseMove (offset clamped to 15% - 85%)
        Dragging --> DefaultTiledLayout : MouseUp -> Layout Persisted
    }

    state "Wire Evidence Bead State (Wires.tsx)" as WireBeads {
        [*] --> DeclaredWire : Socket connected
        DeclaredWire --> EmptyBead : No integration receipts found (Circle stroke)
        DeclaredWire --> EvidencedBead : Active test/spec receipt lands (Circle solid green)
        EvidencedBead --> StaleBead : Source file edit detects drift (Circle amber)
    }

    state "Layout Configuration State" as LayoutLock {
        [*] --> FlexibleLayout : Default
        FlexibleLayout --> LockedLayout : Owner clicks Lock Layout (Status Bar)
        LockedLayout --> FlexibleLayout : Unlock Layout (Drag Handles Hidden)
    }
```

---

## 4. Architectural Invariants

Every workspace layout implementation must enforce the following rules:

1.  **Registry-Only Icons**: Icons used within drag handles, status bar, and panels must be imported solely from [registry.tsx](file:///Users/kle1nz/m1nd/m1nd-ui/src/lib/icons/registry.tsx). Any direct import of `lucide-react` is blocked by `icon-lint.mjs`.
2.  **Violet Quarantine**: Colors within the `iris` (violet) namespace are strictly prohibited in the layout panels. Violet is reserved solely for abstenção/unknown states (like unmapped residue or untested blocks), validated by `violet-lint.mjs`.
3.  **Hairline Borders**: Pane borders and separators must use the `hairline` token (`border-hairline`, `#D8D1C6` in porcelain theme) with a width of exactly `1px`.
4.  **Layout Non-Erosion**: Resizing pane ratios must never override the structural integrity of the Build Map canvas. The map must use responsive resize observers to center block structures automatically upon panel adjustments.
5.  **No-Leak Cleanliness**: Layout configurations stored in `localStorage` or reported in debugging logs must never contain personal directory paths (e.g. `/Users/kleinz/`). All paths must be relative to the workspace root.

---

## 5. Known Gaps

1.  **[High] Canvas Interaction Lag on Resize**: During pane resizing, continuous rerendering of SVG wires in `Wires.tsx` can cause visual stutter on low-tier screens. A debounce or CSS-driven scale overlay may be required during dragging.
2.  **[Medium] Overlay Dialog Precedence**: Modals like `ReviewRatify` must still float over the split panes to prevent split-panel focus confusion, but their boundaries must map to a central layout overlay registry to avoid visual collisions.
