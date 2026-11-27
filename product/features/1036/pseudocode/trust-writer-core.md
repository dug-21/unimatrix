# C2 — Hardened writer core (`lib/trust.js`, extended)

## Purpose
The single built-ins-only write primitive (ADR-003 #5962, ADR-005 #5955, D-021). Lives in `trust.js` so the installed `~/.unimatrix/bin/trust.js` copy (byte-identical, self-contained) can use it for hook-time `trust.json` writes. No second copy anywhere. `safe-write.js` (C3) is the public API; core is low-level.

Constraint: only `require("fs")`, `require("path")`, `require("crypto")`. `test/trust-selfcontained.test.js` unchanged and must pass.

## New exports
`hardenedWriteSync`, `renameWithRetry`, `tempPath`, `SafeWriteError`, `resolveTarget`, `checkContained`, `readState`, `sha256`, `trustRecordFor`, plus existing exports.

`SafeWriteError` is defined here (one class, §6a) and re-exported by `safe-write.js`.

## Functions

```
class SafeWriteError extends Error { constructor(code, file, msg) { super(msg); name="SafeWriteError"; this.code=code; this.file=file } }

function sleepMs(ms)  // existing Atomics.wait helper (kept)

function tempPath(dest) -> `${dirname(dest)}/.${basename(dest)}.tmp-${crypto.randomBytes(8).toString("hex")}`

function renameWithRetry(from, to, platform, retry={count:10, delayMs:25})
  max = platform === "win32" ? retry.count : 1
  for i in 1..: try fs.renameSync(from,to); return
                catch e: if i<max and e.code in {EPERM,EBUSY,EACCES}: sleepMs(retry.delayMs) else throw e

function resolveTarget(file) -> {target, isLink, linkText}
  // FR-22 / FR-23
  try lst = fs.lstatSync(file)
  catch ENOENT: return {target: realPathForm(file), isLink:false}     // S2/S3
  catch ELOOP:  throw SafeWriteError("not-regular", file, `${file}: symlink loop`)   // S6 — code choice Q5
  if lst.isSymbolicLink():
     linkText = fs.readlinkSync(file)
     try target = fs.realpathSync(file)
     catch ENOENT: throw SafeWriteError("dangling-symlink", file, `${file} is a dangling symlink (-> ${linkText}); not written`)
     catch ELOOP:  throw SafeWriteError("not-regular", file, `${file}: symlink loop`)
     st = fs.statSync(target); if !st.isFile(): throw SafeWriteError("not-regular", file, `${file} -> ${target} is not a regular file`)
     return {target, isLink:true, linkText}
  if !lst.isFile(): throw SafeWriteError("not-regular", file, `${file} is not a regular file`)   // S7
  // regular file, but parent may be a symlinked dir (S8):
  return {target: fs.realpathSync(file), isLink:false}

function realPathForm(p)   // same algorithm as C1 (duplicated by necessity: trust.js may not require lib modules). Keep ≤15 lines.

function checkContained(file, target, containWithin, platform)
  if !containWithin: return
  realRoot = fs.realpathSync(containWithin)
  n = s => platform==="win32" ? s.replace(/\\/g,"/").toLowerCase() : s
  if n(target) === n(realRoot) or n(target).startsWith(n(realRoot + path.sep)): return
  throw SafeWriteError("outside-project", file, `${file} resolves to ${target}, outside project root ${containWithin}; not written`)

function readState(target) -> {bytes: Buffer|null, hash: string|"absent", mode: number|null}
  try b = fs.readFileSync(target); st = fs.statSync(target)
      return {bytes:b, hash:sha256(b), mode: st.mode & 0o777}
  catch ENOENT: return {bytes:null, hash:"absent", mode:null}
  catch e: throw SafeWriteError("io", target, `${target}: ${e.message}`)

/**
 * hardenedWriteSync(file, data, opts)
 *   One attempt (C3 owns the retry loop). Writes `data` to the resolved target of
 *   `file` iff the target still matches `opts.expect` ({target, hash}).
 *   opts: {expect, mode, platform, retry, fault, isLink, linkText, containWithin}
 *   returns "written"; throws SafeWriteError("conflict") on CAS mismatch (caller retries).
 */
function hardenedWriteSync(file, data, opts)
  dir = dirname(opts.expect.target)
  fs.mkdirSync(dir, {recursive:true})          // FR-22 (callers create 0700 dirs first where required)
  tmp = null; fd = null
  try
    for k in 0..4:                               // FR-24 EEXIST regen ×5
      tmp = tempPath(opts.expect.target)
      try fd = fs.openSync(tmp, "wx", opts.mode); break
      catch EEXIST: tmp=null; if k===4 throw SafeWriteError("io", file, "could not create temp file")
      catch e: tmp=null; throw SafeWriteError("io", opts.expect.target, `${opts.expect.target}: ${e.message}`)  // S9
    fs.writeSync(fd, data)  (loop until all bytes written)
    if opts.platform !== "win32": fs.fchmodSync(fd, opts.mode)      // umask-proof
    fs.fsyncSync(fd); fs.closeSync(fd); fd = null
    opts.fault?.("before-rename")
    // CAS (FR-25) — re-resolve and re-read
    now = resolveTarget(file)                         // may throw dangling etc.
    checkContained(file, now.target, opts.containWithin, opts.platform)
    if now.target !== opts.expect.target or readState(now.target).hash !== opts.expect.hash:
       throw SafeWriteError("conflict", file, `${file} changed while writing`)
    renameWithRetry(tmp, now.target, opts.platform, opts.retry); tmp = null
    if opts.platform !== "win32": try { dfd=fs.openSync(dir,"r"); fs.fsyncSync(dfd); fs.closeSync(dfd) } catch {}  // FR-28
    return "written"
  catch e
    if fd !== null: try fs.closeSync(fd)
    if tmp !== null: try fs.unlinkSync(tmp)             // FR-30 / AC-10
    throw e instanceof SafeWriteError ? e : SafeWriteError("io", file, `${file}: ${e.message}`)
```

Exported helpers C3 also uses: `resolveTarget`, `checkContained`, `readState`, `sha256`.

### `recordTrustIdentity(paths, home)` rewrite
Body becomes:
```
rec = {...same identity computation...}
dest = join(udir(home), "trust.json")
try
  lst = lstat(dest) or null
  if lst and lst.isSymbolicLink(): return           // FR-20a hook-time: skip, never follow, non-fatal
  data = JSON.stringify(rec)
  cur = readState(dest)
  if cur.bytes equals data and cur.mode === 0o600: return  // unchanged
  if cur.bytes equals data: fs.chmodSync(dest, 0o600); return
  hardenedWriteSync(dest, data, {expect:{target:dest, hash:cur.hash}, mode:0o600, platform:process.platform, retry:{count:4, delayMs:10}})
catch _: swallow                                     // unchanged hook-time posture
```
Return `"created"|"updated"|"unchanged"` (new) so install-store (C12) can report its leg; existing callers ignore the return. Install-time path in C12 does the symlink refusal with the fix message *before* calling this.

## Error handling
All failures surface as `SafeWriteError{code, file}`. Temp always unlinked. `recordTrustIdentity` swallows.

## Key tests (port from trust.test.js / install-store.test.js first — FR-33)
- win32 rename retry: stubbed `fs.renameSync` throws EPERM ×3 then succeeds, `platform:"win32"`; posix: no retry.
- `fault("before-rename")` throwing → no `.*.tmp-*` left.
- Directory fsync open throws → write succeeds.
- `tempPath` pattern `/^\..+\.tmp-[0-9a-f]{16}$/`.
- trust-selfcontained: no non-built-in requires.
- `recordTrustIdentity` with `trust.json` symlink → no write, link intact, no throw.
