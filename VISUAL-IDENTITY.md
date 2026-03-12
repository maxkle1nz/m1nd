# m1nd — Visual Identity Brief

**Product:** m1nd — Cognitive Graph Engine for LLM Agents
**Parent Brand:** COSMOPHONIX INTELLIGENCE
**Creator:** MAX ELIAS KLEINSCHMIDT

---

## 1. Logo Concepts

### Concept A: "The Activation" (Recommended)

**Visual description:**
A compact network graph mark built from 7 nodes and their connections. The central node is the "1" — literally a vertical stroke that doubles as both a graph node and the numeral 1. From this central stroke, 6 smaller circular nodes radiate outward at irregular but balanced intervals, connected by thin edges. The key detail: the nodes are not uniform. They have varying opacity/brightness, creating a gradient of activation — the central "1" and its two nearest neighbors are fully bright (hot signal), the middle ring is at ~60% brightness, and the outermost nodes are dim at ~25% (signal decay). This creates a visual representation of spreading activation frozen in one frame.

The "1" stroke is slightly taller than the surrounding nodes, giving it visual primacy. It has a flat top and a small serif-like horizontal bar at its base, referencing PCB pad geometry.

**Wordmark:** "m1nd" set to the right of the mark in a monospace-adjacent typeface. The "1" in the wordmark uses the same vertical stroke weight as the logo mark's central element, creating visual continuity. Letters "m", "n", "d" are lowercase, neutral weight.

**What it represents:**
- The propagation of signal through a knowledge graph (spreading activation)
- The "1" as the digital, precise, singular intelligence at the center
- Decay gradient = the engine's core behavior (signal fades with distance)
- Irregular node placement = organic graph topology, not rigid grids

**How it scales:**
- **Favicon (16x16, 32x32):** Central "1" stroke only, with 2 bright dots flanking it. Three elements total. The stroke is 2px wide with rounded cap.
- **GitHub avatar (200x200):** Full 7-node mark without wordmark. Background is `#0A0E17`. Nodes rendered with glow effect at their respective brightness levels.
- **Full logo (horizontal):** Mark + "m1nd" wordmark + optional tagline "cognitive graph engine" in small caps below the wordmark.
- **Full logo (stacked):** Mark centered above "m1nd" wordmark for square layouts.

**Color palette for this concept:**
- Central "1" and hot nodes: `#00E5A0` (activation green — the signal)
- Mid-decay nodes: `#00E5A0` at 60% opacity
- Far-decay nodes: `#00E5A0` at 25% opacity
- Edges: `#1A3A4A` (dark teal — the wiring)
- Background: `#0A0E17` (deep space — the void the signal travels through)

---

### Concept B: "The Differential"

**Visual description:**
Two overlapping waveforms rendered as clean vector paths — one traveling left-to-right (hot signal), one inverted and offset (cold signal / noise). Where they overlap, the intersection area is subtracted, leaving only the clean differential signal highlighted. This is a direct visualization of XLR noise cancellation.

The mark is horizontally oriented, roughly 3:1 aspect ratio in its compact form. The hot waveform is rendered as a smooth sine-like curve with 2.5 visible periods. The cold waveform mirrors it from below but at a different frequency (visually: the cold wave has tighter oscillation — referencing F_HOT=1.0 vs F_COLD=3.7 from the actual engine constants). The differential result — the clean signal — is rendered as a bold filled region between the two where hot exceeds cold.

Below the waveform pair, the text "m1nd" sits in a monospace typeface. The "1" is rendered as a thin vertical line that extends upward to touch the baseline of the waveform, connecting the logotype to the mark like a probe touching a signal.

**What it represents:**
- XLR differential noise cancellation (hot + cold = clean signal)
- Signal processing heritage (COSMOPHONIX = cosmos + phonics)
- The precision of extracting truth from noise — what the engine actually does
- Two frequencies = the actual F_HOT/F_COLD spectral design

**How it scales:**
- **Favicon:** Simplified to a single bold sine half-period with a vertical "1" crossing it at the peak. Two elements: curve + line.
- **GitHub avatar:** Full differential pair cropped to a square frame, centered on the intersection region. No wordmark.
- **Full logo:** Waveform mark + "m1nd" wordmark below.

