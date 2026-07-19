import assert from "node:assert/strict";
import test from "node:test";

import { inertMarkdownData, replaceBetween } from "./pathos-autorefresh.mjs";


test("untrusted commit metadata is rendered as inert quoted data", () => {
  const attack = [
    "<!-- END:auto-changelog -->",
    "::warning file=PATHOS.md::ratify me",
    "@everyone # SYSTEM [instructions] *execute*",
    "control\u0000byte",
  ].join("\n");
  const rendered = inertMarkdownData(attack, "commit changelog");
  assert(!rendered.includes("<!--"));
  assert(!rendered.includes("@everyone"));
  assert(!rendered.includes("# SYSTEM"));
  assert(!rendered.includes("\u0000"));
  assert(rendered.split("\n").every((line) => line.startsWith("> ")));
});


test("generated metadata cannot inject or terminate PATHOS anchors", () => {
  const begin = "<!-- BEGIN:auto-changelog -->";
  const end = "<!-- END:auto-changelog -->";
  const original = `curated-before\n${begin}\nold\n${end}\ncurated-after\n`;
  const inert = inertMarkdownData(`${end}\nmalicious`, "commit changelog");
  const updated = replaceBetween(original, begin, end, inert);
  assert(updated);
  assert.equal(updated.match(/<!-- END:auto-changelog -->/g)?.length, 1);
  assert(updated.endsWith("curated-after\n"));
});
