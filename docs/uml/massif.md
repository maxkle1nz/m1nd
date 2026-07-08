# MASSIF UML

MASSIF is a Human View lens that renders persisted X-RAY paint tags from the existing graph snapshot as a stable isometric proof-topography view.

## Component diagram

```mermaid
flowchart LR
  subgraph Existing_Human_View
    HallShell[Hall shell]
    ClientApi[client ts]
    SnapshotTypes[snapshot ts]
    GraphCanvas[GraphCanvas]
    LivingTree[LivingTree]
    TreeLenses[treeLenses]
  end

  subgraph New_MASSIF_Client
    MassifView[MassifView React]
    SnapshotSource[SnapshotSource existing snapshot fetch]
    StateDeriver[StateDeriver tags to five state grammar]
    ContainerTree[ContainerTree path prefix primary layers optional]
    RollupEngine[RollupEngine proportion fill border holes]
    LayoutEngine[LayoutEngine d3 hierarchy stable order]
    IsoRenderer[IsoRenderer Canvas 2D prisms]
    ZoomController[ZoomController d3 zoom grammar LOD]
    DetailPanel[DetailPanel]
  end

  HallShell --> MassifView
  MassifView --> SnapshotSource
  SnapshotSource --> ClientApi
  SnapshotSource --> SnapshotTypes
  SnapshotSource --> StateDeriver
  StateDeriver --> ContainerTree
  TreeLenses -. optional layer lens pattern .-> ContainerTree
  ContainerTree --> RollupEngine
  RollupEngine --> LayoutEngine
  LayoutEngine --> IsoRenderer
  IsoRenderer --> ZoomController
  ZoomController --> DetailPanel
  LivingTree -. shared navigation shell .-> MassifView
  GraphCanvas -. neighboring graph view .-> MassifView
```

## Load and interaction sequence

```mermaid
sequenceDiagram
  participant User
  participant Hall as Hall shell
  participant View as MassifView
  participant Snapshot as SnapshotSource
  participant API as Existing graph snapshot API
  participant State as StateDeriver
  participant Tree as ContainerTree
  participant Rollup as RollupEngine
  participant Layout as LayoutEngine
  participant Render as IsoRenderer
  participant Detail as DetailPanel

  User->>Hall: open MASSIF lens
  Hall->>View: mount for current brain
  View->>Snapshot: request snapshot
  Snapshot->>API: GET graph snapshot
  API-->>Snapshot: nodes edges tags served brain
  Snapshot-->>State: nodes with tags
  State-->>State: derive state from xray tags
  State-->>State: absence becomes unpainted
  State-->>Tree: stateful nodes
  Tree-->>Tree: build path prefix containers
  Tree-->>Tree: attach optional full layer lens
  Tree-->>Rollup: container membership
  Rollup-->>Rollup: compute state proportions and border signal
  Rollup-->>Layout: container weights and stable keys
  Layout-->>Render: packed positions
  Render-->>User: draw Canvas 2D isometric blocks
  User->>Render: click block
  Render->>Detail: selected block and state receipt
  Detail-->>User: state evidence connectedness manifest source
```

## Grammar state diagram

```mermaid
stateDiagram-v2
  [*] --> Unpainted
  Unpainted --> Bedrock: repaint after commit adds bedrock tag
  Unpainted --> Overgrowth: repaint after commit adds overgrowth tag
  Unpainted --> Unproven: repaint after commit adds unproven tag
  Unpainted --> ErosionCandidate: repaint after commit adds erosion candidate tag

  Bedrock --> Unproven: repaint after commit evidence disappears
  Unproven --> Bedrock: repaint after commit evidence appears
  Overgrowth --> Unproven: repaint after commit references appear
  Unproven --> Overgrowth: repaint after commit references disappear

  Bedrock --> ErosionCandidate: repaint after commit manifest rule flags source
  Overgrowth --> ErosionCandidate: repaint after commit manifest rule flags source
  Unproven --> ErosionCandidate: repaint after commit manifest rule flags source
  ErosionCandidate --> Bedrock: repaint after commit rule clears and evidence exists
  ErosionCandidate --> Overgrowth: repaint after commit rule clears and node is orphaned
  ErosionCandidate --> Unproven: repaint after commit rule clears and node is used without evidence
```

## Human View integration diagram

```mermaid
flowchart TB
  subgraph Human_View
    BrainSelector[Brain selector]
    Hall[Hall cards]
    Navigation[Shared navigation shell]
    LivingTree[LivingTree file tree]
    GraphCanvas[GraphCanvas node link view]
    Massif[MASSIF proof topography lens]
  end

  BrainSelector --> Hall
  BrainSelector --> Navigation
  Navigation --> LivingTree
  Navigation --> GraphCanvas
  Navigation --> Massif

  subgraph Shared_Data
    Snapshot[Existing graph snapshot]
    ToolCalls[Existing tool calls]
    Tokens[Shared UI tokens]
  end

  Snapshot --> LivingTree
  Snapshot --> GraphCanvas
  Snapshot --> Massif
  ToolCalls --> LivingTree
  ToolCalls -. optional full layer lens .-> Massif
  Tokens --> Hall
  Tokens --> LivingTree
  Tokens --> GraphCanvas
  Tokens --> Massif
```
