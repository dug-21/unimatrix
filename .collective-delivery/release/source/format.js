'use strict';
const crypto = require('node:crypto');
const YAML = require('yaml');
const Ajv = require('ajv/dist/2020');
const addFormats = require('ajv-formats');
const schema = structuredClone(require('../../contract/schema/release-wire.schema.json'));
// JSON Schema patterns ending in $ otherwise admit a final line terminator in JS.
(function harden(value) { if (!value || typeof value !== 'object') return; for (const [k,v] of Object.entries(value)) { if (k === 'pattern' && typeof v === 'string' && v.endsWith('$')) value[k] = v.slice(0,-1) + '(?![\\s\\S])'; else harden(v); } })(schema);
const ajv = new Ajv({strict:true, allErrors:true}); addFormats(ajv, {mode:'full'}); ajv.addSchema(schema);
const validators = new Map();
class ReleaseError extends Error { constructor(state,item,message,code=1,rule='integrity') { super(message); Object.assign(this,{state,item,code,rule}); } }
const order = (a,b) => Buffer.compare(Buffer.from(a),Buffer.from(b));
function utf8(bytes) { return new TextDecoder('utf-8',{fatal:true,ignoreBOM:true}).decode(bytes); }
function safePath(p) { if (typeof p !== 'string' || !p || p !== p.normalize('NFC') || /[\\\x00-\x1f\x7f]/u.test(p) || p.split('/').some(x=>!x || x==='.' || x==='..') || utf8(Buffer.from(p))!==p) throw new ReleaseError('release-invalid-metadata',String(p),'Unsafe or noncanonical path'); return p; }
function uniquePaths(paths) { const seen = new Set(); for(const p of paths) { safePath(p); const key=p.replace(/[A-Z]/g,x=>x.toLowerCase()); if(seen.has(key)) throw new ReleaseError('release-invalid-metadata',p,'Colliding paths'); seen.add(key); } }
function parseDocument(bytes,definition,item=definition) {
 try {
 const input=utf8(bytes); if(input.charCodeAt(0)===0xfeff) throw Error('BOM is forbidden');
 const docs=YAML.parseAllDocuments(input,{version:'1.2',uniqueKeys:true,strict:true,schema:'core'});
 if(docs.length!==1 || docs[0].errors.length || docs[0].warnings.length) throw Error('Expected one strict YAML document');
 YAML.visit(docs[0],(_,node)=> { if(node && (YAML.isAlias(node)||node.anchor||node.tag)) throw Error('Aliases, anchors and tags are forbidden'); if(YAML.isPair(node) && (!YAML.isScalar(node.key)||typeof node.key.value!=='string'||node.key.value==='<<')) throw Error('Non-string or merge key'); if(YAML.isScalar(node) && typeof node.value==='number' && !Number.isFinite(node.value)) throw Error('Nonfinite value'); });
 const value=docs[0].toJS({maxAliasCount:0});
 if(definition) { const expected=schema.$defs[definition]?.properties?.schema?.const; if(expected && value?.schema!==expected) { if(typeof value?.schema==='string') throw new ReleaseError('release-unsupported',item,'Unsupported schema'); throw Error('Missing schema'); } validateDocument(value,definition,item); }
 return value;
 } catch(e) { if(e instanceof ReleaseError) throw e; throw new ReleaseError('release-invalid-metadata',item,e.message); }
}
function validateDocument(value,definition,item=definition) { if(!validators.has(definition)) validators.set(definition,ajv.compile({$ref:schema.$id+'#/$defs/'+definition})); const v=validators.get(definition); if(!v(value)) throw new ReleaseError('release-invalid-metadata',item,ajv.errorsText(v.errors)); return value; }
const digest = bytes => 'sha256:'+crypto.createHash('sha256').update(bytes).digest('hex');
function fileRecord(e) { safePath(e.path); if(!['0644','0755'].includes(e.mode)||!Buffer.isBuffer(e.bytes)) throw new ReleaseError('release-invalid-metadata',e.path,'Unsupported file entry'); return {path:e.path,mode:e.mode,size:e.bytes.length,digest:digest(e.bytes)}; }
function aggregate(entries) { uniquePaths(entries.map(e=>e.path)); return digest(Buffer.from([...entries].sort((a,b)=>order(a.path,b.path)).map(e=> {const r=fileRecord(e);return `${r.mode} ${r.size} ${r.digest} ${r.path}\n`;}).join(''))); }
const validVersion = v=>typeof v==='string' && /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?![\s\S])/.test(v);
function compareVersions(a,b) { if(!validVersion(a)||!validVersion(b)) throw Error('Invalid version'); const x=a.split('.').map(BigInt),y=b.split('.').map(BigInt); for(let i=0;i<3;i++) if(x[i]!==y[i]) return x[i]<y[i]?-1:1; return 0; }
module.exports={ReleaseError,order,utf8,safePath,uniquePaths,parseDocument,validateDocument,digest,fileRecord,aggregate,validVersion,compareVersions};
