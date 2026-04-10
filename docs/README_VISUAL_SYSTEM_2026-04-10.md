# README Visual System for `m1nd`

Date: `2026-04-10`  
Scope: `README.md` only, but written so the same system can extend to `m1nd.world`, the wiki, release notes, and social launch assets.

## Why this document exists

The current README has strong product language, but the visual layer is still inconsistent.

Right now the visuals mix:

- one highly stylized cyber image
- one comparison graphic that has been iterated but not fully stabilized
- one older systems diagram in a different visual language
- several sections with no supporting visual at all

That creates three problems:

1. the README does not yet feel like a single product story
2. the category claim is stronger than the imagery supporting it
3. the visual language drifts between "AI cyber art", "diagram", and "docs graphic"

This document defines the full image system required to make the README feel intentional, premium, and category-defining.

---

## Core visual thesis

`m1nd` should not look like:

- a generic AI coding tool
- a dashboard vendor
- a cyberpunk wallpaper brand
- a graph database screenshot
- a collection of neon box diagrams

`m1nd` should look like:

- a new systems category
- a cognition layer made visible
- a tool that gives agents orientation before action
- something more editorial and infrastructural than "AI art"

The visual language should communicate:

- code
- docs
- concepts
- change
- structural truth
- operational context
- calm control instead of noisy search

The emotional movement should always be:

`blindness -> orientation`

not:

`boring docs -> flashy sci-fi`

---

## Visual principles

### 1. Prefer systems over scenes

Use images that feel like:

- maps
- fields
- layers
- signal paths
- structural transitions

Avoid images that feel like:

- random robots
- generic futuristic cities
- hacker stock art
- anime neon chaos

### 2. Prefer editorial composition over fake UI

The most common failure mode is AI-generated pseudo-product UI:

- random cards
- fake widgets
- meaningless panels
- boxes with glow
- symbols that look functional but say nothing

We should instead use:

- deliberate negative space
- a few large compositional moves
- one primary idea per image
- text overlays only where they add product clarity

### 3. Text in-image is allowed, but only when it earns its keep

There are two good lanes:

1. image generated without text, then typography composited cleanly afterward
2. image generated from a strict JSON layout spec when Nano Banana can reliably render exact text

Default preference:

- use generated text only for simple high-level headings
- use manual overlay for exact product language that must not drift

### 4. One README, one family

All README images should share:

- the same color family
- the same contrast philosophy
- the same feeling of "operational calm"
- the same rendering quality

Do not mix:

- playful illustration
- hard technical blueprint
- glossy cyber art
- startup collage

unless one master art direction clearly unifies them.

---

## Brand visual language for `m1nd`

### Palette

Primary darks:

- `#050814`
- `#08101d`
- `#0b1322`

Signal colors:

- structural cyan: `#00f5ff`
- graph violet: `#7b61ff`
- verified green: `#00ff88`
- risk amber: `#ffb700`
- failure coral: `#ff5c76`

Use signal colors as accents, not floods.

### Material language

The world should feel like:

- glassless
- dense
- precise
- electrically alive
- calm under load

Think:

- luminous topology
- dark substrate
- thin spectral seams
- restrained glow
- expensive contrast

### Typography

README-facing image typography should feel:

- human-readable
- technical
- crisp
- minimal

Recommended split:

- headline: clean sans
- support line: mono or semi-mono

Avoid:

- fake futuristic fonts
- overly rounded "AI startup" type
- stylized cyberpunk typefaces

---

## README section map

This is the visual map of the current README:

1. logo
2. hero framing
3. tool/client credibility strip
4. key visual
5. before-vs-after comparison
6. `Why m1nd`
7. `Why First`
8. `What m1nd Operationalizes`
9. `What Ships Today`
10. `Quick Start`
11. `Make It The First Layer`
12. `Proof`
13. `Where It Fits`
14. `Architecture At A Glance`
15. learn-more / footer sections

Not every section needs an image.

But the README does need a coherent visual backbone that carries the user through:

`identity -> problem -> solution -> product surface -> adoption -> architecture`

---

## Image inventory

This is the full image system recommended for the README.

### P0: must-have images

These are the minimum assets needed for the README to feel complete.

