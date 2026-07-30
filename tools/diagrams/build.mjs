// Deterministic layout for the m1nd system diagrams.
//
// Every box is sized FROM its text (monospace advance is exact), every label
// sits in a gutter measured to hold it, and both themes come out of one
// geometry pass. Overlap is not a defect patched here; it cannot be expressed.
import { writeFileSync, mkdirSync } from "fs";

const W = 920;
const ADV = 0.6;            // advance per character, SF Mono / Menlo at 1em
const FONT = "ui-monospace, SFMono-Regular, Menlo, monospace";
const tw = (s, size = 16) => [...s].length * size * ADV;
const snap = v => Math.round(v / 8) * 8;
const esc = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

const THEMES = {
  dark:  { field: "#161B22", ink: "#F2F4EF", sec: "#98A2AE", amber: "#E8A33D", red: "#FF7B72", wire: "#687585" },
  light: { field: "#ECEDE8", ink: "#14181C", sec: "#5A6169", amber: "#7E4E00", red: "#A81E22", wire: "#84867F" },
};

const PADX = 24, PADY = 18, LH = 26;

function boxSize(lines, minW = 0) {
  const wid = Math.max(minW, ...lines.map(l => tw(l.t, l.size ?? 16)));
  return { w: snap(wid + PADX * 2), h: snap(lines.length * LH + PADY * 2) };
}

function E(c) {
  const o = [], bg = [];
  const api = {
    out: o,
    bg,
    text(x, y, s, { size = 16, fill = c.ink, anchor = "start" } = {}) {
      o.push(`<text x="${x}" y="${y}" font-family="${FONT}" font-size="${size}" fill="${fill}" text-anchor="${anchor}">${esc(s)}</text>`);
    },
    // The frame is a real rect; a bite is a piece of the field cutting through
    // the stroke, so the opening is honest and the box keeps measurable bounds.
    frame(x, y, w, h, stroke, bite) {
      o.push(`<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="none" stroke="${stroke}" stroke-width="4"/>`);
      if (!bite) return;
      const { side, at = 0.5, len = 40 } = bite;
      if (side === "left" || side === "right") {
        const bx = side === "left" ? x : x + w, by = y + h * at;
        o.push(`<line x1="${bx}" y1="${by - len / 2}" x2="${bx}" y2="${by + len / 2}" stroke="${c.field}" stroke-width="10"/>`);
      } else {
        const by = side === "top" ? y : y + h, bx = x + w * at;
        o.push(`<line x1="${bx - len / 2}" y1="${by}" x2="${bx + len / 2}" y2="${by}" stroke="${c.field}" stroke-width="10"/>`);
      }
    },
    box(x, y, lines, { stroke = c.wire, bite = null, minW = 0, align = "center" } = {}) {
      const { w, h } = boxSize(lines, minW);
      o.push(`<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${c.field}"/>`);
      api.frame(x, y, w, h, stroke, bite);
      lines.forEach((l, i) => {
        const size = l.size ?? 16, by = y + PADY + 17 + i * LH;
        if (align === "center") api.text(x + w / 2, by, l.t, { size, fill: l.fill ?? c.ink, anchor: "middle" });
        else api.text(x + PADX, by, l.t, { size, fill: l.fill ?? c.ink });
      });
      return { x, y, w, h, cx: x + w / 2, cy: y + h / 2, r: x + w, b: y + h };
    },
    boxAt(cx, y, lines, opt = {}) {
      const { w } = boxSize(lines, opt.minW ?? 0);
      return api.box(snap(cx - w / 2), y, lines, opt);
    },
    line(x1, y1, x2, y2, { stroke = c.wire, dashed = false, width = 4, back = false } = {}) {
      const s = `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${stroke}" stroke-width="${width}"${dashed ? ' stroke-dasharray="10 8"' : ""}/>`;
      (back ? bg : o).push(s);
    },
    tri(x, y, dir, stroke = c.wire) {
      const s = 7, l = 14;
      const p = { r: `${x},${y} ${x - l},${y - s} ${x - l},${y + s}`,
                  l: `${x},${y} ${x + l},${y - s} ${x + l},${y + s}`,
                  d: `${x},${y} ${x - s},${y - l} ${x + s},${y - l}`,
                  u: `${x},${y} ${x - s},${y + l} ${x + s},${y + l}` }[dir];
      o.push(`<polygon points="${p}" fill="${stroke}"/>`);
    },
    arrowH(x1, x2, y, opt = {}) {
      const dir = x2 > x1 ? "r" : "l";
      api.line(x1, y, dir === "r" ? x2 - 12 : x2 + 12, y, opt);
      api.tri(x2, y, dir, opt.stroke ?? c.wire);
    },
    arrowV(x, y1, y2, opt = {}) {
      const dir = y2 > y1 ? "d" : "u";
      api.line(x, y1, x, dir === "d" ? y2 - 12 : y2 + 12, opt);
      api.tri(x, y2, dir, opt.stroke ?? c.wire);
    },
    elbowHVH(x1, y1, xm, x2, y2, opt = {}) {
      api.line(x1, y1, xm, y1, opt);
      api.line(xm, y1, xm, y2, opt);
      api.arrowH(xm, x2, y2, opt);
    },
    circle(cx, cy, r, stroke = c.wire) {
      o.push(`<circle cx="${cx}" cy="${cy}" r="${r}" fill="${c.field}" stroke="${stroke}" stroke-width="4"/>`);
    },
    square(cx, cy, s, stroke = c.wire) {
      o.push(`<rect x="${cx - s / 2}" y="${cy - s / 2}" width="${s}" height="${s}" fill="${c.field}" stroke="${stroke}" stroke-width="4"/>`);
    },
    bar(x, y, w, h, fill) {
      o.push(`<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}"/>`);
    },
  };
  return api;
}

