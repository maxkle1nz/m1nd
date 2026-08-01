# The README demos: six real calls against a live m1nd owner. Each one shows the
# raw response the agent receives, lights the line that matters, then dissolves
# the payload into what a human would take from it.
#
# Nothing here is illustrative. Every latency, byte count and payload line was
# captured on 2026-07-31 from the served owner on port 1338 (graph: 18,084
# nodes, 73,332 edges, 129 ingest roots) and is quoted verbatim.
#
# Each scene is emitted as its own short film so it can sit beside the paragraph
# that makes its claim, at a pace a human can actually read.
import os
import subprocess
import sys
from PIL import Image, ImageDraw, ImageFont

W, H, FPS = 1000, 560, 12
FIELD, INK, SEC, AMBER, RED, WIRE = "#161B22", "#F2F4EF", "#98A2AE", "#E8A33D", "#FF7B72", "#687585"
MENLO = "/System/Library/Fonts/Menlo.ttc"
F = lambda s: ImageFont.truetype(MENLO, s)
f_body, f_mono, f_small = F(20), F(16), F(14)
INSTALL = "npx -y @maxkle1nz/m1nd update apply --yes"
PROV = "2026-07-31 · live owner · 18,084 nodes / 73,332 edges · latency = median of 11 calls"

SCENES = {
    "impact": dict(
        verb="impact", tag="what breaks if I change this",
        call='impact({"node_id":"calibration.rs::fn::conformal_quantile"})',
        ms=173, unit="ms", size="1,909 B",
        js=[
            '"blast_radius": [',
            '  { "label": "partial_cmp", "hop_distance": 1, "signal": 0.274 },',
            '  { "label": "cmp",         "hop_distance": 2, "signal": 0.075 },',
            '  { "label": "HeapEntry",   "hop_distance": 2, "signal": 0.075 }, …',
            '],',
            '"summary": "7 nodes in blast radius."',
        ],
        hi=[1, 3], callout="activation.rs never spells conformal_quantile",
        human=[
            ("Seven dependents, ranked, with the hop that found them.", INK),
            ("", INK),
            ("Two of them live in a file that never spells the symbol.", SEC),
            ("", INK),
            ("grep cannot find those. The edges can.", AMBER),
        ],
    ),
    "north": dict(
        verb="north", tag="the orientation packet, injected at spawn",
        call='north({"task":"understand how verdicts are calibrated"})',
        ms=1964, unit="ms", size="6,755 B",
        js=[
            '"binding": { "node_count": 18084, "edge_count": 73332 },',
            '"memory": [],',
            '"honest_gaps": [',
            '  "The memory store holds 55 durable claim(s), but none',
            '   surfaced for this task — recall found no task-relevant',
            '   match, not an empty store."',
            '],',
            '"sufficiency": { "captured": 0.0726, "state": "gathering" }',
        ],
        hi=[2, 3, 4, 5, 6], callout="it names what it does not know",
        human=[
            ("Before the first prompt, the agent has the map, the memory,", INK),
            ("and the gap.", INK),
            ("", INK),
            ("Most systems tell you what they found.", SEC),
            ("This one tells you what it is missing.", AMBER),
        ],
    ),
    "refusal": dict(
        verb="the refusal", tag="a dead end that teaches the retry", err=True,
        call='seek({"query":"conformal gate"})        ← agent_id missing',
        ms=None, unit="", size="276 B",
        js=[
            '"error": "invalid_params",',
            '"message": "missing field `agent_id`;',
            '',
            '   minimal call: seek({"agent_id":"jimi",',
            '                       "query":"rate limit retry logic"});',
            '',
            '   next: Retry with the minimal valid call."',
        ],
        hi=[3, 4], callout="it hands back a call that works",
        human=[
            ("Not “no results”. It names the field it needs and returns", INK),
            ("a call that works.", INK),
            ("", INK),
            ("The agent repairs itself on the next line, with nobody", SEC),
            ("watching and nothing guessed.", SEC),
        ],
    ),
    "predict": dict(
        verb="predict", tag="a confidence graded against this repo's own history",
        call='predict({"changed_node":"…::fn::conformal_quantile"})',
        ms=172, unit="ms", size="10 predictions",
        js=[
            '"calibration": { "n": 9196, "target_alpha": 0.10,',
            '                 "tau": 0.4583,',
            '                 "measured_precision": 0.3386 },',
            '"predictions": [{ "label": "conformal_quantile_empty_is_one",',
            '                  "confidence": 0.4827 }, … ]',
        ],
        hi=[1, 4], callout="0.4827 against a cut of 0.4583",
        human=[
            ("It grades its own confidence against a cut measured on", INK),
            ("9,196 predictions from this repository.", INK),
            ("", INK),
            ("This one clears the line by 0.0244.", SEC),
            ("act, with the arithmetic attached.", AMBER),
        ],
    ),
    "seek": dict(
        verb="seek", tag="an answer that carries its own verdict",
        call='seek({"query":"conformal gate act abstain","limit":5})',
        ms=208, unit="ms", size="17 results",
        js=[
            '"results": [ { "label": "conformal_quantile", "score": 0.5089 }, … ],',
            '"sufficiency": {',
            '  "captured": 0.2420,',
            '  "why": "the strongest match left out still scores 0.44 —',
            '          relevant context did not fit"',
            '},',
            '"trust_envelope": { "calibrated": false }',
        ],
        hi=[2, 3, 4], callout="the answer grades itself",
        human=[
            ("Seventeen hits in 208 milliseconds, and then it tells", INK),
            ("on itself: this set carries 24% of the salience.", INK),
            ("", INK),
            ("An answer that reports its own insufficiency is one", SEC),
            ("you can act on.", AMBER),
        ],
    ),
    "cost": dict(
        verb="the same question, both ways", tag="what the graph is actually worth", cmp=True,
        call='"what breaks if I change conformal_quantile?"',
        ms=None, unit="", size="",
        left=[
            "WITHOUT THE GRAPH",
            "",
            "grep names 2 files",
            "16,417 lines",
            "606,278 bytes to read",
            "\u2248 151,000 tokens",
            "",
            "and it still misses the",
            "two dependents that",
            "never spell the symbol",
        ],
        right=[
            "WITH THE GRAPH",
            "",
            "one call: impact()",
            "1,909 bytes back",
            "173 ms",
            "\u2248 477 tokens",
            "",
            "7 dependents, ranked,",
            "with hop distance",
            "",
        ],
        hi=[5], callout="317\u00d7 the reading, for a worse answer",
        human=[
            ("Three hundred times the reading, for a worse answer.", INK),
            ("", INK),
            ("And every subagent you spawn pays that bill again.", SEC),
            ("", INK),
            ("The graph is injected once, at spawn, for all of them.", AMBER),
        ],
    ),
}

