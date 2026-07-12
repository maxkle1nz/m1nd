# The cockpit-chat widget — official template (`m1nd-cockpit-widget-v0`)

Versioned material. This is the ONE reference skin an agent pastes to render the
`cockpit` menu (or the human_view deep rung) as a rich, navigable card in a chat
that supports HTML widgets. It is fed by a STATE JSON (the `cockpit` verb's own
output is that JSON source) + this template; the agent pastes both.

> Law source: askGOD verdict "the navigable cockpit" (`ASKGOD-VERDICT-COCKPIT.md`)
> amendment 7 + the owner's field notes on the v3.2 prototype. Skin: h4nd
> (`~/god-hud`). The widget renders READS and navigation only — it NEVER carries
> a write.

---

## 1. The five laws of this widget (non-negotiable)

1. **No write, ever.** No button names or triggers a write verb (ratify, import,
   post, apply, memorize, …). Pointer entries (the tray) render as a DOOR that
   points to where the human acts — never as an in-widget action. Approving
   inside the widget is laundered consent (a click fabricates a message with the
   user's authority); the lowest legitimate friction is a deep-link to the tray
   card, where the click travels browser→server with no model in between.
2. **The payload is a short reference, never free text.** A button sends only
   `select <slot>` + the `menu_sig` it was rendered from — the SAME thing a
   human would type. NEVER a command string, NEVER a string interpolated from
   graph/repo content (that is a prompt-injection channel).
3. **Links by origin.** An `https://` external link is a native `<a href>`
   (opens out). A `localhost`/`127.0.0.1` link (the served UI, the tray) goes
   through the agent via `sendPrompt` — the browser widget cannot reach the
   owner's loopback, and the agent deep-links honestly.
4. **Modal lives in normal flow.** Any expanded detail / "modal" is a normal
   block with `min-height`, pushed into the document flow — NEVER `position:
   absolute`/`fixed` alone (it detaches from the transcript and clips in chat).
5. **Two skins, one geometry** (uiproof + the ASCII fallback law): the rich HTML
   skin and the plain text card (the human_view lines) carry the SAME facts and
   the SAME navigation; the widget is an enrichment, never a second source.

---

## 2. The h4nd skin (the design system tokens)

```
--breu:        #0E0D0C;   /* dark ink — the ground */
--papel:       #F2EFE9;   /* paper — the card */
--papel-2:     #E8E4DA;   /* paper, one step down (rows) */
--ink:         #1A1815;   /* body text on paper */
--ink-soft:    #6B655C;   /* secondary text */
--vermilion:   #C43422;   /* the stamp / the one alarm accent */
--line:        #0E0D0C;   /* hard 1px rule */
--grid:        24px;      /* everything snaps to a 24px grid */
--shadow-hard: 3px 3px 0 #0E0D0C;   /* solid, no blur */
--shadow-lift: 4px 4px 0 #0E0D0C;   /* raised elements */
```

House rules:
- **Hard solid shadows only** (`3px 3px 0` / `4px 4px 0`) — never a soft/blurred
  shadow; the look is printed paper on ink, not glass.
- **The vermilion stamp** is the ONLY saturated color and appears ONCE per card,
  tilted `-1deg` (`transform: rotate(-1deg)`) like a rubber stamp — reserved for
  the calling state (a raised pulse cell, a bell awaiting a stamp). Never fill a
  whole surface with it.
- **24px grid**: padding, gaps, and row heights are multiples of 24px.
- **Monospace for the wordmark + pulse** (they must align to the text card);
  the labels may use the system UI stack.
- **Theme-aware**: paper card on ink ground reads in both light and dark chat;
  the tokens above are the dark-ground default. Under a light chat, keep the
  paper card (it is the product's identity), only softening the outer ground.

---

## 3. The STATE contract (what the cockpit verb feeds)

The widget renders THIS shape verbatim (it is the `cockpit` response, trimmed to
what the skin needs). The agent pastes it into `STATE` and never edits values:

```json
{
  "schema": "m1nd-cockpit-v0",
  "depth": 0,
  "menu_sig": "mc_fd063e9a",
  "state_sig": "bell:3|map:12|coh:ok|recv:match|sv:7",
  "store_version": 7,
  "wordmark": "m1nd",
  "pulse": "╷╷╷│╷",
  "entries": [
    { "slot": 1, "kind": "pointer", "label": "the tray · 3 await your stamp",
      "door": "open the tray — the stamp lives there" },
    { "slot": 2, "kind": "read", "label": "the map · 12 blocks ratified",
      "verb": "system_blocks_snapshot", "why": "…" }
  ]
}
```

- `pulse` mirrors the human_view line-1 row; a raised cell `│` gets the vermilion
  stamp treatment, a calm cell `╷` stays ink.
- A `read` entry renders a button whose payload is `select <slot> @ <menu_sig>`.
- A `pointer` entry renders a DOOR (a labelled link), NEVER a submit button.

---

## 4. Reference render (self-contained; inline everything)

```html
<style>
  .m1c { background: var(--breu, #0E0D0C); padding: 24px; font: 14px/1.5 ui-sans-serif, system-ui; }
  .m1c__card { background: #F2EFE9; color: #1A1815; border: 1px solid #0E0D0C;
    box-shadow: 4px 4px 0 #0E0D0C; padding: 24px; max-width: 560px; }
  .m1c__head { display: flex; align-items: baseline; gap: 12px; font-family: ui-monospace, Menlo, monospace;
    border-bottom: 1px solid #0E0D0C; padding-bottom: 12px; margin-bottom: 12px; }
  .m1c__mark { font-weight: 700; }
  .m1c__pulse { letter-spacing: 2px; }
  .m1c__pulse .up { color: #C43422; }              /* raised cell = the stamp accent */
  .m1c__crumb { color: #6B655C; font-size: 12px; margin-bottom: 12px; }
  .m1c__row { display: flex; justify-content: space-between; align-items: center;
    background: #E8E4DA; border: 1px solid #0E0D0C; box-shadow: 3px 3px 0 #0E0D0C;
    padding: 12px; margin-bottom: 12px; min-height: 24px; }
  .m1c__btn { font: inherit; cursor: pointer; background: #F2EFE9; border: 1px solid #0E0D0C;
    box-shadow: 3px 3px 0 #0E0D0C; padding: 6px 12px; }
  .m1c__btn:active { box-shadow: 1px 1px 0 #0E0D0C; transform: translate(2px,2px); }
  .m1c__door { color: #C43422; text-decoration: underline; transform: rotate(-1deg);
    display: inline-block; font-weight: 600; }
  .m1c__modal { border: 1px solid #0E0D0C; box-shadow: 4px 4px 0 #0E0D0C; background: #F2EFE9;
    padding: 24px; margin-top: 12px; min-height: 96px; }  /* NORMAL FLOW, min-height — never absolute */
</style>

<div class="m1c">
  <div class="m1c__card" id="m1c-card"></div>
</div>

<script>
  // STATE is pasted by the agent from the cockpit verb output — never hand-edited.
  const STATE = /*__STATE__*/ { schema:"m1nd-cockpit-v0", depth:0, menu_sig:"mc_0",
    state_sig:"", store_version:null, wordmark:"m1nd", pulse:"╷╷╷╷╷", entries:[] };

  function renderPulse(p){
    return [...p].map(c => c === '│'
      ? '<span class="up">│</span>' : '<span>╷</span>').join('');
  }

  function render(state){
    const crumb = state.depth === 0 ? 'root' : `root › ${state.label || 'detail'}`;
    const rows = (state.entries || []).map(e => {
      if (e.kind === 'pointer') {
        // A DOOR — never a submit. Points to where the human acts.
        return `<div class="m1c__row"><span>${escapeHtml(e.label)}</span>
          <span class="m1c__door" data-door="${escapeAttr(e.door||'')}">${escapeHtml(e.door||'open')}</span></div>`;
      }
      // A READ button — payload is the SHORT reference only.
      return `<div class="m1c__row"><span>${escapeHtml(e.label)}</span>
        <button class="m1c__btn" data-slot="${e.slot}">look ›</button></div>`;
    }).join('');

    document.getElementById('m1c-card').innerHTML = `
      <div class="m1c__head">
        <span class="m1c__mark">${escapeHtml(state.wordmark || 'm1nd')}</span>
        <span class="m1c__pulse">${renderPulse(state.pulse || '╷╷╷╷╷')}</span>
      </div>
      <div class="m1c__crumb">${escapeHtml(crumb)} · ${escapeHtml(state.menu_sig || '')}</div>
      ${rows}`;

    // READ button → sends the SHORT reference a human would type (law 2).
    document.querySelectorAll('.m1c__btn').forEach(b => b.onclick = () => {
      const slot = b.getAttribute('data-slot');
      // payload = number + menu_sig, NEVER free text, NEVER graph content.
      sendPrompt(`cockpit select ${slot} @ ${state.menu_sig}`);
    });
    // DOOR → the agent deep-links (localhost via agent; https as native anchor).
    document.querySelectorAll('[data-door]').forEach(d => d.onclick = () => {
      sendPrompt(`open the tray for me`);   // a request, not a write; no interpolated graph string
    });
  }

  function escapeHtml(s){ return String(s).replace(/[&<>"]/g, m =>
    ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[m])); }
  function escapeAttr(s){ return escapeHtml(s).replace(/'/g, '&#39;'); }
  render(STATE);
</script>
```

### Notes on the reference

- **`sendPrompt` payloads are references, not commands.** `cockpit select 2 @
  mc_fd063e9a` is what a human would type; the agent re-invokes `cockpit` with
  `select:"2"` and the `menu_sig` proves the human acted on the menu it saw
  (anti-staleness). A door sends a plain request ("open the tray for me"), never
  a graph-derived string.
- **External vs localhost links.** For an `https://…` link (docs, a public page)
  render a native `<a href target="_blank" rel="noopener">` — it opens out and
  needs no agent. For a `localhost`/served-UI link, DO NOT `<a href>` (the widget
  sandbox cannot reach the owner's loopback): send a `sendPrompt` request and let
  the agent deep-link.
- **The modal (`.m1c__modal`)** is appended in the normal document flow with a
  `min-height`; it is never `position:absolute/fixed` alone — a detached modal
  clips inside a chat transcript and loses the breadcrumb.
- **Two skins, one geometry:** this widget and the plain text card
  (`m1nd ╷╷╷│╷  …`) render the SAME `menu_sig`/`state_sig` and the SAME entries;
  when the surface cannot hold the widget, the text card is the honest fallback
  (the ASCII pulse `...|.`), and nothing about the navigation changes.
