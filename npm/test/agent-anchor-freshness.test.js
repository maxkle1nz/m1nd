"use strict";

const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");
const { checkAnchorFreshness } = require("../lib/agent-anchor-freshness");

const files = [path.resolve("repo-alpha", "src/one.js"), path.resolve("repo-alpha", "src/two.js")];
const payload = (changes = {}) => ({ checked: 2, fresh: [...files], stale: [], unknown: [], ...changes });
function fixture(response, error = false) {
  const calls = [];
  return {
    calls,
    tools() { throw new Error("hidden tool availability must not depend on tools/list"); },
    async callTool(name, args) {
      calls.push({ name, args });
      if (response instanceof Error) throw response;
      return { result: { isError: error, content: [{ type: "text", text: JSON.stringify(response) }] } };
    },
  };
}

test("explicit anchor check calls hidden am_i_stale once and accepts exact fresh coverage", async () => {
  const client = fixture(payload({ fresh: [...files].reverse(), notes: ["private diagnostic"], summary: "untrusted text" }));
  const receipt = await checkAnchorFreshness(client, "anchor-probe", files);
  assert.equal(receipt.status, "fresh_at_boundary");
  assert.equal(receipt.scope, "explicit_files_only");
  assert.equal(receipt.atomic_snapshot, false);
  assert.equal(receipt.checked, 2);
  assert.deepEqual(receipt.files, files);
  assert.deepEqual(client.calls, [{ name: "am_i_stale", args: { agent_id: "anchor-probe", files } }]);
  assert.ok(!JSON.stringify(receipt).includes("private diagnostic"));
  assert.ok(!JSON.stringify(receipt).includes("untrusted text"));
});

test("one and eight explicit files are accepted without truncation or input mutation", async () => {
  for (const count of [1, 8]) {
    const requested = Array.from({ length: count }, (_, index) => path.resolve("repo-alpha", `source${index}.js`));
    const before = [...requested];
    const client = fixture(payload({ checked: count, fresh: [...requested] }));
    const check = client.callTool;
    client.callTool = async (name, args) => {
      const result = await check(name, args);
      args.files.push(path.resolve("unexpected", "not-requested.js"));
      return result;
    };
    const result = await checkAnchorFreshness(client, "anchor-probe", requested);
    assert.equal(result.status, "fresh_at_boundary");
    assert.equal(result.files.length, count);
    assert.deepEqual(requested, before);
    assert.deepEqual(result.files, before);
  }
});

test("matching changed or missing anchor invalidates context, including mixed unknown coverage", async () => {
  for (const reason of ["changed", "missing"]) {
    const receipt = await checkAnchorFreshness(fixture(payload({ fresh: [], stale: [{ path: files[0], reason, extra: "ignored" }], unknown: [files[1]] })), "anchor-probe", files);
    assert.equal(receipt.status, "stale");
    assert.deepEqual(receipt.stale, [{ path: files[0], reason }]);
    assert.deepEqual(receipt.unknown, [files[1]]);
    assert.ok(!JSON.stringify(receipt).includes("ignored"));
  }
});

test("unknown and incomplete checks never report fresh", async () => {
  for (const response of [
    payload({ fresh: [files[0]], unknown: [files[1]] }),
    payload({ checked: 1, fresh: [files[0]] }),
    payload({ checked: 0, fresh: [] }),
  ]) {
    const receipt = await checkAnchorFreshness(fixture(response), "anchor-probe", files);
    assert.equal(receipt.status, "not_proven");
  }
});

test("malformed, duplicate and unrequested native paths cannot escape through the receipt", async () => {
  const outside = path.resolve("unrequested", "private.js");
  for (const response of [
    null, "not an object", {}, payload({ checked: "2" }), payload({ checked: -1 }),
    payload({ checked: 3 }), payload({ fresh: [...files, outside] }),
    payload({ checked: 0, fresh: [], stale: [{ path: files[0], reason: "changed" }] }),
    payload({ fresh: [files[0], files[0]] }), payload({ fresh: [files[0], {}] }),
    payload({ fresh: [files[0]], stale: [{ path: files[0], reason: "changed" }] }),
    payload({ fresh: [files[0]], unknown: [outside] }),
    payload({ fresh: [files[0]], stale: [{ path: outside, reason: "changed" }] }),
    payload({ fresh: [files[0]], stale: [{ path: files[1], reason: "unexpected reason" }] }),
    payload({ fresh: [files[0]], stale: [{ path: files[1] }] }),
    payload({ fresh: [files[0]], unknown: "invalid" }),
  ]) {
    const receipt = await checkAnchorFreshness(fixture(response), "anchor-probe", files);
    assert.equal(receipt.status, "not_proven", JSON.stringify(response));
    assert.deepEqual(receipt.stale, []);
    assert.deepEqual(receipt.unknown, []);
    assert.ok(!JSON.stringify(receipt).includes(outside));
    assert.ok(!JSON.stringify(receipt).includes("unexpected reason"));
  }
});

test("tool failures stay not proven without returning native or transport error text", async () => {
  for (const client of [fixture(new Error("private transport detail")), fixture({ error: "private native detail" }, true)]) {
    const receipt = await checkAnchorFreshness(client, "anchor-probe", files);
    assert.equal(receipt.status, "not_proven");
    assert.equal(receipt.reason, "tool_error");
    assert.ok(!JSON.stringify(receipt).includes("private"));
    assert.equal(client.calls.length, 1);
  }
});

test("invalid or oversized inputs do not call the native tool or silently truncate", async () => {
  for (const requested of [undefined, [], Array(1), ["relative.js"], [files[0], files[0]], [files[0], null],
    [files[0] + "\0"], Array.from({ length: 9 }, (_, index) => path.resolve("repo-alpha", `file${index}.js`))]) {
    const client = fixture(payload());
    const receipt = await checkAnchorFreshness(client, "anchor-probe", requested);
    assert.equal(receipt.status, "not_proven");
    assert.equal(receipt.reason, "invalid_input");
    assert.deepEqual(receipt.files, []);
    assert.equal(client.calls.length, 0);
  }
  for (const agentId of [null, 0, "", "\0"]) {
    const client = fixture(payload());
    assert.equal((await checkAnchorFreshness(client, agentId, files)).status, "not_proven");
    assert.equal(client.calls.length, 0);
  }
});
