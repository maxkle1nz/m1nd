'use strict';
// Read-only byte comparisons. Supplied baselines are assertions, not authenticated authorship.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const fail = message => { const e = new Error(`memory review refused: ${message}`); e.code = 'M1ND_MEMORY_REVIEW_REFUSED'; throw e; };
const record = x => x && typeof x === 'object' && !Array.isArray(x);
const exact = (x, keys) => record(x) && Object.keys(x).sort().join('|') === keys.slice().sort().join('|');
const sha = x => typeof x === 'string' && /^[a-f0-9]{64}$/.test(x);
function revision(x) {
 if (!exact(x,['kind','head','branch'])) return false;
 if (x.kind === 'non_git') return x.head === null && x.branch === null;
 if (typeof x.branch !== 'string' || !x.branch || x.branch.length>512) return false;
 return x.kind === 'git_unborn' ? x.head === null : x.kind === 'git' && typeof x.head === 'string' && /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/i.test(x.head);
}
function validIdentity(x) { return exact(x,['schema','canonical_root','source_revision']) && x.schema==='m1nd-agent-runtime-cache-v1' && typeof x.canonical_root==='string' && path.isAbsolute(x.canonical_root) && revision(x.source_revision); }
function identityValue(x) { return JSON.stringify([x.schema,x.canonical_root,x.source_revision.kind,x.source_revision.head,x.source_revision.branch]); }
function stamp(s) { return [s.dev,s.ino,s.mode,s.size,s.mtimeMs,s.ctimeMs].join(':'); }
function directory(p, guards) {
 const s=fs.lstatSync(p); if(!s.isDirectory() || s.isSymbolicLink()) fail('unsafe directory');
 guards.push(()=>{if(stamp(fs.lstatSync(p))!==stamp(s)) fail('directory changed during review');});
}
function readFile(p, limit, guards) {
 const before=fs.lstatSync(p); if(!before.isFile() || before.isSymbolicLink() || before.size>limit) fail('unsafe or oversized file');
 if (!fs.constants.O_NOFOLLOW) fail('nofollow unavailable');
 const fd=fs.openSync(p,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW);
 try {
  const opened=fs.fstatSync(fd); if(stamp(opened)!==stamp(before)) fail('file changed before read');
  // Bound reads even when another process grows the file after the initial stat.
  const buffer=Buffer.alloc(limit+1); let n=0;
  while(n<buffer.length) { const got=fs.readSync(fd,buffer,n,buffer.length-n,null); if(!got) break; n+=got; }
  if(n>limit || stamp(fs.fstatSync(fd))!==stamp(before) || stamp(fs.lstatSync(p))!==stamp(before)) fail('file changed during read');
  guards.push(()=>{if(stamp(fs.lstatSync(p))!==stamp(before)) fail('file changed during review');});
  return buffer.subarray(0,n);
 } finally { fs.closeSync(fd); }
}
function safeRelative(p) { return typeof p==='string' && p.length>0 && p.length<=1024 && !/[\\:\x00-\x1f\x7f]/.test(p) && !path.posix.isAbsolute(p) && p.split('/').every(x=>x && x!=='.' && x!=='..'); }
function evidenceRead(repo, rel, guards, budget) {
 if(!safeRelative(rel)) return {path:rel,status:'unsafe_path'};
 try {
  const parts=rel.split('/'); let dir=repo;
  for(const part of parts.slice(0,-1)) { dir=path.join(dir,part); directory(dir,guards); }
  const p=path.join(repo,...parts); const s=fs.lstatSync(p);
  if(s.isSymbolicLink() || !s.isFile()) return {path:rel,status:'unsafe_path'};
  if(s.size>1024*1024 || (budget.bytes+=s.size)>16*1024*1024) fail('evidence byte budget exceeded');
  return {path:rel,status:'observed',observed_sha256:digest(readFile(p,1024*1024,guards))};
 } catch(e) {
  if(e.code==='ENOENT') return {path:rel,status:'missing'};
  if(e.code==='M1ND_MEMORY_REVIEW_REFUSED') throw e;
  return {path:rel,status:'unreadable'};
 }
}
function parseEvidence(bytes) {
 const text=bytes.toString('utf8'); if(!Buffer.from(text).equals(bytes)) fail('invalid UTF-8');
 const paths=[]; const re=/\[𝔻 evidence: ([^\]\r\n]*)\]/gu; let m;
 while((m=re.exec(text))) paths.push(m[1]);
 if((text.match(/\[𝔻 evidence:/gu)||[]).length!==paths.length || paths.length>16 || new Set(paths).size!==paths.length) fail('malformed, duplicate or excessive evidence');
 return paths;
}
function baseline(value, repo) {
 if(value===undefined || value===null) return null;
 let encoded; try { encoded=JSON.stringify(value); } catch(_) { fail('invalid provenance'); }
 if(!encoded || Buffer.byteLength(encoded)>131072) fail('oversized provenance');
 // Work with a bounded inert copy, never caller getters during comparisons.
 const p=JSON.parse(encoded);
 if(!exact(p,['schema','source_identity','documents']) || p.schema!=='m1nd-memory-sharing-baseline-v1' || !validIdentity(p.source_identity) || p.source_identity.canonical_root!==repo || !Array.isArray(p.documents) || p.documents.length>32) fail('invalid provenance identity');
 const out=new Map();
 for(const d of p.documents) {
  if(!exact(d,['name','sha256','evidence']) || !safeRelative(d.name) || d.name.includes('/') || !d.name.endsWith('.light.md') || !sha(d.sha256) || !Array.isArray(d.evidence) || d.evidence.length>16 || out.has(d.name)) fail('invalid provenance document');
  const ev=new Map();
  for(const e of d.evidence) { if(!exact(e,['path','sha256']) || !safeRelative(e.path) || !sha(e.sha256) || ev.has(e.path)) fail('invalid provenance evidence'); ev.set(e.path,e.sha256); }
  out.set(d.name,{sha256:d.sha256,evidence:ev});
 }
 return out;
}
function reviewAgentMemory({repo,runtimeDir,identity,provenance}={}) {
 const guards=[]; const budget={bytes:0};
 try {
  if(!validIdentity(identity) || typeof repo!=='string' || !path.isAbsolute(repo) || typeof runtimeDir!=='string' || !path.isAbsolute(runtimeDir)) fail('invalid selection');
  const canonical=fs.realpathSync(repo); if(canonical!==identity.canonical_root) fail('repo identity mismatch');
  directory(repo,guards); directory(runtimeDir,guards);
  const key=digest(JSON.stringify(identity)); if(path.basename(runtimeDir)!==key) fail('cache key mismatch');
  const manifest=JSON.parse(readFile(path.join(runtimeDir,'agent-cache-identity.json'),8192,guards).toString('utf8'));
  if(!validIdentity(manifest) || identityValue(manifest)!==identityValue(identity)) fail('manifest mismatch');
  const supplied=baseline(provenance,canonical);
  const output={schema:'m1nd-memory-review-v1',status:'reviewed',view_id:key,source_revision_observed:{...identity.source_revision},authored_provenance:supplied?'supplied_unverified':'absent',documents:[],semantic_validity:'not_certified',sharing_authorized:false,mutations:false};
  const memory=path.join(runtimeDir,'agent-memory');
  try { fs.lstatSync(memory); } catch(e) { if(e.code!=='ENOENT') throw e; output.status='no_memory_observed'; for(const g of guards) g(); return output; }
  directory(memory,guards);
  const names=fs.readdirSync(memory); if(names.length>256) fail('excessive directory entries');
  const files=names.filter(n=>n.endsWith('.light.md')).sort(); if(files.length>32) fail('excessive memory files');
  for(const name of files) {
   if(!safeRelative(name) || name.includes('/')) fail('unsafe memory name');
   const bytes=readFile(path.join(memory,name),65536,guards); const observed=digest(bytes); const paths=parseEvidence(bytes); const b=supplied?.get(name);
   const evidence=paths.map(p=>evidenceRead(canonical,p,guards,budget));
   let status='origin_not_attested';
   if(b) {
    let invalid=b.sha256!==observed; let complete=paths.length>0 && b.evidence.size===paths.length;
    for(const e of evidence) {
     const expected=b.evidence.get(e.path); if(expected) e.expected_sha256=expected; else complete=false;
     if(e.status==='missing'||e.status==='unsafe_path') invalid=true;
     if(e.status==='unreadable') complete=false;
     if(expected && e.observed_sha256) { e.status=e.observed_sha256===expected?'matched':'changed'; if(e.status==='changed') invalid=true; }
    }
    status=invalid?'invalidated':complete?'candidate_only':'origin_not_attested';
   }
   output.documents.push({name,status,observed_sha256:observed,evidence});
  }
  output.baseline_documents_not_observed=supplied?[...supplied.keys()].filter(n=>!files.includes(n)):[];
  for(const g of guards) g(); return output;
 } catch(e) { if(e.code==='M1ND_MEMORY_REVIEW_REFUSED') throw e; fail('unreadable or changing selection'); }
}
module.exports={reviewAgentMemory};