| ID | Section | Purpose | Ratio | Suggested file |
|---|---|---|---|---|
| `R01` | Hero | establish category, tone, and product mystery | `16:10` or `3:2` | `.github/m1nd-key-visual-v2.jpeg` |
| `R02` | Before vs After | show the transition from blind exploration to grounded action | `16:9` | `.github/m1nd-agent-first-map-v2.jpeg` |
| `R03` | What m1nd Operationalizes | show code + docs + concepts + change becoming one system | `16:9` | `.github/m1nd-operability-surface-v2.png` |
| `R04` | Architecture At A Glance | show crate/system relationship clearly | `16:9` | `.github/m1nd-architecture-overview.png` |

### P1: high-value supporting images

These are not strictly required for a first pass, but they make the README much stronger.

| ID | Section | Purpose | Ratio | Suggested file |
|---|---|---|---|---|
| `R05` | Why m1nd | show the cost of stateless repo wandering | `16:9` | `.github/m1nd-orientation-tax.png` |
| `R06` | Why First | show where m1nd sits before search/edit/review/docs/ops | `16:9` | `.github/m1nd-first-layer.png` |
| `R07` | What Ships Today | make the 3 jobs legible at a glance | `16:9` | `.github/m1nd-jobs-surface.png` |
| `R08` | Quick Start | show ingest -> activate -> learn as a visual pipeline | `16:9` | `.github/m1nd-quickstart-flow.png` |
| `R09` | Where It Fits | position m1nd between LSP, compiler, security, docs, and runtime tools | `16:9` | `.github/m1nd-market-gap.png` |

### P2: optional premium system

These are launch-grade, site-grade, or social-grade assets that extend the README system.

| ID | Section | Purpose | Ratio | Suggested file |
|---|---|---|---|---|
| `R10` | Hero alt | cleaner editorial fallback for dark-mode and press kits | `16:9` | `.github/m1nd-key-visual-editorial.png` |
| `R11` | L1GHT lane | make docs/specs/concepts feel first-class, not secondary | `16:9` | `.github/m1nd-light-lane.png` |
| `R12` | Federation | show multi-repo and cross-domain graph stitching | `16:9` | `.github/m1nd-federation-map.png` |
| `R13` | Audit/runtime | show daemon, alerts, drift, continuity | `16:9` | `.github/m1nd-runtime-loop.png` |
| `R14` | Social teaser | compressed one-shot version for launch posts | `1:1` | `.github/social/m1nd-square-launch.png` |
| `R15` | OpenGraph | README-consistent social preview card | `1200x630` | `.github/social/m1nd-og-card.png` |

---

## Detailed asset briefs

## `R01` Hero Key Visual

### Job

This is the category-establishing image.

It should make the reader feel:

- this is not a toy
- this is not a code assistant skin
- this is a new systems layer

### Message

`m1nd` is the intelligence layer that makes code, docs, and change operable.

### Composition

Use one of these two approaches:

1. a dark substrate with a central signal object emerging from code, docs, and graph paths
2. a panoramic editorial scene where multiple technical surfaces collapse into one calm operating field

### Must include

- a strong sense of convergence
- negative space for hero copy
- restrained brand colors
- premium rendering quality

### Must avoid

- humanoid robots
- cityscapes unless conceptually justified
- stock "AI brain" tropes
- text-heavy art

### Prompt seed

```text
Create a premium dark editorial hero visual for a software intelligence brand.
Show code, docs, and system structure converging into one calm operational field.
No humanoid robots, no cityscape, no fake UI, no dashboard, no readable text.
Use a very dark blue-black substrate, restrained cyan/violet/emerald signal light,
and a sense of structural orientation emerging from noise.
Minimal, architectural, expensive, precise, category-defining.
Leave clean negative space for headline overlay.
```

### Acceptance criteria

- reader can feel "new category" before reading the paragraph
- image still works when reduced to README width
- no cheesy AI tropes

---

## `R02` Before vs After m1nd

### Job

This is the most important explanatory image in the README.

It should answer:

`What actually changes when an agent uses m1nd?`

### Message

Without `m1nd`, an agent wanders.  
With `m1nd`, it acts with structure and proof.

### Composition

Split comparison.

Left:

- maze
- broken trails
- wasted motion
- friction
- fragmentation

Right:

- topology
- signal paths
- orientation
- convergence
- calm control

### Text treatment

Prefer exact manual overlay for:

- headline
- subtitle
- left title + caption
- right title + caption
- footer summary

Text in-image from Nano Banana can be used as a base if it comes out clean, but final README asset should be corrected manually when needed.

### Canonical copy

