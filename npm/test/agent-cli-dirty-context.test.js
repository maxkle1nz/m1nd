"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");
const CLI = path.resolve(__dirname, "../bin/m1nd.js");
const POSIX = { skip: process.platform === "win32" && "POSIX fixture launcher" };

function run(mode) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "m1nd-dirty-context-"));
  try {
    const repo = path.join(root, "repo-alpha"), binary = path.join(root, "fixture.js");
    fs.mkdirSync(repo);
    const anchor = path.join(repo, "signal.js");
    fs.writeFileSync(anchor, "export const signal = 1;\n");
    fs.writeFileSync(binary, `#!${process.execPath}\n` + `
const fs=require('node:fs'), readline=require('node:readline');
if(process.argv.includes('--version')){console.log('m1nd-mcp 1.6.5');process.exit(0);}
const file=process.env.FIXTURE_ANCHOR, mode=process.env.FIXTURE_DRIFT;
const original=fs.readFileSync(file,'utf8');
readline.createInterface({input:process.stdin}).on('line',line=>{
 const q=JSON.parse(line);let result={};
 if(q.method==='tools/list')result={tools:['trust_selftest','session_handshake','surgical_context_v2'].map(name=>({name}))};
 if(q.method==='tools/call'){
  const name=q.params.name;let payload={};
  if(name==='trust_selftest')payload={verdict:'full_trust',graph_state:{node_count:1}};
  if(name==='session_handshake'){
   payload={trust_mode:'full_trust',graph_state:{node_count:1}};
   if(mode==='before')fs.writeFileSync(file,'export const signal = 2;\\n');
  }
  if(name==='am_i_stale'){
   const changed=fs.readFileSync(file,'utf8')!==original;
   const requested=q.params.arguments.files[0];
   payload={checked:1,fresh:mode==='unknown'||changed?[]:[requested],unknown:mode==='unknown'?[requested]:[],stale:changed?[{path:requested,reason:'changed'}]:[],notes:['private-note-not-forwarded']};
  }
  if(name==='surgical_context_v2'){
   payload={ok:true,primary_file:{file_path:file,source:'obsolete-capsule-sentinel'}};
   if(mode==='after')fs.writeFileSync(file,'export const signal = 3;\\n');
  }
  result={content:[{type:'text',text:JSON.stringify(payload)}]};
 }
 process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:q.id,result})+'\\n');
});`, { mode: 0o700 });
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("M1ND_") && !key.startsWith("GIT_")));
    Object.assign(env, { FIXTURE_ANCHOR: anchor, FIXTURE_DRIFT: mode, M1ND_REGISTRY_DIR: path.join(root, "registry") });
    const command = spawnSync(process.execPath, [CLI, "agent", "context", "--repo", repo,
      "--binary", binary, "--no-attach", "--shared-runtime", "--query", "signal", "--anchor", "signal.js", "--json"],
    { encoding: "utf8", timeout: 15000, env });
    assert.equal(command.error, undefined);
    assert.equal(command.signal, null, command.stderr);
    return { status: command.status, payload: JSON.parse(command.stdout) };
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}

for (const phase of ["before", "after"]) {
  test(`context suppresses a capsule when uncommitted source changes ${phase} retrieval`, POSIX, () => {
    const { status, payload } = run(phase);
    assert.equal(status, 1);
    assert.equal(payload.ok, false);
    assert.equal(payload.status, "source_changed_during_context");
    assert.deepEqual(payload.results, []);
    assert.ok(!JSON.stringify(payload).includes("obsolete-capsule-sentinel"));
    assert.ok(!JSON.stringify(payload).includes("private-note-not-forwarded"));
    const contextCalls = payload.calls.filter(call => call.tool === "surgical_context_v2");
    assert.equal(contextCalls.length, phase === "before" ? 0 : 1);
  });
}

test("context reports unknown anchor freshness without inventing a fresh result or retrying", POSIX, () => {
  const { status, payload } = run("unknown");
  assert.equal(status, 0);
  assert.equal(payload.source_freshness.status, "not_proven");
  assert.equal(payload.calls.filter(call => call.tool === "am_i_stale").length, 1);
  assert.ok(!JSON.stringify(payload).includes("private-note-not-forwarded"));
});
