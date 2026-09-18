'use strict';
const fs=require('node:fs');
const path=require('node:path');
const {ReleaseError,utf8,uniquePaths,order}=require('./format');
const C=fs.constants;
const stable=(a,b)=>['dev','ino','mode','size','mtimeNs','ctimeNs','nlink'].every(k=>a[k]===b[k]);
// Every parent is held open, including the ancestors of an offered absolute path.
function snapshotDirectory(directory) {
 const handles=[]; const checks=[]; const entries=[];
 const openDir=p=> { const fd=fs.openSync(p,C.O_RDONLY|C.O_DIRECTORY|C.O_NOFOLLOW); handles.push(fd); return fd; };
 try {
 let fd=openDir('/'); const absolute=path.resolve(directory);
 for(const part of absolute.split('/').filter(Boolean)) {const p=`/proc/self/fd/${fd}/${part}`,before=fs.lstatSync(p,{bigint:true});fd=openDir(p);if(!stable(before,fs.fstatSync(fd,{bigint:true})))throw Error('Directory replaced');checks.push({p,fd,before});}
 function walk(dir,prefix) {
 const base=`/proc/self/fd/${dir}`,before=fs.fstatSync(dir,{bigint:true});
 const names=fs.readdirSync(base,{encoding:'buffer'}).map(utf8).sort(order);uniquePaths(names);
 if(prefix && !names.length) throw new ReleaseError('release-unexpected-content',prefix,'Empty directory');
 for(const name of names) { const p=`${base}/${name}`,rel=prefix?`${prefix}/${name}`:name;const pre=fs.lstatSync(p,{bigint:true});
 if(pre.isDirectory()) {const child=openDir(p);if(!stable(pre,fs.fstatSync(child,{bigint:true})))throw Error('Directory replaced');walk(child,rel);if(!stable(pre,fs.lstatSync(p,{bigint:true})))throw Error('Directory changed');continue;}
 if(!pre.isFile()||pre.nlink!==1n) throw new ReleaseError('release-changed-content',rel,'Not a single-link regular file');
 const mode=(Number(pre.mode)&0o7777).toString(8).padStart(4,'0');if(!['0644','0755'].includes(mode))throw new ReleaseError('release-changed-content',rel,'Unsupported mode');
 if(pre.size>BigInt(require('node:buffer').constants.MAX_LENGTH))throw Error('File exceeds safe buffer bound');
 const f=fs.openSync(p,C.O_RDONLY|C.O_NOFOLLOW|C.O_NONBLOCK);try { const opened=fs.fstatSync(f,{bigint:true});if(!stable(pre,opened))throw Error('File replaced');const bytes=fs.readFileSync(f);if(BigInt(bytes.length)!==pre.size||!stable(opened,fs.fstatSync(f,{bigint:true}))||!stable(opened,fs.lstatSync(p,{bigint:true})))throw Error('File changed');entries.push({path:rel,mode,bytes});checks.push({p,before:pre});}finally{fs.closeSync(f);}
 }
 if(!stable(before,fs.fstatSync(dir,{bigint:true}))||JSON.stringify(names)!==JSON.stringify(fs.readdirSync(base,{encoding:'buffer'}).map(utf8).sort(order)))throw Error('Directory changed');
 checks.push({fd:dir,before});
 }
 walk(fd,''); for(const c of checks)if(!stable(c.before,c.p?fs.lstatSync(c.p,{bigint:true}):fs.fstatSync(c.fd,{bigint:true})))throw Error('Snapshot changed');uniquePaths(entries.map(e=>e.path));return entries;
 }catch(e){if(e instanceof ReleaseError)throw e;throw new ReleaseError('source-unavailable',directory,e.message,3,'snapshot');}finally{for(const fd of handles.reverse())fs.closeSync(fd);}
}
module.exports={snapshotDirectory};

function readLocalFile(filename) {
 const handles=[];const checks=[];const absolute=path.resolve(filename);const parts=absolute.split('/').filter(Boolean);const basename=parts.pop();
 try {
 let fd=fs.openSync('/',C.O_RDONLY|C.O_DIRECTORY|C.O_NOFOLLOW);handles.push(fd);
 for(const part of parts){const p=`/proc/self/fd/${fd}/${part}`;const before=fs.lstatSync(p,{bigint:true});fd=fs.openSync(p,C.O_RDONLY|C.O_DIRECTORY|C.O_NOFOLLOW);handles.push(fd);if(!stable(before,fs.fstatSync(fd,{bigint:true})))throw Error('Parent replaced');checks.push({p,before});}
 uniquePaths([basename]);const p=`/proc/self/fd/${fd}/${basename}`;const before=fs.lstatSync(p,{bigint:true});
 if(!before.isFile()||before.nlink!==1n)throw Error('Expected single-link regular file');
 const mode=(Number(before.mode)&0o7777).toString(8).padStart(4,'0');if(!['0644','0755'].includes(mode))throw Error('Unsupported mode');if(before.size>BigInt(require('node:buffer').constants.MAX_LENGTH))throw Error('File exceeds safe buffer bound');
 const file=fs.openSync(p,C.O_RDONLY|C.O_NOFOLLOW|C.O_NONBLOCK);handles.push(file);if(!stable(before,fs.fstatSync(file,{bigint:true})))throw Error('File replaced');const bytes=fs.readFileSync(file);
 if(BigInt(bytes.length)!==before.size||!stable(before,fs.fstatSync(file,{bigint:true}))||!stable(before,fs.lstatSync(p,{bigint:true})))throw Error('File changed');
 for(const c of checks)if(!stable(c.before,fs.lstatSync(c.p,{bigint:true})))throw Error('Parent changed');return {path:basename,mode,bytes};
 }catch(e){throw new ReleaseError('source-unavailable',filename,e.message,3,'snapshot');}finally{for(const fd of handles.reverse())fs.closeSync(fd);}
}
module.exports.readLocalFile=readLocalFile;