ORDER = ["impact", "north", "refusal", "predict", "seek", "cost"]


def head(d, scene):
    d.text((40, 28), "m1nd", font=f_body, fill=INK)
    d.text((40 + f_body.getlength("m1nd") + 20, 32), scene["verb"], font=f_small, fill=SEC)
    d.text((W - 40, 32), scene["tag"], font=f_small, fill=WIRE, anchor="ra")
    d.line([40, 58, W - 40, 58], fill=WIRE, width=2)


def panel(d, x, y, w, h, color=WIRE):
    d.rectangle([x, y, x + w, y + h], outline=color, width=3)
    d.line([x + 56, y, x + 112, y], fill=FIELD, width=7)


def tint(line):
    spans, buf, i = [], "", 0
    while i < len(line):
        ch = line[i]
        if ch == '"':
            j = line.find('"', i + 1)
            if j == -1:
                buf += ch; i += 1; continue
            tok = line[i:j + 1]
            key = j + 1 < len(line) and line[j + 1] == ":"
            spans += [(buf, WIRE), (tok, SEC if key else INK)]
            buf = ""; i = j + 1
        elif ch.isdigit() and (not buf or buf[-1] in " :[,-"):
            j = i
            while j < len(line) and (line[j].isdigit() or line[j] == "."):
                j += 1
            spans += [(buf, WIRE), (line[i:j], AMBER)]
            buf = ""; i = j
        else:
            buf += ch; i += 1
    spans.append((buf, WIRE))
    return [s for s in spans if s[0]]


def draw_spans(d, x, y, spans, force=None):
    for text, col in spans:
        d.text((x, y), text, font=f_mono, fill=force or col)
        x += f_mono.getlength(text)


