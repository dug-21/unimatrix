'use strict';
const {isDeepStrictEqual:eq}=require('node:util');
const {ReleaseError,parseDocument,digest,aggregate,fileRecord,compareVersions,order}=require('../source/format');
const FILES=['CLEARANCES.yaml','CONTRACT.md','MANIFEST.yaml','OBLIGATIONS.yaml','REGISTRATIONS.yaml','RELEASE.yaml','VOCABULARY.yaml'];
const DEFINITIONS={'CLEARANCES.yaml':'clearances','MANIFEST.yaml':'manifest','OBLIGATIONS.yaml':'obligations','REGISTRATIONS.yaml':'registration-set','RELEASE.yaml':'release','VOCABULARY.yaml':'vocabulary'};
function diagnose(state,item,message,rule='integrity'){return {state,rule,item,message};}
function sortDiagnostics(diagnostics){return diagnostics.sort((a,b)=>order(a.item,b.item)||order(a.rule,b.rule)||order(a.state,b.state));}
// The expected release file set is derived PER RELEASE, never as a global constant: the
// fixed kernel seven ∪ that release's own program-facing payload, self-described by its
// MANIFEST.files[] (bound to the accepted index by content_digest). 0.2.0 declares no
// payload ⇒ exactly seven, unchanged (DR-01). Payload paths are absent from DEFINITIONS,
// so payload is integrity-checked (fileRecord) but never schema-parsed (schemaless).
function payloadPaths(entries) {
 const m=entries.find(e=>e.path==='MANIFEST.yaml');
 if(!m) return [];
 try {const manifest=parseDocument(m.bytes,'manifest','MANIFEST.yaml');return [...new Set(manifest.files.map(r=>r.path).filter(p=>!FILES.includes(p)))];}
 catch(e){return [];}
}
function inspect(entries,expected,authorityEntries) {
 const diagnostics=[];const add=(s,p,m)=>diagnostics.push(diagnose(s,p,m));const byPath=new Map(entries.map(e=>[e.path,e]));const docs={};
 const expectedSet=[...FILES,...payloadPaths(entries)];
 for(const p of expectedSet)if(!byPath.has(p))add('release-missing-content',p,'Required release file absent');
 for(const e of entries){if(!expectedSet.includes(e.path))add('release-unexpected-content',e.path,'Unexpected release file');if(e.mode!=='0644')add('release-changed-content',e.path,'Release mode must be 0644');if(DEFINITIONS[e.path])try{docs[e.path]=parseDocument(e.bytes,DEFINITIONS[e.path],e.path);}catch(error){add(error.state,e.path,error.message);}}
 if(authorityEntries)for(const e of authorityEntries){const actual=byPath.get(e.path);if(actual&&!eq(fileRecord(actual),fileRecord(e)))add('release-changed-content',e.path,'Bytes, size or mode differ from accepted release');}
 const manifest=docs['MANIFEST.yaml'];
 if(manifest){const listed=new Map();for(const r of manifest.files){if(listed.has(r.path)||!expectedSet.includes(r.path)||r.path==='MANIFEST.yaml')add('release-invalid-metadata','MANIFEST.yaml','Manifest set is not the release file tree');listed.set(r.path,r);const actual=byPath.get(r.path);if(actual&&!eq(fileRecord(actual),r))add('release-changed-content',r.path,'Manifest record does not match captured file');}for(const p of expectedSet.filter(p=>p!=='MANIFEST.yaml'))if(!listed.has(p))add('release-invalid-metadata','MANIFEST.yaml',`Manifest omits ${p}`);}
 const registrations=docs['REGISTRATIONS.yaml'];if(registrations&&!eq(registrations.source,expected.binding.registry_source))add('release-invalid-metadata','REGISTRATIONS.yaml','Registry source differs from accepted binding');
 const release=docs['RELEASE.yaml'];if(release&&(!eq(release.metadata,expected.metadata)||!eq(release.binding,expected.binding)))add('release-invalid-metadata','RELEASE.yaml','Metadata or binding differs from accepted index');
 if(aggregate(entries)!==expected.content_digest && !diagnostics.some(d=>['release-changed-content','release-missing-content','release-unexpected-content'].includes(d.state)))add('release-changed-content','MANIFEST.yaml','Whole-content digest differs from accepted index');
 return sortDiagnostics([...new Map(diagnostics.map(d=>[JSON.stringify(d),d])).values()]);
}
function readAuthority(source) {
 let raw;try {raw=source.readFile('baseline/RELEASES.yaml');}catch(e){throw e;}
 try {
 const index=parseDocument(raw.bytes,'release-index','baseline/RELEASES.yaml');
 const releases=source.readTree('baseline/releases',{optional:true});const verdicts=source.readTree('baseline/verdicts',{optional:true});
 const known=new Map();const expectedVerdicts=new Set();let previous;
 for(const entry of index.entries){const v=entry.metadata.release_identity;
 if(known.has(v)|| (previous&&compareVersions(v,previous)<=0))throw Error('Index versions must be unique and increasing');
 if(!eq(entry.metadata.predecessor,previous?{state:'promoted-release',version:previous}:{state:'unpromoted-prior-version',version:'0.1.0'}))throw Error('Invalid predecessor');
 if(!previous&&(v!=='0.2.0'||entry.metadata.compatibility!=='breaking'))throw Error('Invalid first release');
 const supersedes=entry.metadata.supersedes;for(let i=0;i<supersedes.length;i++)if(!known.has(supersedes[i])||(i&&compareVersions(supersedes[i-1],supersedes[i])>=0))throw Error('Invalid supersedes set');
 const vp=entry.binding.candidate_digest.slice(7)+'.yaml';if(expectedVerdicts.has(vp))throw Error('Repeated candidate binding');expectedVerdicts.add(vp);
 const verdictFile=verdicts.find(e=>e.path===vp);if(!verdictFile||verdictFile.mode!=='0644'||digest(verdictFile.bytes)!==entry.promotion.gate_verdict_digest)throw Error('Missing or changed authorizing verdict');
 const verdict=parseDocument(verdictFile.bytes,'gate-verdict',vp);if(!eq(verdict.binding,entry.binding)||new Set(verdict.executed.map(x=>x.check)).size!==17)throw Error('Verdict binding/check set mismatch');
 const contents=releases.filter(e=>e.path.startsWith(v+'/')).map(e=>({...e,path:e.path.slice(v.length+1)}));
 known.set(v,{entry,entries:contents,verdict,verdictFile});previous=v;
 }
 if(verdicts.length!==expectedVerdicts.size||verdicts.some(e=>!expectedVerdicts.has(e.path)))throw Error('Orphan verdict');
 if(releases.some(e=>!known.has(e.path.split('/')[0])))throw Error('Unindexed release directory');
 return {index,indexBytes:raw.bytes,known};
 }catch(e){throw new ReleaseError('source-invalid','baseline/RELEASES.yaml',e.message,3,'authority-closure');}
}
module.exports={FILES,DEFINITIONS,diagnose,sortDiagnostics,inspect,readAuthority};