**Color palette for this concept:**
- Hot signal: `#00E5A0` (the true signal)
- Cold signal: `#FF3366` (noise — warm pink, visually "wrong" but energetic)
- Differential result: `#FFFFFF` or `#00E5A0` depending on background
- Background: `#0A0E17`

---

### Concept C: "The Circuit Trace"

**Visual description:**
A PCB-inspired mark. The letter forms of "m1nd" are constructed entirely from circuit board traces — horizontal and vertical copper paths with 90-degree or 45-degree bends, connected by round vias (solder pads) at junction points. The "1" is the simplest trace: a single vertical via-to-via connection, while "m", "n", and "d" use more complex routing with multiple bends.

At 4 specific junction points within the letterforms, the vias glow — they are "activated" nodes. These glow points are placed at: the first hump of "m", the center of "1", the junction of "n", and the bowl of "d". Thin traces extend outward from the letterforms into the surrounding space, terminating in smaller vias — representing the graph extending beyond the visible frame.

The overall impression is that the word "m1nd" IS a circuit board, and some of its nodes are currently firing.

**What it represents:**
- Max's core metaphor: "software repos should be navigated like PCB circuits"
- The fusion of language/naming ("mind") with engineering substrate (circuit board)
- Active nodes within a larger system
- The traces extending beyond the frame = the graph is always larger than what you see

**How it scales:**
- **Favicon:** A single glowing via (circle) with 4 short trace stubs extending from it at 0/90/180/270 degrees. Clean, geometric, unmistakable.
- **GitHub avatar:** The full "m1nd" circuit trace at high detail, background `#0A0E17`, traces in `#1A6B50` (dark copper-green), glowing vias in `#00E5A0`.
- **Full logo:** The circuit-trace "m1nd" functions as both mark and wordmark simultaneously. Optional subtitle "cognitive graph engine" in a clean sans-serif below.

**Color palette for this concept:**
- Traces (inactive): `#1A6B50` (oxidized copper green)
- Vias (active/glowing): `#00E5A0` (activation green)
- Vias (inactive): `#2A4A3A` (dark matte green)
- Substrate/background: `#0A0E17` (PCB dark)
- Solder mask accent: `#0D2818` (the dark green of a real PCB solder mask)

---

## 2. Color System

### Primary Brand Color

**Activation Green: `#00E5A0`**
- RGB: 0, 229, 160
- HSL: 162, 100%, 45%

This is the color of signal. It reads as electric, precise, alive — but not aggressive. It sits between cyan and green, avoiding the overused blue of dev tools and the cliche neon green of "hacker" aesthetics. It references oscilloscope traces, terminal phosphor, and bioluminescence simultaneously.

It communicates: *active, intelligent, signal (not noise), alive*.

### Secondary Palette

| Name | Hex | Usage | Reasoning |
|------|-----|-------|-----------|
| **Deep Void** | `#0A0E17` | Primary background, dark UI | The space through which signals travel. Near-black with a cool blue undertone — darker and more intentional than pure `#000`. |
| **Cold Signal** | `#FF3366` | Error states, noise visualization, anti-seed indicators, destructive actions | The "cold" frequency in XLR cancellation. Warm pink-red — noise that gets cancelled. Used sparingly. |
| **Trace Copper** | `#1A6B50` | Secondary elements, inactive states, borders, subtle UI | Oxidized copper. The wiring that carries no current yet. Technical, muted, grounding. |
| **Resonance Gold** | `#F5A623` | Warnings, highlights, resonance indicators, Hebbian reinforcement | Standing wave energy. Warm, attention-drawing without being alarming. Used for plasticity/learning states. |
| **Semantic Silver** | `#8B9DAF` | Body text on dark backgrounds, secondary labels, metadata | Neutral intelligence. Readable, recessive, professional. |

### Extended Neutrals

| Name | Hex | Usage |
|------|-----|-------|
| Surface 0 (deepest) | `#0A0E17` | Page background |
| Surface 1 | `#111827` | Card/panel background |
| Surface 2 | `#1F2937` | Elevated surface, input fields |
| Surface 3 | `#374151` | Borders, dividers |
| Text Primary | `#F3F4F6` | Headings, primary content |
| Text Secondary | `#8B9DAF` | Descriptions, metadata |
| Text Muted | `#4B5563` | Disabled, placeholder |

### Light Mode Variants

