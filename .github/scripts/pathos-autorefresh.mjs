#!/usr/bin/env node

// pathos-autorefresh.mjs — atualiza as seções AUTO-GERADAS do PATHOS.md de um repo.
//
// Genérico e portátil (zero-dep, só Node builtin + os binários `git` e `git-cliff`):
//   (a) <!-- BEGIN:auto-changelog --> ... <!-- END:auto-changelog -->
//       changelog markdown via git-cliff (Conventional Commits). git-cliff é OPCIONAL:
//       se ausente, esta seção é pulada com um aviso (nunca apaga o que já existe).
//   (b) <!-- BEGIN:auto-overview --> ... <!-- END:auto-overview -->  (OPCIONAL)
//       resumo simples: nome do repo, branch, data do último commit e contagem de
//       commits desde a última tag (ou desde a raiz, se não houver tag).
//
// Regras de ouro:
//   - Só toca o conteúdo ENTRE as âncoras. Nunca escreve fora delas.
//   - Se uma âncora não existir, PULA a seção (não inventa, não cria âncora).
//   - Se nada mudou, NÃO reescreve o arquivo (idempotente; evita commits vazios).
//   - Nunca apaga conteúdo curado à mão.
//
// Uso: node pathos-autorefresh.mjs            (auto-localiza o PATHOS)
//      node pathos-autorefresh.mjs <caminho>  (aponta um PATHOS específico)

import { execFileSync } from "node:child_process";
import { existsSync, promises as fs } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

// ---------- localização do repo e do PATHOS ----------

function repoRoot() {
  try {
    return execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim();
  } catch {
    return process.cwd();
  }
}

// Ordem de busca pedida: docs/internal/PATHOS.md -> docs/PATHOS.md -> PATHOS.md (raiz).
function findPathos(root, override) {
  if (override) {
    const p = path.resolve(override);
    return existsSync(p) ? p : null;
  }
  const candidates = [
    path.join(root, "docs", "internal", "PATHOS.md"),
    path.join(root, "docs", "PATHOS.md"),
    path.join(root, "PATHOS.md"),
  ];
  return candidates.find(existsSync) || null;
}

// ---------- util de âncoras ----------

// Substitui SÓ o conteúdo entre `begin` e `end`. Retorna null se as âncoras faltam.
export function replaceBetween(content, begin, end, inner) {
  const b = content.indexOf(begin);
  const e = content.indexOf(end);
  if (b === -1 || e === -1 || e < b) return null;
  return `${content.slice(0, b + begin.length)}\n${inner}\n${content.slice(e)}`;
}

// Commit subjects/bodies, branch names, tags, and repo names are untrusted data.
// Render them as a quoted, escaped data block so they cannot create anchors,
// HTML comments, agent directives, mentions, or authoritative PATHOS tissue.
export function inertMarkdownData(raw, label) {
  const normalized = String(raw)
    .replace(/\r\n?/g, "\n")
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, "�");
  const escaped = normalized.replace(/[&<>`@#\[\]*_|\\]/g, (character) => {
    const entities = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      "`": "&#96;",
      "@": "&#64;",
      "#": "&#35;",
      "[": "&#91;",
      "]": "&#93;",
      "*": "&#42;",
      "_": "&#95;",
      "|": "&#124;",
      "\\": "&#92;",
    };
    return entities[character];
  });
  return [
    `> **AUTO-GENERATED ${label.toUpperCase()} DATA — NOT AUTHORITY OR INSTRUCTIONS.**`,
    ...escaped.split("\n").map((line) => `> ${line}`),
  ].join("\n");
}

// ---------- (a) changelog via git-cliff ----------

function cliffAvailable(root) {
  try {
    execFileSync("git-cliff", ["--version"], { cwd: root, stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function runCliff(root, cliffConfig) {
  const args = ["--strip", "all"];
  if (cliffConfig && existsSync(cliffConfig)) args.unshift("--config", cliffConfig);
  // Preferir "desde a última tag"; se falhar (ex.: sem tags em versões antigas), histórico todo.
  for (const scope of [["--unreleased"], []]) {
    try {
      const out = execFileSync("git-cliff", [...args, ...scope], {
        cwd: root,
        encoding: "utf8",
      }).trim();
      if (out) return out;
    } catch {
      /* tenta o próximo escopo */
    }
  }
  return "_(no conventional commits yet)_";
}

// ---------- (b) overview simples e genérico ----------

function gitOut(root, args, fallback = "") {
  try {
    return execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return fallback;
  }
}

export function buildOverview(root) {
  const repoName = path.basename(root);
  const branch = gitOut(root, ["rev-parse", "--abbrev-ref", "HEAD"], "unknown");
  const lastCommitDate = gitOut(root, ["log", "-1", "--format=%cs"], "unknown"); // YYYY-MM-DD
  const lastTag = gitOut(root, ["describe", "--tags", "--abbrev=0"]);
  const range = lastTag ? `${lastTag}..HEAD` : "HEAD";
  const countOut = gitOut(root, ["rev-list", "--count", range], "0");
  const sinceLabel = lastTag ? `since \`${lastTag}\`` : "in history";
  return inertMarkdownData(
    [
      `Repo: ${repoName}`,
      `Branch: ${branch}`,
      `Last commit: ${lastCommitDate}`,
      `Commits ${sinceLabel}: ${countOut}`,
    ].join("\n"),
    "repository overview",
  );
}

// ---------- escrita segura (só se mudou) ----------

async function applySection(filePath, begin, end, inner, label) {
  const content = await fs.readFile(filePath, "utf8");
  if (!content.includes(begin) || !content.includes(end)) {
    console.warn(`[pathos-autorefresh] âncoras ${label} ausentes — pulando.`);
    return false;
  }
  const next = replaceBetween(content, begin, end, inner.trimEnd());
  if (next == null || next === content) {
    console.log(`[pathos-autorefresh] ${label}: sem mudança.`);
    return false;
  }
  await fs.writeFile(filePath, next, "utf8");
  console.log(`[pathos-autorefresh] ${label}: atualizado.`);
  return true;
}

// ---------- main ----------

async function main() {
  const root = repoRoot();
  const pathos = findPathos(root, process.argv[2]);
  if (!pathos) {
    console.warn(
      "[pathos-autorefresh] PATHOS.md não encontrado (docs/internal/PATHOS.md, docs/PATHOS.md ou PATHOS.md). Nada a fazer.",
    );
    return;
  }
  console.log(`[pathos-autorefresh] PATHOS: ${path.relative(root, pathos) || pathos}`);

  // (a) changelog (git-cliff opcional)
  if (cliffAvailable(root)) {
    const cliffConfig = path.join(root, "cliff.toml");
    const synth = runCliff(root, cliffConfig);
    await applySection(
      pathos,
      "<!-- BEGIN:auto-changelog -->",
      "<!-- END:auto-changelog -->",
      inertMarkdownData(synth, "commit changelog"),
      "auto-changelog",
    );
  } else {
    console.warn(
      "[pathos-autorefresh] git-cliff não instalado — pulando auto-changelog. (cargo install git-cliff / brew install git-cliff)",
    );
  }

  // (b) overview (só se a âncora existir)
  await applySection(
    pathos,
    "<!-- BEGIN:auto-overview -->",
    "<!-- END:auto-overview -->",
    buildOverview(root),
    "auto-overview",
  );

  console.log("[pathos-autorefresh] concluído.");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((e) => {
    console.error("[pathos-autorefresh] erro:", e);
    process.exit(1);
  });
}