def frame(scene, phase, p, path):
    img = Image.new("RGB", (W, H), FIELD)
    d = ImageDraw.Draw(img)
    head(d, scene)

    call = scene["call"]
    shown = call[:max(1, int(len(call) * min(1.0, p * 2)))] if phase == "call" else call
    d.text((40, 88), "›", font=f_body, fill=AMBER)
    d.text((66, 90), shown, font=f_mono, fill=INK)
    if phase == "call" and int(p * 12) % 2 == 0:
        cx = 66 + f_mono.getlength(shown)
        d.line([cx + 3, 90, cx + 3, 108], fill=AMBER, width=2)

    if scene["ms"]:
        v = scene["ms"] if phase != "json" else int(scene["ms"] * p)
        d.text((W - 40, 92), f"{v:,} {scene['unit']}   {scene['size']}", font=f_small,
               fill=AMBER, anchor="ra")
    elif scene["size"] and phase != "call":
        d.text((W - 40, 92), scene["size"], font=f_small, fill=AMBER, anchor="ra")

    top, lh = 146, 26
    n = max(len(scene["left"]), len(scene["right"])) if scene.get("cmp") else len(scene["js"])
    lit = scene.get("hi", [])
    if phase in ("json", "light", "hold", "morph"):
        panel(d, 32, top - 18, W - 64, n * lh + 44, color=RED if scene.get("err") else WIRE)
        vis = n if phase != "json" else max(1, int(n * p + 0.5))
        if scene.get("cmp"):
            gx = W // 2
            d.line([gx, top - 10, gx, top + n * lh + 14], fill=WIRE, width=2)
            for side, x0 in ((scene["left"], 64), (scene["right"], gx + 40)):
                for i, line in enumerate(side[:vis]):
                    y = top + i * lh
                    col = INK if i == 0 else SEC
                    if i in lit and phase in ("light", "hold", "morph"):
                        col = AMBER
                    elif lit and phase in ("light", "hold", "morph"):
                        col = WIRE
                    if phase == "morph":
                        gone = p * n * 1.4
                        if n - i <= gone - 1:
                            continue
                        if n - i <= gone:
                            col = WIRE
                    d.text((x0, y), line, font=f_mono, fill=col)
            for i in lit:
                if phase in ("light", "hold", "morph"):
                    d.line([46, top + i * lh - 2, 46, top + i * lh + 20], fill=AMBER, width=3)
            if phase in ("light", "hold"):
                d.text((64, top - 18 + n * lh + 44 + 20), "\u2191 " + scene["callout"], font=f_small, fill=AMBER)
        for i, line in enumerate(([] if scene.get("cmp") else scene["js"])[:vis]):
            y = top + i * lh
            dim = WIRE if (phase in ("light", "hold", "morph") and lit and i not in lit) else None
            if phase == "morph":
                gone = p * n * 1.4
                if n - i <= gone - 1:
                    continue
                if n - i <= gone:
                    dim = WIRE
            if phase in ("light", "hold", "morph") and i in lit:
                d.line([46, y - 2, 46, y + 20], fill=AMBER, width=3)
            if scene.get("err"):
                base = AMBER if i in lit else (RED if i < 2 else SEC)
                d.text((64, y), line, font=f_mono, fill=dim or base)
            else:
                draw_spans(d, 64, y, tint(line), force=dim)
        if phase in ("light", "hold") and lit:
            d.text((64, top - 18 + n * lh + 44 + 20), "↑ " + scene["callout"], font=f_small, fill=AMBER)

    if phase in ("morph", "human"):
        hp = 1.0 if phase == "human" else max(0.0, (p - 0.4) / 0.6)
        lines = scene["human"]
        vis = len(lines) if phase == "human" else int(len(lines) * hp + 0.5)
        y0 = 182 if phase == "human" else 182 + int(90 * (1 - hp))
        if phase == "human":
            panel(d, 32, y0 - 22, W - 64, len(lines) * 32 + 46)
            d.text((64, y0 - 52), "what the agent now knows", font=f_small, fill=AMBER)
        for i, (text, col) in enumerate(lines[:vis]):
            d.text((64, y0 + i * 32), text, font=f_body, fill=col)

    if phase == "cta":
        d.text((64, 190), "install", font=f_small, fill=SEC)
        d.text((64, 222), INSTALL, font=f_body, fill=INK)
        d.line([64, 268, 64 + f_body.getlength(INSTALL), 268], fill=AMBER, width=3)
        d.text((64, 302), "one Rust binary · local · MIT · nothing leaves your machine",
               font=f_body, fill=SEC)

    d.text((40, H - 34), PROV, font=f_small, fill=WIRE)
    img.save(path)


BEATS = [("call", 1.0), ("json", 1.6), ("light", 0.8), ("hold", 3.6),
         ("morph", 1.5), ("human", 6.5), ("cta", 2.6)]


def build(key, outdir):
    scene = SCENES[key]
    os.makedirs(outdir, exist_ok=True)
    for f in os.listdir(outdir):
        os.remove(os.path.join(outdir, f))
    k = 0
    for phase, secs in BEATS:
        nf = max(1, int(secs * FPS))
        for i in range(nf):
            frame(scene, phase, (i + 1) / nf, f"{outdir}/f{k:05d}.png")
            k += 1
    return k


def encode(outdir, stem):
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", str(FPS),
                    "-i", f"{outdir}/f%05d.png", "-c:v", "libx264", "-pix_fmt", "yuv420p",
                    "-crf", "20", f"{stem}.mp4"], check=True)
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", f"{stem}.mp4", "-vf",
                    f"fps={FPS},scale={W}:-1:flags=lanczos,palettegen=max_colors=32:stats_mode=diff",
                    "/tmp/pal.png"], check=True)
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", f"{stem}.mp4", "-i", "/tmp/pal.png",
                    "-filter_complex",
                    f"fps={FPS},scale={W}:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=none:diff_mode=rectangle",
                    f"{stem}.gif"], check=True)
    return os.path.getsize(f"{stem}.gif")


if __name__ == "__main__":
    keys = sys.argv[1:] or ORDER
    os.makedirs("demos", exist_ok=True)
    total = 0
    for key in keys:
        n = build(key, "frames_demo")
        size = encode("frames_demo", f"demos/{key}")
        total += size
        print(f"{key:10} {n:4} frames  {n / FPS:5.1f}s  {size / 1024:6.0f} KB")
    print(f"total {total / 1024 / 1024:.1f} MB")