- `Before vs After m1nd`
- `Without structure, agents wander. With m1nd, they act with proof.`
- `STATELESS EXPLORATION`
- `search -> open -> search again -> guess`
- `M1ND-GROUNDED ACTION`
- `ingest once -> understand structure -> act with proof`
- `Left: repeated reads, guessed blast radius, risk discovered too late.`
- `Right: graph truth, connected context, less waste, safer change.`

### Prompt seed

```text
Create a premium 16:9 editorial comparison visual.
Left side represents stateless code wandering: red-black maze fragments, broken search trails,
file shards, wasted motion, friction. Right side represents structural understanding:
cyan-violet graph constellations, clear pathways, calm connected systems, proof-driven orientation.
No fake dashboard UI, no generic boxes, no robot, no city, no extra symbols.
Large clean top area for headline overlay.
```

### Acceptance criteria

- concept is understandable in under 2 seconds
- left and right feel like different operational states, not color swaps
- no pseudo-UI clutter

---

## `R03` What m1nd Operationalizes

### Job

This image should explain the scope of the product better than a bullet list can.

### Message

`m1nd` does not just index code.  
It turns code, docs, concepts, and change into one operable surface.

### Composition

Preferred structure:

- four outer domains:
  - code
  - docs
  - concepts
  - change
- one inner converged system:
  - `m1nd`
- one outer action ring:
  - search
  - review
  - edit
  - operate

This should not look like a corporate hub-and-spoke diagram.
It should feel like an operating field.

### Text in image

Allowed, because the set is simple and structural.

### Canonical terms

- `code`
- `docs`
- `concepts`
- `change`
- `m1nd`
- `search`
- `review`
- `edit`
- `operate`

### Prompt seed

```text
Create a premium systems visual showing four technical domains converging into one intelligence layer.
Domains: code, docs, concepts, change.
Center: m1nd.
Outer action ring: search, review, edit, operate.
The image should feel like one operating surface, not a business diagram.
Dark substrate, luminous topology, restrained cyan violet green accents, precise and calm.
```

### Acceptance criteria

- readers understand "goes beyond code"
- feels systemic, not slideware
- labels are legible and few

---

## `R04` Architecture At A Glance

### Job

Give technical readers a clean mental model of the crates.

### Message

There are three core crates and one auxiliary bridge.

### Composition

Prefer clarity over atmosphere here.

This should be:

- cleaner
- flatter
- more technical
- less cinematic

than the hero visuals.

### Elements

- `m1nd-core`
- `m1nd-ingest`
- `m1nd-mcp`
- `m1nd-openclaw`

Optional second row:

- ingest adapters
- MCP runtime
- graph engine
- HTTP/UI surface

### Style

Not a screenshot.  
Not a wireframe.  
Not fake code.

Think "technical launch diagram", not "docs default block chart".

### Prompt seed

```text
Create a crisp technical architecture visual for a Rust workspace with three core crates and one auxiliary bridge.
Show m1nd-core, m1nd-ingest, m1nd-mcp, and m1nd-openclaw in a clean compositional relationship.
Dark background, subtle signal color, exact labels, minimal decorative glow.
Readable at README scale.
```

### Acceptance criteria

- instantly readable
- technically respectable
- not generic cloud architecture art

---

## `R05` Orientation Tax

### Job

Visualize the hidden cost described in `Why m1nd`.

### Message

Agents waste time and context rebuilding the same system shape again and again.

### Composition

Possible approaches:

- spiral of repeated grep/open/read loops collapsing inward
- one agent cursor trapped in recursive branches
- file shards and arrows forming a treadmill

### Suggested usage

This can sit below `Why m1nd` or become a social explainer asset.

---

## `R06` First Layer

### Job

Make the "before search / before edit / before review / before docs / before ops" thesis visual.

### Message

`m1nd` sits before action.

### Composition

Five outward action lanes:

- search
- edit
- review
- docs
- ops

One first-stop layer before them:

- `m1nd`

The visual should make that ordering emotionally obvious.

---

## `R07` Jobs Surface

### Job

Give an executive summary of the three jobs:

- understand the system
- predict and verify change
- keep context alive over time

### Composition

Three vertically aligned or horizontally linked tiles, each with:

- one short title
- one sentence
- one lightweight glyph

Use this only if the README still feels too text-heavy after the P0 assets land.

---

## `R08` Quick Start Flow

### Job

Make the onboarding path feel easy:

- ingest
- ask
- reinforce

### Composition

Small, clean, terminal-to-graph ribbon.

Do not turn this into a huge tutorial diagram.

It should feel like:

- 15 second setup
- low friction
- clear sequence

