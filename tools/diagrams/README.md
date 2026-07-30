# System diagrams

The eight diagrams in the README and the wiki are build artifacts, not
hand-drawn files. `build.mjs` emits all sixteen SVGs (each scene in a dark and a
light variant) into `docs/assets/diagrams/`, and `validate.mjs` is the gate.

```sh
node tools/diagrams/build.mjs        # writes docs/assets/diagrams/*.svg
node tools/diagrams/validate.mjs     # exit 0 = the set is valid
```

Both accept an output directory as the first argument if you want to render
somewhere else.

## Why generated

The first pass was hand-written SVG with absolute coordinates. Text width was
never measured, so labels crossed frames and each other; fixing one collision
pushed the next one sideways, and the gate counted 81 typography violations
across the set.

The generator sizes every box from the text it carries (the monospace advance is
exactly 0.6em for the SF Mono / Menlo stack it names), places every label in a
gutter measured to hold it, and emits both themes from a single geometry pass.
Overlap is no longer a defect to fix; it is a state the layout cannot reach.

## What the gate checks

The palette (twelve hexes, nothing else), well-formed XML, a viewBox, `<title>`
and `<desc>` for readers without sight, no gradients or filters or animation, no
decorative opacity, under 60 KB per file, and typography: text inside the canvas,
text never crossing a frame, at least 8 px between a label and the box around it,
and no two labels overlapping. The validator estimates text width more
conservatively (0.62em) than the generator draws it (0.60em), so a pass there
leaves slack in the real render.

## Editing

Change the scene functions in `build.mjs`, re-run both commands, and commit the
regenerated SVGs. Do not hand-edit the files under `docs/assets/diagrams/`: the
next build overwrites them.
