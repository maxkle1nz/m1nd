'use strict';
const nodeTest = require('node:test');
const test = (name, fn) => nodeTest(name, { skip: !fs.constants.O_NOFOLLOW && 'Memory review requires no-follow file reads' }, fn);
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { reviewAgentMemory } = require('../lib/agent-memory-review');
const hash = x => crypto.createHash('sha256').update(x).digest('hex');
function fixture(t) {
 const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'memory-review-'))); t.after(()=>fs.rmSync(base,{recursive:true,force:true}));
 const repo=path.join(base,'repo'); fs.mkdirSync(repo); fs.writeFileSync(path.join(repo,'a.js'),'old');
 const identity={schema:'m1nd-agent-runtime-cache-v1',canonical_root:repo,source_revision:{kind:'git',head:'a'.repeat(40),branch:'main'}};
 const runtimeDir=path.join(base,hash(JSON.stringify(identity))); fs.mkdirSync(runtimeDir); fs.writeFileSync(path.join(runtimeDir,'agent-cache-identity.json'),JSON.stringify(identity));
 const memory=path.join(runtimeDir,'agent-memory'); fs.mkdirSync(memory);
 const doc='---\nCreated: 123\n---\n[𝔻 evidence: a.js]\n'; fs.writeFileSync(path.join(memory,'one.light.md'),doc);
 const provenance={schema:'m1nd-memory-sharing-baseline-v1',source_identity:identity,documents:[{name:'one.light.md',sha256:hash(doc),evidence:[{path:'a.js',sha256:hash('old')}]}]};
 return {repo,runtimeDir,identity,provenance,memory};
}
test('complete supplied baseline is only a candidate; no mutation or claim text',t=>{
 const f=fixture(t); const before=fs.readdirSync(f.runtimeDir); const r=reviewAgentMemory(f);
 assert.equal(r.documents[0].status,'candidate_only'); assert.equal(r.authored_provenance,'supplied_unverified'); assert.equal(r.sharing_authorized,false); assert.equal(r.semantic_validity,'not_certified'); assert.equal(r.mutations,false);
 assert.deepEqual(fs.readdirSync(f.runtimeDir),before); assert.equal(JSON.stringify(r).includes('Created:'),false);
 assert.deepEqual(r.baseline_source_identity,f.provenance.source_identity); assert.equal(r.baseline_identity_authenticated,false); assert.equal(r.observations_atomic,false);
});
test('legacy and partial baselines cannot become candidates',t=>{
 const f=fixture(t); assert.equal(reviewAgentMemory({...f,provenance:undefined}).documents[0].status,'origin_not_attested');
 f.provenance.documents[0].evidence=[]; assert.notEqual(reviewAgentMemory(f).documents[0].status,'candidate_only');
});
test('dirty, delete, rename and changed document invalidate',t=>{
 const f=fixture(t); fs.writeFileSync(path.join(f.repo,'a.js'),'changed'); assert.equal(reviewAgentMemory(f).documents[0].status,'invalidated');
 fs.unlinkSync(path.join(f.repo,'a.js')); assert.equal(reviewAgentMemory(f).documents[0].status,'invalidated');
 fs.writeFileSync(path.join(f.repo,'renamed.js'),'old'); assert.equal(reviewAgentMemory(f).documents[0].status,'invalidated');
 fs.writeFileSync(path.join(f.repo,'a.js'),'old'); fs.appendFileSync(path.join(f.memory,'one.light.md'),'changed'); assert.equal(reviewAgentMemory(f).documents[0].status,'invalidated');
});
test('identity, provenance root, duplicate and malformed refusal',t=>{
 const f=fixture(t); assert.throws(()=>reviewAgentMemory({...f,identity:{...f.identity,canonical_root:'/wrong'}}));
 f.provenance.source_identity={...f.identity,canonical_root:'/wrong'}; assert.throws(()=>reviewAgentMemory(f));
 f.provenance.source_identity=f.identity; f.provenance.documents.push(f.provenance.documents[0]); assert.throws(()=>reviewAgentMemory(f));
});
test('unsafe cross-platform paths and symlinks never candidates',t=>{
 const f=fixture(t);
 for(const p of ['../x','/tmp/x','C:\\x','..\\x','a.js:2']) {
  fs.writeFileSync(path.join(f.memory,'one.light.md'),`[𝔻 evidence: ${p}]\n`);
  const r=reviewAgentMemory({...f,provenance:undefined}); assert.equal(r.documents[0].evidence[0].status,'unsafe_path');
 }
 fs.writeFileSync(path.join(f.memory,'one.light.md'),'[𝔻 evidence: link.js]\n'); fs.symlinkSync('a.js',path.join(f.repo,'link.js'));
 assert.equal(reviewAgentMemory({...f,provenance:undefined}).documents[0].evidence[0].status,'unsafe_path');
 fs.unlinkSync(path.join(f.memory,'one.light.md')); fs.symlinkSync(path.join(f.repo,'a.js'),path.join(f.memory,'one.light.md')); assert.throws(()=>reviewAgentMemory(f));
});
test('hard bounds reject rather than silently truncate',t=>{
 const f=fixture(t); fs.writeFileSync(path.join(f.memory,'one.light.md'),'x'.repeat(65537)); assert.throws(()=>reviewAgentMemory(f));
 fs.writeFileSync(path.join(f.memory,'one.light.md'),Array.from({length:17},(_,i)=>`[𝔻 evidence: x${i}]`).join('\n')); assert.throws(()=>reviewAgentMemory(f));
 fs.writeFileSync(path.join(f.memory,'one.light.md'),''); for(let i=0;i<32;i++) fs.writeFileSync(path.join(f.memory,`${i}.light.md`),''); assert.throws(()=>reviewAgentMemory(f));
});
test('descriptor detects in-flight file mutation and unreadable evidence is inconclusive',t=>{
 const f=fixture(t); const original=fs.readSync; let changed=false;
 fs.readSync=function(fd,...args) { const n=original.call(this,fd,...args); if(!changed) { changed=true; fs.appendFileSync(path.join(f.runtimeDir,'agent-cache-identity.json'),' '); } return n; };
 try { assert.throws(()=>reviewAgentMemory(f),{code:'M1ND_MEMORY_REVIEW_REFUSED'}); } finally { fs.readSync=original; }
 fs.writeFileSync(path.join(f.runtimeDir,'agent-cache-identity.json'),JSON.stringify(f.identity));
 const open=fs.openSync;
 fs.openSync=function(p,...args) { if(p===path.join(f.repo,'a.js')) { const e=new Error('fixture denied'); e.code='EACCES'; throw e; } return open.call(this,p,...args); };
 try { const r=reviewAgentMemory(f); assert.equal(r.documents[0].status,'origin_not_attested'); assert.equal(r.documents[0].evidence[0].status,'unreadable'); } finally { fs.openSync=open; }
});
test('no write operations and no sibling view reads; retained tree bytes match',t=>{
 const f=fixture(t); const sibling=path.join(path.dirname(f.runtimeDir),'sibling'); fs.mkdirSync(sibling); fs.writeFileSync(path.join(sibling,'secret'),'secret');
 const snapshot=p=>Object.fromEntries(fs.readdirSync(p,{recursive:true}).sort().map(n=>{const q=path.join(p,n);return[n,fs.lstatSync(q).isFile()?hash(fs.readFileSync(q)):'directory'];}));
 const before=snapshot(path.dirname(f.runtimeDir)); const saved={};
 for(const key of ['writeFileSync','mkdirSync','renameSync','unlinkSync','rmSync']) { saved[key]=fs[key]; fs[key]=()=>{throw new Error('unexpected mutation');}; }
 try { assert.equal(reviewAgentMemory(f).documents[0].status,'candidate_only'); } finally { for(const [k,v] of Object.entries(saved)) fs[k]=v; }
 assert.deepEqual(snapshot(path.dirname(f.runtimeDir)),before);
});
test('manifest mismatch, dangling memory symlink and evidence size fail closed',t=>{
 const f=fixture(t); fs.writeFileSync(path.join(f.runtimeDir,'agent-cache-identity.json'),JSON.stringify({...f.identity,source_revision:{...f.identity.source_revision,branch:'other'}})); assert.throws(()=>reviewAgentMemory(f));
 fs.writeFileSync(path.join(f.runtimeDir,'agent-cache-identity.json'),JSON.stringify(f.identity)); fs.writeFileSync(path.join(f.repo,'a.js'),'x'.repeat(1024*1024+1)); assert.throws(()=>reviewAgentMemory(f));
 fs.rmSync(f.memory,{recursive:true}); fs.symlinkSync(path.join(f.repo,'missing'),f.memory); assert.throws(()=>reviewAgentMemory(f));
});