---

## `R09` Where It Fits

### Job

Show the market gap.

### Message

`m1nd` is not replacing:

- LSP
- compiler
- test runner
- security suite
- observability stack

It fills the agent-orientation gap before action.

### Composition

Preferred:

- a field map, not a quadrant
- established tools orbiting around the system
- `m1nd` occupying the orientation layer between them and the agent

Avoid standard Gartner-style quadrants.

---

## `R10` Editorial Hero Alt

This is the clean fallback hero if the main key visual feels too noisy.

Use it for:

- README fallback
- press screenshots
- launch threads
- docs header

This version should be:

- sparser
- more typographic
- less illustrative

---

## `R11` L1GHT Lane

### Job

Show that docs/specs/concepts are first-class.

### Message

`L1GHT` is not an appendix. It is part of the operable system.

### Composition

Markdown/spec fragments binding into code nodes and graph edges.

This image is especially useful if we want to push beyond the "coding agents only" frame.

---

## `R12` Federation

### Job

Show multi-repo and cross-domain intelligence.

### Message

`m1nd` can operate across repo boundaries and document boundaries.

### Composition

Multiple dark clusters stitched by identity, dependency, and semantic links.

---

## `R13` Runtime Loop

### Job

Show audit, daemon, alerts, and continuity.

### Message

`m1nd` is not a one-shot query engine. It keeps context alive.

### Composition

Closed loop:

- watched roots
- drift detection
- alert surfacing
- audit
- continuity

Do not make this feel like a monitoring dashboard.

---

## `R14` Social Teaser

Square teaser version for:

- X
- LinkedIn
- Discord
- launch cards

It should compress the README thesis into one frame:

`Blind agent loop -> grounded agent loop`

---

## `R15` OpenGraph Card

This is the share card for links.

### Required content

- `m1nd`
- `The software intelligence layer for AI agents`
- one compact visual cue for `code + docs + change`

Keep it cleaner than the README images.

---

## Recommended order of production

### Phase 1: repair the README backbone

Create first:

1. `R01`
2. `R02`
3. `R03`
4. `R04`

These four are enough to make the README feel coherent.

### Phase 2: add explanatory depth

Create next:

5. `R06`
6. `R07`
7. `R09`
8. `R11`

### Phase 3: launch system

Create last:

9. `R14`
10. `R15`
11. optional alternates and variants

---

## Production workflow

## Lane A: generate art first, overlay text later

Use this for:

- `R01`
- `R02`
- `R05`
- `R06`
- `R09`
- `R11`
- `R12`
- `R13`

Why:

- exact product copy matters
- AI text is still inconsistent under pressure

## Lane B: use strict JSON prompt with text rendered by Nano Banana

Use this only when:

- label set is short
- structure is simple
- there is strong benefit to the text being native to the image

Good candidates:

- `R03`
- `R04`
- `R07`
- `R15`

## Lane C: hybrid

Generate with JSON layout spec, then patch exact copy manually if one or two labels drift.

This is currently the best lane for `R02`.

---

## Standard JSON prompt template for Nano Banana

```json
{
  "goal": "state the job of the image in one sentence",
  "style": {
    "mood": "premium, editorial, technical, calm",
    "avoid": [
      "fake dashboard UI",
      "extra text",
      "misspellings",
      "random boxes",
      "generic cyberpunk clutter"
    ]
  },
  "canvas": {
    "aspect_ratio": "16:9",
    "background": "very dark blue-black"
  },
  "layout": {
    "type": "describe composition in one sentence"
  },
  "text_rules": {
    "render_exact_text": true,
    "no_extra_text_anywhere": true,
    "all_text_must_be_crisp_and_legible": true
  },
  "text": [
    {"role": "headline", "value": "Exact copy here"}
  ]
}
```

---

## Global acceptance rubric

Every README image should be reviewed against these questions:

### Category

- does this feel like a new infrastructure category, not an AI gimmick?

### Clarity

- can the reader understand the message in under 3 seconds?

### Brand

- does it feel like the same world as the other `m1nd` images?

### Restraint

- is there too much glow, too many boxes, too much pseudo-UI?

### Legibility

- is every required word readable at README width?

### Reusability

- can this asset also work in docs, social, and launch collateral?

If the answer is "no" to more than one of those, the image is not ready.

---

## Current README image assessment

## Source images already available in `~/Downloads`

There are already `5` new Gemini-generated images in `/Users/cosmophonix/Downloads`.

