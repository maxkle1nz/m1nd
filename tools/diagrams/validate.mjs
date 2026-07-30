// Gate mecânico dos UMLs visuais do m1nd. EXIT 0 = pass.
import { readFileSync, existsSync, statSync } from "fs";
import { execSync } from "child_process";

const SCENES = ["transplant-two-phase","brain-per-repo","verdict-gate","grounded-memory",
                "mailbox","presence-collision","ambient-hooks","l1ght-lane"];
const THEMES = ["dark","light"];
// Paleta medida do brief gen-008 (dark + light) — nenhum hex fora disto.
const ALLOW = new Set(["#161B22","#F2F4EF","#98A2AE","#E8A33D","#FF7B72","#687585",
                       "#ECEDE8","#14181C","#5A6169","#7E4E00","#A81E22","#84867F"].map(s=>s.toUpperCase()));
const OUT = process.argv[2] ?? "docs/assets/diagrams";
let fails = [];
for (const s of SCENES) for (const t of THEMES) {
  const p = `${OUT}/${s}-${t}.svg`;
  if (!existsSync(p)) { fails.push(`${p}: MISSING`); continue; }
  const kb = statSync(p).size/1024;
  if (kb > 60) fails.push(`${p}: ${kb.toFixed(1)}KB > 60KB`);
  const src = readFileSync(p, "utf8");
  try { execSync(`xmllint --noout ${p}`, {stdio:"pipe"}); } catch { fails.push(`${p}: not well-formed XML`); }
  if (!/viewBox=/.test(src)) fails.push(`${p}: no viewBox`);
  if (!/<title[\s>]/.test(src)) fails.push(`${p}: no <title>`);
  if (!/<desc[\s>]/.test(src)) fails.push(`${p}: no <desc>`);
  if (/linearGradient|radialGradient|<filter|<animate|feGaussian/i.test(src)) fails.push(`${p}: forbidden effect`);
  for (const m of src.toUpperCase().matchAll(/#[0-9A-F]{6}\b/g))
    if (!ALLOW.has(m[0])) fails.push(`${p}: hex fora da paleta ${m[0]}`);
  const op = [...src.matchAll(/opacity="([^"]+)"/g)].filter(m => m[1] !== "1" && m[1] !== "0");
  if (op.length) fails.push(`${p}: decorative opacity`);

  // Tipografia (regra do diretor, 2026-07-30): letras nunca encostam em moldura,
  // nunca estouram o canvas, nunca se sobrepõem. Larguras estimadas por classe de
  // fonte (mono 0.62em/char, sans 0.55em/char) — aproximação deliberadamente folgada.
  const vb = /viewBox="0 0 (\d+) (\d+)"/.exec(src);
  const W = vb ? +vb[1] : 920;
  const texts = [];
  for (const m of src.matchAll(/<text\b([^>]*)>([^<]*)<\/text>/g)) {
    const a = m[1], content = m[2].trim();
    if (!content) continue;
    const gx = /\bx="(-?[\d.]+)"/.exec(a), gy = /\by="(-?[\d.]+)"/.exec(a);
    if (!gx || !gy) continue;
    const size = +(/font-size="([\d.]+)"/.exec(a)?.[1] ?? 16);
    const mono = /monospace/.test(a);
    const anchor = /text-anchor="(\w+)"/.exec(a)?.[1] ?? "start";
    const w = content.length * size * (mono ? 0.62 : 0.55);
    const x = +gx[1], y = +gy[1];
    const x0 = anchor === "middle" ? x - w / 2 : anchor === "end" ? x - w : x;
    texts.push({ content, x0, x1: x0 + w, yTop: y - size, yBot: y + size * 0.25, size });
  }
  for (const t of texts) {
    if (t.x0 < 8 || t.x1 > W - 8)
      fails.push(`${p}: texto estoura o canvas: "${t.content}" [${t.x0.toFixed(0)}..${t.x1.toFixed(0)}] vs [8..${W - 8}]`);
  }
  const rects = [...src.matchAll(/<rect\b[^>]*\bfill="none"[^>]*>/g)].map(r => {
    const g = k => +(new RegExp(`\\b${k}="(-?[\\d.]+)"`).exec(r[0])?.[1] ?? NaN);
    return { x: g("x"), y: g("y"), w: g("width"), h: g("height") };
  }).filter(r => !Number.isNaN(r.x) && r.w >= 48 && r.h >= 32);
  const PAD = 8;
  for (const t of texts) for (const r of rects) {
    const cy = (t.yTop + t.yBot) / 2;
    if (cy < r.y || cy > r.y + r.h) continue;
    const overlap = Math.min(t.x1, r.x + r.w) - Math.max(t.x0, r.x);
    if (overlap <= 0) continue;
    const inside = t.x0 >= r.x && t.x1 <= r.x + r.w;
    if (!inside)
      fails.push(`${p}: texto atravessa moldura: "${t.content}" [${t.x0.toFixed(0)}..${t.x1.toFixed(0)}] vs caixa [${r.x}..${r.x + r.w}]`);
    else if (t.x0 - r.x < PAD || r.x + r.w - t.x1 < PAD)
      fails.push(`${p}: padding < ${PAD}px na caixa: "${t.content}" (folgas ${(t.x0 - r.x).toFixed(0)}/${(r.x + r.w - t.x1).toFixed(0)})`);
  }
  for (let i = 0; i < texts.length; i++) for (let j = i + 1; j < texts.length; j++) {
    const a = texts[i], b = texts[j];
    const vOver = Math.min(a.yBot, b.yBot) - Math.max(a.yTop, b.yTop);
    const hOver = Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0);
    if (vOver > Math.min(a.size, b.size) * 0.5 && hOver > 4)
      fails.push(`${p}: colisão de textos: "${a.content}" × "${b.content}" (sobrepõem ${hOver.toFixed(0)}px)`);
  }
}
if (fails.length) { console.error("GATE FAIL:\n" + fails.join("\n")); process.exit(1); }
console.log(`GATE PASS: ${SCENES.length*2} SVGs válidos, paleta travada, sem efeitos.`);