function wrap(s, max) {
  const out = [];
  let cur = "";
  for (const wd of s.split(" ")) {
    if ((cur + " " + wd).trim().length > max) { out.push(cur.trim()); cur = wd; }
    else cur += " " + wd;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

function head(e, c, title, subtitle) {
  e.text(40, 46, title, { size: 22 });
  const sub = wrap(subtitle, 82);
  sub.forEach((l, i) => e.text(40, 78 + i * 24, l, { size: 16, fill: c.sec }));
  const ruleY = 78 + sub.length * 24 + 4;
  e.line(40, ruleY, W - 40, ruleY, { width: 2 });
  return ruleY + 48;
}

function foot(e, c, y, lines) {
  const flat = lines.flatMap(l => wrap(l, 82));
  flat.forEach((l, i) => e.text(W / 2, y + i * 26, l, { size: 16, fill: c.sec, anchor: "middle" }));
  return y + flat.length * 26 + 20;
}

/* ------------------------------------------------------------------ scenes */

const scenes = {};

scenes["transplant-two-phase"] = c => {
  const e = E(c);
  const y = head(e, c, "transplant_two_phase",
    "preview plans the move, commit re-checks every file hash before writing and returns an honest receipt");
  const lane = [150, 460, 780];
  const heads = ["agent", "m1nd server", "filesystem"].map((t, i) =>
    e.boxAt(lane[i], y, [{ t, size: 18 }], { minW: 160 }));
  const top = heads[0].b;
  let row = top + 60;

  const step = (a, b, label, opt = {}) => {
    e.text((lane[a] + lane[b]) / 2, row - 16, label, { size: 16, fill: opt.stroke ?? c.ink, anchor: "middle" });
    e.arrowH(lane[a], lane[b], row, opt);
    row += 64;
  };
  const note = (i, lines) => {
    const b = e.boxAt(lane[i], row - 24, lines, { bite: { side: "left", at: 0.5 } });
    row = b.b + 56;
  };

  step(0, 1, "1  transplant_preview()");
  note(1, [{ t: "plans the enlarged region," }, { t: "its deps and its referrers" }]);
  step(1, 0, "2  plan", { dashed: true });
  step(0, 1, "3  transplant_commit()");
  note(1, [{ t: "re-validates every file hash" }]);
  step(1, 2, "4  atomic write");
  step(2, 1, "ok", { dashed: true });
  step(1, 0, "5  receipt", { dashed: true });

  const sep = row - 32;
  e.line(40, sep, W - 40, sep, { width: 2, dashed: true });
  row = sep + 64;
  step(0, 1, "6  commit on a taken name", { stroke: c.red });
  step(1, 0, "REFUSE", { stroke: c.red, dashed: true });

  const bottom = row - 40;
  lane.forEach(x => e.line(x, top, x, bottom, { width: 2, dashed: true, back: true }));
  const rb = e.boxAt(W / 2, bottom + 32, [
    { t: "the occupant is named · state: blocked · nothing was written", fill: c.red },
  ], { stroke: c.red, bite: { side: "left", at: 0.5 } });

  const end = foot(e, c, rb.b + 48, [
    "solid = call · dashed = return · red = typed refusal",
    "the receipt names what did not travel: refs_unresolved and state_left_behind",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "transplant, the two-phase move",
    desc: "A sequence across three lanes: agent, m1nd server and filesystem. The agent calls transplant_preview and the server plans the enlarged region with its dependencies and referrers, returning a plan. The agent then calls transplant_commit; the server re-validates every planned file hash, writes atomically, and returns a receipt naming refs_unresolved and state_left_behind. A final branch in red shows a commit onto a taken name being refused, with the occupant named, the state blocked and nothing written.",
  };
};

scenes["brain-per-repo"] = c => {
  const e = E(c);
  const y = head(e, c, "brain_per_repo",
    "one served owner, one brain per repository root, and thin bridges that carry no graph of their own");

  const mk = n => [{ t: `brain ${n}`, size: 18 }, { t: `~/proj${n}`, fill: c.sec }, { t: "graph · memory", fill: c.sec }];
  const bw = boxSize(mk("A")).w;
  const brains = ["A", "B", "C"].map((n, i) =>
    e.box(392, y + i * 144, mk(n), { bite: { side: "left", at: 0.5 } }));

  const owner = e.box(40, brains[1].cy - 44, [{ t: "owner", size: 18 }, { t: "served :1337", fill: c.sec }], { minW: 168 });
  const trunk = owner.r + 64;
  e.text(trunk - 8, owner.y - 16, "serves", { size: 16, fill: c.sec, anchor: "end" });
  brains.forEach(b => e.elbowHVH(owner.r, owner.cy, trunk, b.x, b.cy));

  brains.forEach(b => {
    const br = e.box(W - 40 - 152, b.y + 16, [{ t: "agent", size: 16 }, { t: "bridge", fill: c.sec }], { minW: 104 });
    e.arrowH(br.x, b.r, br.cy);
  });

  const lastB = brains[2].b;
  const bad = e.box(392, lastB + 104, [
    { t: "agent", size: 18, fill: c.red }, { t: "repo not hosted", fill: c.red },
  ], { stroke: c.red, minW: bw - PADX * 2, bite: { side: "top", at: 0.5 } });
  e.text(owner.cx + 32, lastB + 70, "reception → typed refusal", { size: 16, fill: c.red });
  e.line(owner.cx, owner.b, owner.cx, lastB + 88, { stroke: c.red, dashed: true });
  e.line(owner.cx, lastB + 88, bad.cx, lastB + 88, { stroke: c.red, dashed: true });
  e.arrowV(bad.cx, lastB + 88, bad.y, { stroke: c.red, dashed: true });
  e.text(bad.cx, bad.b + 32, "no wrong answer · state: blocked", { size: 16, fill: c.red, anchor: "middle" });

  const end = foot(e, c, bad.b + 80, [
    "a bridge holds no graph and no lease: it consumes the brain that covers its root",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "one brain per repository",
    desc: "A topology. A single served owner on port 1337 serves three brains, one per repository root, each with its own graph and memory. Agents attach on the right as thin bridges that hold no graph and take no lease. Below, an agent whose repository is not hosted receives a typed refusal from reception, drawn in red: the system blocks instead of answering wrongly.",
  };
};

scenes["verdict-gate"] = c => {
  const e = E(c);
  const y = head(e, c, "verdict_gate",
    "the conformal gate as an honest funnel: one cut at alpha 0.10 separates act from everything else");

  const inb = e.box(40, y + 152, [{ t: "n ≈ 9.2k", size: 20 }, { t: "predictions in", fill: c.sec }], { minW: 176 });
  const gate = 392, barX = gate + 48, BAR = 380;
  e.arrowH(inb.r, gate, inb.cy);
  e.text(gate + 8, y + 8, "one cut · α = 0.10", { size: 16, fill: c.sec });

  const rows = [
    { k: "act",      pct: 0.135, n: "≈ 1.24k", fill: c.ink,   note: "the agent may act on it" },
    { k: "reverify", pct: 0.26,  n: "≈ 2.4k",  fill: c.amber, note: "a human re-confirms before acting" },
    { k: "abstain",  pct: 0.60,  n: "≈ 5.5k",  fill: c.sec,   note: "the honest exit of a weak signal" },
  ];
  let ry = y + 44;
  const first = ry + 12;
  let lastBar = 0;
  rows.forEach(r => {
    e.text(barX, ry, `${r.k} · ${Math.round(r.pct * 1000) / 10}% · ${r.n}`, { size: 16, fill: r.fill });
    e.bar(barX, ry + 14, snap(BAR * r.pct), 32, r.fill);
    e.text(barX, ry + 84, r.note, { size: 16, fill: c.sec });
    lastBar = ry + 46;
    ry += 128;
  });
  e.line(gate, first - 24, gate, lastBar + 8, { width: 4 });

  const end = foot(e, c, ry + 8, [
    "bar length is the measured share, not decoration",
    "abstain is the largest slice: the system says it does not know instead of guessing",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "the conformal verdict gate",
    desc: "A funnel. About 9.2 thousand predictions enter on the left and meet a single vertical cut, the conformal gate at alpha 0.10. Three bars on the right carry the outcome, their lengths proportional to the measured shares: act at 13.5 percent which the agent may act on, reverify at 26 percent where a human re-confirms first, and abstain at 60 percent, the largest slice and the honest exit of a weak signal.",
  };
};

scenes["grounded-memory"] = c => {
  const e = E(c);
  let y = head(e, c, "grounded_memory",
    "a claim is anchored to the code it cites; when that code changes, the claim marks itself stale");

  const CX = 296, LX = 408;
  const link = (label, { jog = false } = {}) => {
    const from = y, to = y + (jog ? 128 : 88);
    if (jog) {
      const m = from + 40;
      e.line(CX, from, CX, m, { stroke: c.amber });
      e.line(CX, m, CX + 32, m + 32, { stroke: c.amber });
      e.line(CX + 32, m + 32, CX + 32, to - 12, { stroke: c.amber });
      e.tri(CX + 32, to, "d", c.amber);
      e.text(CX + 56, m + 14, "stale", { size: 16, fill: c.amber });
    } else e.arrowV(CX, from, to);
    if (label) e.text(LX, from + (to - from) / 2 + 6, label, { size: 16, fill: jog ? c.amber : c.sec });
    y = to;
  };

  let b = e.boxAt(CX, y, [{ t: "memorize()", size: 18 }, { t: "claims + evidence", fill: c.sec }],
    { bite: { side: "top", at: 0.5 } });
  y = b.b;
  link("grounded_in → the cited code node");
  b = e.boxAt(CX, y, [{ t: "code node", size: 18 }, { t: "hash H1", fill: c.sec }]); y = b.b;
  link("the code changes: H1 ≠ H2");
  b = e.boxAt(CX, y, [{ t: "cross_verify()", size: 18 }, { t: "compares the hashes", fill: c.sec }]); y = b.b;
  link("the anchor no longer holds", { jog: true });
  b = e.boxAt(CX + 32, y, [
    { t: "claim: stale", size: 18, fill: c.amber }, { t: "warns instead of asserting", fill: c.amber },
  ], { stroke: c.amber });

  const end = foot(e, c, b.b + 64, [
    "45 degrees is the house sign for drift: the wire holds, its ground does not",
    "it warns instead of answering from a claim it can no longer stand on",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "grounded memory going stale",
    desc: "A vertical state machine. A memorize call stores claims with their evidence, and an edge named grounded_in ties them to the code node they cite at hash H1. When that code changes to hash H2, cross_verify compares the hashes and the wire breaks with a 45 degree step labelled stale. The final state, framed in amber, is a claim marked stale that warns instead of asserting.",
  };
};

scenes["mailbox"] = c => {
  const e = E(c);
  const y = head(e, c, "mailbox",
    "an agent leaves a letter on disk; another agent, days later, starts its mission already knowing");

  const A = e.box(40, y, [{ t: "agent A", size: 18 }, { t: "mission X", fill: c.sec }], { minW: 152 });
  const file = e.boxAt(W / 2, y, [{ t: ".m1nd/inbox.jsonl", size: 18 }, { t: "append-only", fill: c.sec }],
    { bite: { side: "left", at: 0.5 } });
  const B = e.box(W - 40 - A.w, y, [{ t: "agent B", size: 18 }, { t: "days later", fill: c.sec }], { minW: 152 });

  e.text((A.r + file.x) / 2, A.cy - 22, "writes", { size: 16, fill: c.sec, anchor: "middle" });
  e.arrowH(A.r, file.x, A.cy);
  e.text((file.r + B.x) / 2, A.cy - 22, "sweeps", { size: 16, fill: c.sec, anchor: "middle" });
  e.arrowH(B.x, file.r, A.cy, { dashed: true });

  const n = A.b + 44;
  e.text(A.cx, n, "finds a defect", { size: 16, fill: c.sec, anchor: "middle" });
  e.text(A.cx, n + 24, "outside its scope", { size: 16, fill: c.sec, anchor: "middle" });
  e.text(file.cx, n, "on disk, beside the code", { size: 16, fill: c.sec, anchor: "middle" });
  e.text(B.cx, n, "CLI or REST,", { size: 16, fill: c.sec, anchor: "middle" });
  e.text(B.cx, n + 24, "never in the query loop", { size: 16, fill: c.sec, anchor: "middle" });

  const start = e.boxAt(W / 2, n + 80, [{ t: "the mission starts", size: 18 }, { t: "already knowing", fill: c.sec }],
    { bite: { side: "top", at: 0.5 } });
  e.line(B.cx, B.b + 96, B.cx, start.cy, { dashed: true });
  e.arrowH(B.cx, start.r, start.cy, { dashed: true });

  const end = foot(e, c, start.b + 64, [
    "a letter on disk outlives the session, the context window and its author",
    "recorded where the code lives, for the owner of that system to fix in due time",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "the per-brain mailbox",
    desc: "A flow between agents. Agent A, on mission X, finds a defect outside its own scope and writes a letter into .m1nd/inbox.jsonl, an append-only file on disk beside the code. Agent B, days later, sweeps that mailbox over CLI or REST, never inside the query loop, and its mission starts already knowing about the defect.",
  };
};

scenes["presence-collision"] = c => {
  const e = E(c);
  const y = head(e, c, "presence_collision",
    "two sessions on the same brain register presence; the warning reaches both before either one writes");

  const brain = e.boxAt(W / 2, y, [{ t: "brain", size: 18 }, { t: "presences[ ] · TTL", fill: c.sec }], { minW: 208 });
  const A = e.box(40, y, [{ t: "agent A", size: 18 }, { t: "same brain", fill: c.sec }], { minW: 152 });
  const B = e.box(W - 40 - A.w, y, [{ t: "agent B", size: 18 }, { t: "same brain", fill: c.sec }], { minW: 152 });

  e.text((A.r + brain.x) / 2, A.cy - 22, "presence", { size: 16, fill: c.sec, anchor: "middle" });
  e.arrowH(A.r, brain.x, A.cy);
  e.text((brain.r + B.x) / 2, A.cy - 22, "presence", { size: 16, fill: c.sec, anchor: "middle" });
  e.arrowH(B.x, brain.r, A.cy);

  e.text(brain.cx + 24, brain.b + 44, "warns both, before either writes", { size: 16, fill: c.amber });
  const wY = brain.b + 112, warn = [{ t: "orientation pkg", fill: c.amber }, { t: "collision", fill: c.amber }];
  const wA = e.box(40, wY, warn, { stroke: c.amber, minW: 152, bite: { side: "top", at: 0.5 } });
  const wB = e.box(W - 40 - wA.w, wY, warn, { stroke: c.amber, minW: 152, bite: { side: "top", at: 0.5 } });
  const trunk = brain.b + 72;
  e.line(brain.cx, brain.b, brain.cx, trunk, { stroke: c.amber, dashed: true });
  e.line(wA.cx, trunk, wB.cx, trunk, { stroke: c.amber, dashed: true });
  e.arrowV(wA.cx, trunk, wA.y, { stroke: c.amber, dashed: true });
  e.arrowV(wB.cx, trunk, wB.y, { stroke: c.amber, dashed: true });

  const end = foot(e, c, wA.b + 64, [
    "the system warns; the human decides",
    "each session is a presence with a TTL; the notice rides in the orientation package",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "presence and collision",
    desc: "Two agents work on the same brain. Each session registers a presence with a time to live, drawn as arrows into the central brain box. When the work overlaps, a dashed amber path carries the collision notice down into the orientation package of both agents, before either one lands a change. The system warns; the human decides.",
  };
};

scenes["ambient-hooks"] = c => {
  const e = E(c);
  const y = head(e, c, "ambient_hooks",
    "the host fires at spawn and the north package is injected, so the agent is oriented before the first prompt");

  const hook = e.boxAt(200, y, [{ t: "ambient hook", size: 18 }, { t: "injects north", fill: c.sec }], { minW: 200 });
  const pkg = e.box(W - 40 - 264, y, [
    { t: "north package", size: 18 }, { t: "map · memory", fill: c.sec }, { t: "trust · gaps", fill: c.sec },
  ], { minW: 216, bite: { side: "left", at: 0.5 } });
  e.text((hook.r + pkg.x) / 2, hook.cy - 22, "injects", { size: 16, fill: c.sec, anchor: "middle" });
  e.arrowH(hook.r, pkg.x, hook.cy);

  const born = e.box(pkg.x, pkg.b + 96, [{ t: "agent + subagent", size: 18 }, { t: "born oriented", fill: c.sec }],
    { minW: 216, bite: { side: "top", at: 0.5 } });
  e.arrowV(pkg.cx, pkg.b, born.y, { dashed: true });

  const lineY = Math.max(born.b, hook.b) + 112;
  const bus = lineY - 56;
  e.line(40, lineY, W - 40, lineY, { width: 4 });
  const ticks = [{ x: 168, t: "SessionStart" }, { x: 360, t: "agentSpawn" }, { x: 552, t: "TaskStart" }];
  ticks.forEach(t => {
    e.line(t.x, lineY - 14, t.x, lineY + 14, { width: 4 });
    e.text(t.x, lineY + 44, t.t, { size: 16, anchor: "middle" });
    e.line(t.x, bus, t.x, lineY - 18, { dashed: true, width: 2 });
  });
  e.line(ticks[0].x, bus, ticks[2].x, bus, { dashed: true, width: 2 });
  e.arrowV(hook.cx, bus, hook.b, { dashed: true, width: 2 });
  e.line(792, lineY - 14, 792, lineY + 14, { stroke: c.sec, width: 4 });
  e.text(792, lineY + 44, "first prompt", { size: 16, fill: c.sec, anchor: "middle" });
  e.text(792, lineY + 68, "(only now)", { size: 16, fill: c.sec, anchor: "middle" });

  const end = foot(e, c, lineY + 116, [
    "context arrives at spawn, not after the first prompt",
    "map, memory, trust and gaps are already there when the session opens",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "ambient hooks at spawn",
    desc: "A session opening. The host fires three events on the timeline at the bottom, SessionStart, agentSpawn and TaskStart, each feeding the ambient hook above. The hook injects the north package, carrying the map, the memory, the trust and the gaps, into the agent and its subagents, which are therefore born oriented. The first user prompt arrives only later, marked further right on the same timeline.",
  };
};

scenes["l1ght-lane"] = c => {
  const e = E(c);
  const y = head(e, c, "l1ght_lane",
    "one graph, two lanes: documents as circles, code as squares, and a single ranked answer across both");

  const cols = [
    { cx: 280, doc: "paper · DOI", route: "DOI/Crossref", code: "mod A" },
    { cx: 460, doc: "RFC 9110",    route: "RFC",          code: "mod B" },
    { cx: 640, doc: "design note", route: "JATS",         code: "mod C" },
  ];
  const docY = y + 56, wireY = docY + 136, codeY = wireY + 96;

  e.text(40, docY - 56, "documents", { size: 16, fill: c.sec });
  e.text(40, codeY + 64, "code", { size: 16, fill: c.sec });
  cols.forEach(col => {
    e.text(col.cx, docY - 56, col.doc, { size: 16, anchor: "middle" });
    e.line(col.cx, docY + 28, col.cx, wireY, { width: 2 });
    e.line(col.cx, wireY, col.cx, codeY - 28, { width: 2 });
    e.circle(col.cx, docY, 28);
    e.square(col.cx, codeY, 56);
    e.text(col.cx + 16, docY + 84, col.route, { size: 16, fill: c.sec });
    e.text(col.cx, codeY + 64, col.code, { size: 16, anchor: "middle" });
  });
  e.line(cols[0].cx + 28, docY, cols[1].cx - 28, docY, { dashed: true, width: 2 });
  e.text((cols[0].cx + cols[1].cx) / 2, docY - 18, "cites", { size: 16, fill: c.sec, anchor: "middle" });

  const seek = e.box(40, wireY - 44, [{ t: "seek()", size: 18 }, { t: "one query", fill: c.sec }], { minW: 112 });
  const ans = e.box(W - 40 - 160, wireY - 44, [{ t: "1 answer", size: 18 }, { t: "ranked", fill: c.sec }],
    { minW: 112, bite: { side: "left", at: 0.5 } });
  e.arrowH(seek.r, ans.x, wireY);

  const end = foot(e, c, codeY + 136, [
    "typed edges join the lanes: a paper cites an RFC, a module implements a spec",
    "seek crosses both and returns one ranked answer, not two piles to reconcile",
  ]);
  return {
    h: snap(end + 8), body: [...e.bg, ...e.out].join("\n"),
    title: "the l1ght document lane",
    desc: "One graph with two lanes. The upper lane holds document nodes drawn as circles, a paper with a DOI, an RFC and a design note, joined by a dashed cites edge. The lower lane holds code nodes drawn as squares. Vertical connectors labelled with the real ingest routes, DOI slash Crossref, RFC and JATS, tie each document to its code. A seek query on the left crosses the horizontal wire through both lanes and returns a single ranked answer on the right.",
  };
};

/* ------------------------------------------------------------------- build */

const OUT = process.argv[2] ?? "docs/assets/diagrams";
mkdirSync(OUT, { recursive: true });
let n = 0;
for (const [name, fn] of Object.entries(scenes)) {
  for (const [theme, c] of Object.entries(THEMES)) {
    const s = fn(c);
    writeFileSync(`${OUT}/${name}-${theme}.svg`,
`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${s.h}" width="${W}" height="${s.h}" role="img">
<title>${esc(s.title)}</title>
<desc>${esc(s.desc)}</desc>
<rect x="0" y="0" width="${W}" height="${s.h}" fill="${c.field}"/>
${s.body}
</svg>
`);
    n++;
  }
}
console.log(`emitted ${n} SVGs`);
