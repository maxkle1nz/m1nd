---
Protocol: L1GHT/1.0
Node:     M1ND L1GHT Design System Usage
State:    active
Color:    resonance
Glyph:    ⛛
Completeness: canonical
Proof:    grounded
Depends on:
- m1nd.ingest (light adapter)
- m1nd.activate
- m1nd.search
- ANOMALY Design Doctrine
---

# Using M1nd With a L1GHT Design System

This document shows how M1nd was used to ground the **ANOMALY Design System** — a set of modular UI components — using the `light` adapter and boot memory tools.

The same pattern applies to any design system, knowledge corpus, or component library.

---

## What L1GHT/1.0 Is

A markdown frontmatter convention. Each file declares:

```yaml
---
Protocol: L1GHT/1.0
Node:     <human-readable node name>
State:    active | draft | deprecated
Color:    <matter state label>
Depends on:
- <other node name>
---
```

Below the frontmatter, the body contains code blocks tagged with the component they document (e.g. `<css-variables>`, `<html-structure>`, `<javascript-logic>`).

M1nd's `light` adapter parses both the frontmatter metadata and the body, creating structured nodes with dependency edges automatically.

---

## Session Pattern Used

### 1. Ingest the corpus

```
m1nd.ingest(
  path = "/path/to/anomaly/docs/design/ssot/modules",
  adapter = "light",
  mode = "merge"
)
```

All L1GHT node files are indexed. Dependency edges are resolved from `Depends on:` fields.

### 2. Verify retrieval

```
m1nd.seek("How do I build a navigation component for ANOMALY?")
```

Returns `04-LIQUID-NAVIGATION.md` as the top result, with its CSS and JS blocks surfaced.

```
m1nd.activate("quantum cursor anomaly design")
```

Returns `02-QUANTUM-CURSOR.md` as the highest-activation node.

### 3. Set a boot anchor

```
m1nd.boot_memory(
  action = "set",
  key = "ANOMALY_DESIGN_SYSTEM",
  value = {
    entry_node: "modules/00-AGENT-ASSEMBLY-GUIDE.md",
    modules_directory: "docs/design/ssot/modules/"
  }
)
```

Any agent starting a session can call `m1nd.boot_memory(action="get", key="ANOMALY_DESIGN_SYSTEM")` to recover the entry point without re-reading the whole corpus.

---

## What Was Built

Using only the module nodes, two complete interfaces were assembled:

| File | Description |
|---|---|
| `test-anomaly-interface.html` | 3-pane layout: Liquid Nav + Void Terminal + Truth Cluster |
| `anomaly-observer.html` | Header + Neural Topology SVG + Observatory + Telemetry Stream |

Both files are under 500 lines and required no external CSS framework.

A new module was added solely by creating a new L1GHT/1.0 markdown file in `modules/` and running `m1nd.ingest` with `mode = merge`. No other file changes required.

---

## Commit Convention

The ANOMALY repo encodes graph events directly in commit messages:

```
feat(design): add Waveform Modulator control

[⍂ node: ANOMALY.Design.QuantumControls]
[⍐ state: active]
[𝔻 evidence: New L1GHT/1.0 node (09-QUANTUM-CONTROLS.md) replacing standard input[type=range]]
[⟁ depends_on: ANOMALY.Design.VoidEnvironment]
[⟁ affects: ANOMALY.Design.AgentAssemblyGuide]
```

M1nd ingests these annotations through the `light` adapter when the repo is re-ingested after a commit.

---

## Source

ANOMALY repository: `https://github.com/maxkle1nz/ANOMALY`

Module directory: `docs/design/ssot/modules/`

Entry node: `docs/design/ssot/modules/00-AGENT-ASSEMBLY-GUIDE.md`