Light mode inverts the surface hierarchy but keeps the same accent colors:

| Name | Hex | Usage |
|------|-----|-------|
| Surface 0 (lightest) | `#FAFBFC` | Page background |
| Surface 1 | `#F3F4F6` | Card background |
| Surface 2 | `#E5E7EB` | Elevated surface |
| Surface 3 | `#D1D5DB` | Borders |
| Text Primary | `#0A0E17` | Headings |
| Text Secondary | `#4B5563` | Descriptions |
| Activation Green (light mode) | `#00B37D` | Slightly darkened for contrast on white — same hue, reduced lightness |
| Cold Signal (light mode) | `#E6194B` | Darkened for readability |

### Color Accessibility Notes

- `#00E5A0` on `#0A0E17` = contrast ratio 11.2:1 (AAA compliant)
- `#8B9DAF` on `#0A0E17` = contrast ratio 5.8:1 (AA compliant)
- `#F3F4F6` on `#0A0E17` = contrast ratio 16.4:1 (AAA compliant)
- Light mode `#00B37D` on `#FAFBFC` = contrast ratio 3.2:1 (use only for large text/icons; pair with dark text for body)

---

## 3. Typography

### Heading Font: **JetBrains Mono**

- Source: [Google Fonts](https://fonts.google.com/specimen/JetBrains+Mono) / [JetBrains](https://www.jetbrains.com/lp/mono/)
- License: SIL Open Font License
- Weights to use: Bold (700) for H1, Medium (500) for H2-H3
- Why: Monospace that does not sacrifice readability. Its ligatures and geometric consistency echo the precision of the engine. The "1" glyph is distinctly differentiated from "l" and "I" — critical for a product literally named "m1nd". JetBrains Mono's "1" has a clear flag stroke that matches the logo mark's central element.
- CSS: `font-family: 'JetBrains Mono', monospace;`

### Body Font: **Inter**

- Source: [Google Fonts](https://fonts.google.com/specimen/Inter)
- License: SIL Open Font License
- Weights to use: Regular (400) for body, Medium (500) for emphasis, SemiBold (600) for labels
- Why: The most legible sans-serif available for screens. Variable font with optical sizing. Designed specifically for UI contexts. Neutral enough to not compete with the monospace headings but sharp enough to hold its own. x-height optimized for small sizes.
- CSS: `font-family: 'Inter', sans-serif;`

### Code Font: **JetBrains Mono**

- Same as headings — this creates typographic unity. Code blocks, inline code, terminal output, and headings all share the same typeface. The distinction between "heading" and "code" is made through size, weight, and color — not font family.
- Weights: Regular (400) for code blocks, Light (300) for long-form code
- CSS: `font-family: 'JetBrains Mono', monospace;`

### Type Scale (base 16px)

| Element | Size | Weight | Font | Tracking |
|---------|------|--------|------|----------|
| H1 | 32px / 2rem | 700 | JetBrains Mono | -0.02em |
| H2 | 24px / 1.5rem | 500 | JetBrains Mono | -0.01em |
| H3 | 18px / 1.125rem | 500 | JetBrains Mono | 0 |
| Body | 16px / 1rem | 400 | Inter | 0 |
| Small / Label | 14px / 0.875rem | 500 | Inter | 0.01em |
| Caption | 12px / 0.75rem | 400 | Inter | 0.02em |
| Code block | 14px / 0.875rem | 400 | JetBrains Mono | 0 |
| Inline code | 14px / 0.875rem | 400 | JetBrains Mono | 0 |

---

## 4. Visual Language

### What the Visual Style Communicates

**Signal, not noise.** Every visual element earns its place. The system's core purpose is separating relevant information from irrelevant information — the visual language mirrors this by being sparse, precise, and high-contrast. Nothing decorative. Nothing gratuitous.

**Alive, not static.** The color system implies motion and energy — nodes glow, signals propagate, connections strengthen. Even in static media, the varying node brightness and directional gradients suggest a system caught mid-computation.

**Deep, not shallow.** Dark backgrounds and restrained palettes suggest depth and seriousness. This is infrastructure, not a consumer app. The aesthetic says "this is the engine room" without being intimidating.

**Technical, not corporate.** Circuit traces, oscilloscope green, monospace type — these are the visual signatures of someone who builds things, not someone who sells things.

### Mood Board Description

Textures and images that define the brand world:

- **Oscilloscope traces on dark screens** — green phosphor waveforms on black. The original "signal made visible."
- **PCB macro photography** — copper traces, solder mask, vias at extreme close-up. The green and gold of real circuit boards.
- **Neural activation maps** — fMRI scans showing localized brain activity. Bright regions against dark tissue.
- **Network topology visualizations** — force-directed graphs with varying node sizes and edge opacity. Not the generic "connected dots" stock image — real graph layouts with community structure visible.
- **Spectrograms** — frequency-domain audio visualizations. Horizontal time axis, vertical frequency axis, intensity mapped to color. The visual representation of sound being decomposed.
- **Standing wave patterns** — Chladni plate photography. Sand forming geometric patterns on vibrating metal plates. Order emerging from vibration.
- **Dark IDE screenshots** — code as texture. The rhythm of indentation, the color of syntax highlighting, the density of information.

### Do's

- Use activation green (`#00E5A0`) as the primary accent on dark backgrounds
- Let dark space breathe — the void is part of the design, not emptiness to fill
- Use node/graph metaphors in diagrams and illustrations
- Show varying activation states (bright = active, dim = decayed, dark = inactive)
- Use monospace for anything that represents system output, data, or precision
- Maintain high contrast ratios — the brand is about signal clarity
- Use the glow effect sparingly and only on "active" elements
- Reference the 4 dimensions through color when showing multi-dimensional data: structural=green, semantic=blue (`#3B82F6`), temporal=gold (`#F5A623`), causal=purple (`#8B5CF6`)

### Don'ts

- Do not use gradients for decoration — gradients only represent activation decay or signal propagation
- Do not use rounded, bubbly, or "friendly" shapes — the geometry is angular, precise, technical
- Do not use blue as a primary color — every dev tool uses blue. m1nd is green. The signal is green.
- Do not use stock photography of any kind
- Do not use the glow effect on everything — glow means "active." If everything glows, nothing is active.
- Do not add drop shadows to UI elements — depth is communicated through surface color stepping, not shadows
- Do not use more than 2 accent colors in a single view/composition
- Do not use the Cold Signal red (`#FF3366`) for anything positive — it is always noise, error, or cancellation
- Do not use light mode as the default — dark mode is the primary context

### How It Differs from Typical Dev Tool Branding

Most developer tools converge on the same visual playbook: blue primary color, geometric sans-serif, gradient blob backgrounds, abstract "connection" illustrations, light mode default. The result is a sea of indistinguishable brands.

m1nd diverges in specific ways:

| Typical Dev Tool | m1nd |
|-----------------|------|
| Blue primary | Green primary (oscilloscope, circuit, neural) |
| Geometric sans-serif logo | Monospace + graph node hybrid |
| Gradient blobs | High-contrast dark with point-source light (glow on nodes) |
| Abstract "connection" art | Literal graph topology, actual signal visualization |
| Light mode default | Dark mode native — this is an engine room |
| Rounded corners everywhere | Mixed: round nodes + angular traces |
| Corporate-neutral tone | Technical-precise tone |
| "Illustration style" characters | No characters. Systems, signals, graphs. |

---

## 5. GitHub Presence

### Profile Picture

Use **Concept A (The Activation)** rendered at 500x500, centered in frame:
- Background: `#0A0E17`
- The 7-node mark with the central "1" stroke
- Nodes rendered with subtle glow at their activation brightness levels
- No wordmark — mark only
- 48px padding on all sides
- Export at 500x500 PNG with transparency OFF (use solid background)

### Repository Social Preview (1280x640)

Layout (left-to-right):
- **Left third (0-426px):** The full mark at large scale, vertically centered. Activation green nodes with glow on `#0A0E17` background.
- **Center/Right (426-1280px):**
  - Top: "m1nd" in JetBrains Mono Bold, 64px, `#F3F4F6`
  - Below: "cognitive graph engine" in Inter Medium, 24px, `#8B9DAF`, letter-spacing 0.08em, all caps
  - Below (32px gap): A single-line code sample in JetBrains Mono Regular, 18px, `#00E5A0`:
    `activate("auth") -> [session, jwt, middleware, user_model]`
  - Bottom: "COSMOPHONIX INTELLIGENCE" in Inter SemiBold, 14px, `#4B5563`, letter-spacing 0.12em, all caps

Background: solid `#0A0E17`. Subtle grid of dots at 5% opacity (`#1F2937`) across the entire image to reference graph topology without being distracting.

### README Badge Theme

Use shields.io badges with custom colors:

| Badge | Style | Color |
|-------|-------|-------|
| Build status | `flat-square` | Green: `#00E5A0`, label bg: `#1F2937` |
| Version/release | `flat-square` | Green: `#00E5A0`, label bg: `#1F2937` |
| License | `flat-square` | Silver: `#8B9DAF`, label bg: `#1F2937` |
| Language (Rust) | `flat-square` | Gold: `#F5A623`, label bg: `#1F2937` |
| Tests passing | `flat-square` | Green: `#00E5A0`, label bg: `#1F2937` |
| Tests failing | `flat-square` | Red: `#FF3366`, label bg: `#1F2937` |

Example shields.io URL pattern:
```
https://img.shields.io/badge/build-passing-00E5A0?style=flat-square&labelColor=1F2937
https://img.shields.io/badge/tests-159_passing-00E5A0?style=flat-square&labelColor=1F2937
https://img.shields.io/badge/lang-Rust-F5A623?style=flat-square&labelColor=1F2937
https://img.shields.io/badge/license-MIT-8B9DAF?style=flat-square&labelColor=1F2937
```

---

## 6. ASCII Art Logo

Primary ASCII logo for terminal output and README headers:

```
               ·  ·
            ·       ·
          ·    ███    ·
         ·     ███     ·
        ·      ███      ·
         ·     ███     ·
          ·    ███    ·
            ·       ·
               ·  ·

           m  1  n  d
```

Compact version for CLI startup banner:

```
    ╭─────────────────────────╮
    │                         │
    │    ·  ╻  ·    m1nd      │
    │   · ──╋── ·   v0.1.0   │
    │    ·  ╹  ·              │
    │                         │
    ╰─────────────────────────╯
      cognitive graph engine
```

Minimal single-line version for log output prefix:

```
[m1nd]
```

Extended version for splash screen or documentation header:

```
                  ○
                 /|\
            ○···/ | \···○
           /   /  |  \   \
      ○···/···○───█───○···\···○
           \   \  |  /   /
            ○···\ | /···○
                 \|/
                  ○

    ███    ███            █████
    ████  ████  ██  ████  ██  ██
    ██ ████ ██  ██  ██ ██ ██  ██
    ██  ██  ██  ██  ██ ██ █████
    ██      ██  ██  ██ ██ ██
    ██      ██  ██  ████  ██

    ── cognitive graph engine ──
      COSMOPHONIX INTELLIGENCE
```

High-fidelity version with activation decay visualization:

```
                  ░
                 ╱│╲
            ░╌╌╱ │ ╲╌╌░
           ╱   ╱  │  ╲   ╲
      ░╌╌╱╌╌▒───▓███▓───▒╌╌╲╌╌░
           ╲   ╲  │  ╱   ╲
            ░╌╌╲ │ ╱╌╌░
                 ╲│╱
                  ░

    Legend:  ███ = seed (1.0)
              ▓ = 1-hop (0.7)
              ▒ = 2-hop (0.4)
              ░ = 3-hop (0.1)
```

---

## Summary: Recommended Direction

**Logo:** Concept A — "The Activation"

It is the most versatile, the most immediately legible at small sizes, and it communicates the product's core mechanic (spreading activation) in a single glance. The central "1" element provides strong brand recognition and ties directly to the name. It works as a standalone mark, with a wordmark, and in ASCII.

Concept B (The Differential) is the most technically faithful to the XLR engine, but waveforms are harder to read at small sizes and risk being confused with audio/music branding. Reserve this visual language for internal technical documentation and data visualization.

Concept C (The Circuit Trace) is the most visually distinctive and has the strongest connection to Max's "repos are circuits" philosophy, but it requires higher resolution to read and may limit future applications. Consider this for merchandise, presentations, and large-format uses.

**Primary color:** `#00E5A0` — Activation Green. Non-negotiable.

**Typography:** JetBrains Mono for headings and code, Inter for body. Two fonts, one voice.

**Tone:** Technical. Precise. Alive. Dark.

---

*COSMOPHONIX INTELLIGENCE*