These are important because they are better starting points than forcing fresh directions from zero every time.

### Inventory

| File | Size | What it currently looks like | Best candidate role |
|---|---|---|---|
| `Gemini_Generated_Image_kv5kz5kv5kz5kv5k.jpeg` | `2048x2048` | central `m1nd` medallion with clean negative space and outgoing signal beams | `R01` hero alt / social teaser source |
| `Gemini_Generated_Image_dxemjddxemjddxem.jpeg` | `2752x1536` | three-card systems flow: understand -> predict -> verify | `R07` jobs surface or `R06` first-layer support visual |
| `Gemini_Generated_Image_teaioteaioteaiot.jpeg` | `2752x1536` | blind agent loop -> grounded action composition with strong central convergence | `R02` before-vs-after source |
| `Gemini_Generated_Image_k93xeek93xeek93x.jpeg` | `2752x1536` | radial spiral of `grep/open/read` recursion | `R05` orientation tax source |
| `Gemini_Generated_Image_dwcmvrdwcmvrdwcm.jpeg` | `2752x1536` | crate/box architecture image with labeled modules | `R04` architecture overview source |

### Recommendation

Do not treat the `Downloads` set as final publish-ready assets.

Treat them as:

- composition winners
- semantic winners
- direction anchors

Then convert them into final README assets by:

1. choosing the right source image for the right job
2. correcting any bad text or duplicated labels
3. normalizing them into one brand system
4. exporting repo-owned final assets under `.github/`

### Best mapping from the current `Downloads` batch

#### Use as primary source for `R01`

- `Gemini_Generated_Image_kv5kz5kv5kz5kv5k.jpeg`

Why:

- strongest negative space
- most premium silhouette
- least diagram clutter
- easiest to convert into a clean hero with controlled typography

#### Use as primary source for `R02`

- `Gemini_Generated_Image_teaioteaioteaiot.jpeg`

Why:

- already expresses the product thesis
- left/right movement is clear
- central `m1nd` convergence is stronger than the interim repo comparison asset

Needs:

- exact copy correction
- cleanup of incidental micro-text
- one final editorial overlay pass

#### Use as primary source for `R05`

- `Gemini_Generated_Image_k93xeek93xeek93x.jpeg`

Why:

- best visualization of recursive repo wandering
- excellent support visual for the "orientation tax" section

#### Use as primary source for `R07`

- `Gemini_Generated_Image_dxemjddxemjddxem.jpeg`

Why:

- best fit for the 3-job framing
- already reads like a structured sequence

Needs:

- rename/correct wording to our canonical product language if necessary

#### Use as primary source for `R04`

- `Gemini_Generated_Image_dwcmvrdwcmvrdwcm.jpeg`

Why:

- it is already trying to be an architecture diagram

Needs:

- fix mislabeled duplicate `m1nd-core`
- normalize spacing and crate relationships
- convert from "AI-generated architecture attempt" into a final precise technical asset

### What to ignore for now

Do not use the square `2048x2048` source as-is in the README body.

Use it only as:

- hero seed
- social seed
- OpenGraph seed

because the README itself benefits more from wide narrative compositions.

## Existing asset: `.github/m1nd-key-visual.png`

Strengths:

- memorable
- high energy
- brand personality

Weaknesses:

- too scene-based
- too close to cyber art
- not enough structural/product meaning

Decision:

- keep as temporary hero if needed
- replace with `R01 v2`

## Existing asset: `.github/m1nd-agent-first-map.png`

Strengths:

- communicates the product idea
- now uses a cleaner generated base

Weaknesses:

- still a transitional asset, not final brand language

Decision:

- acceptable interim asset
- replace with `R02 v2` once final direction is approved

## Existing asset: `.github/m1nd-operability-surface.svg`

Strengths:

- useful structure
- coherent labeling

Weaknesses:

- old visual language
- too diagrammatic
- less premium than the rest of the story

Decision:

- replace with `R03 v2`

---

## Minimal image system for next pass

If we only do the minimum serious upgrade, create these four next:

1. `R01` new hero
2. `R02` final before-vs-after
3. `R03` operability surface replacement
4. `R04` architecture overview

That alone will make the README feel substantially more complete and category-worthy.

---

## Final recommendation

Do not treat the README as "one hero plus one diagram".

Treat it as a product narrative with a visual backbone:

- identity image
- explanatory transition image
- scope image
- architecture image

That is the smallest complete system.

Anything less keeps the README looking like a strong text document with ad hoc art.
