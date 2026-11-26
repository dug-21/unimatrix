# trust (`lib/trust.js`) — new, JS

## Purpose
Ownership/permission trust check for shim, `install.json`, `redirect.json`, `trust.json`, resolved `packageRoot` (ADR-002, D-029, D-036; FR-12b/12c/12c1). Refuse loudly, never skip. The same file is copied by install-store to `~/.unimatrix/bin/trust.js` so `shim-entry.js` can run it BEFORE any `packageRoot` code is loaded (design point 1 in OVERVIEW). The file therefore must be dependency-free (Node builtins only) and must not `require` anything relative.

## Trust bootstrap (ADR-002, D-090 accepted)
The sibling `~/.unimatrix/bin/trust.js` is self-contained (Node builtins only, NO requires from the package dir). install-store refreshes it atomically (tmp+rename) on init/upgrade; shim-entry loads it before anything from packageRoot; it is a trust-set member.

## Exports
```
class TrustError extends Error { constructor(path, reason) ; this.path, this.reason; message = `unimatrix: trust check failed for ${path}: ${reason}; run 'npx unimatrix init'` }
checkTrust(paths: string[], platform, opts?: {home, uid?, spawn?}) -> void       // per-event entry point
trustSet(home, {installJson?, hasRedirect}) -> string[]                          // files/dirs to check
fullCheckWindows(paths, opts) -> void                                            // init-time / mismatch path (spawns icacls)
recordTrustIdentity(paths, home) -> void                                         // writes trust.json (Windows)
statIdentity(p) -> {size, mtimeMs, ino}
```

## trustSet
```
function trustSet(home, ctx):
  u = join(home, ".unimatrix")
  set = [u, join(u,"bin"), shimPath(platform), join(u,"bin","shim-entry.js"), join(u,"bin","trust.js")]
  add each of install.json, redirect.json (if exists), trust.json (if exists, win32 & posix)
  if ctx.installJson?.packageRoot: add packageRoot and its parent (pkg dir)
  if redirect has client: add that client root (directory) too
  return unique existing paths    // absent redirect is not an error; absent shim is (init failed)
```

## checkTrust (POSIX, every event)
```
function checkTrust(paths, platform, opts):
  if platform == "win32": return checkTrustWindowsPerEvent(paths, opts)
  euid = opts.uid ?? process.geteuid()
  for p in paths:
    st = fs.lstatSync(p)      // ENOENT -> TrustError(p, "missing")
    if st.isSymbolicLink(): throw TrustError(p, "symlink not allowed")
    if st.uid != euid: throw TrustError(p, "not owned by current user")
    if st.mode & 0o022: throw TrustError(p, "group/other writable")
    if p is ~/.unimatrix or ~/.unimatrix/bin or pkg dir:
       if st.mode & 0o077: throw TrustError(p, "directory mode must be 0700")
  // parent dir mode for files: covered because ~/.unimatrix and bin are members of the set
```
Cost target: ~4-6 `lstatSync` + JSON reads done by caller (ADR-005). Never spawns.

## Windows (D-036)
Per event, no process spawn on the matching path:
```
function checkTrustWindowsPerEvent(paths, opts):
  tj = readTrustJson(home)                // missing / unparseable / not plain object -> null
  if tj == null: return fullAndRewrite(paths)      // null-check BEFORE any use of tj
  tjPath = normalizeKey(join(~/.unimatrix, "trust.json"))
  for p in paths:
    if normalizeKey(p) == tjPath: continue         // trust.json is NOT compared by its own identity: recordTrustIdentity
                                                   // records identities BEFORE rewriting trust.json, so a self-entry could never match
    cur = statIdentity(p)                 // ENOENT -> TrustError(p,"missing")
    rec = tj[normalizeKey(p)]
    if p is ~/.unimatrix:                 // trust.json integrity is covered by its PARENT dir identity (ADR-002 #5900)
       // dir identity = {ino, mode, uid-equivalent}; size/mtime EXCLUDED because every legitimate trust.json
       // rewrite (tmp+rename) changes the dir mtime. Any add/replace of trust.json by someone else without
       // going through fullAndRewrite is not detectable by mtime here, so dir ino/mode mismatch OR missing rec -> full check.
       if !rec or rec.ino!=cur.ino or rec.mode!=cur.mode: return fullAndRewrite(paths)
       continue
    if !rec or rec.size!=cur.size or rec.mtimeMs!=cur.mtimeMs or rec.ino!=cur.ino: return fullAndRewrite(paths)
  // all matched: pass, zero process spawn
function fullAndRewrite(paths):
  fullCheckWindows(paths)                 // throws TrustError on failure
  recordTrustIdentity(paths, home)        // tmp (unique name) + rename; failure to write is NOT fatal (next event re-checks)
```
```
function fullCheckWindows(paths, opts):
  me = currentUserSid()                   // `whoami /user /fo csv /nh` via absolute %SystemRoot%\System32\whoami.exe (init-time only)
  for p in paths:
    sddl = run(%SystemRoot%\System32\icacls.exe, [p, "/save", tmpfile])   // absolute path, PATH-independent (R-22); parse SDDL, locale-independent
    owner = parse "O:<sid>"; if owner != me and owner not in {S-1-5-32-544 BA}: throw TrustError(p,"owner is not current user")
    for each allow ACE (A;...;rights;;;sid): if rightsGrantWrite(rights) and sid not in {me, S-1-5-18 (SY), S-1-5-32-544 (BA), S-1-3-0 (CO)}: throw TrustError(p, `extra writer ${sid}`)
    // rightsGrantWrite: hex mask or aliases (GA,GW,FA,FW,WD,AD,WO,WDAC,DE,DC) where mask & (0x2|0x4|0x40|0x10000|0x40000|0x80000|0x10000000|0x40000000) != 0
    if p in {~/.unimatrix, bin}: require SDDL protected (P flag in DACL "D:P") // inheritance disabled
```
`recordTrustIdentity(paths, home)`: build (skipping the `trust.json` path itself; `statIdentity` includes `mode`) `{normalizedPath: statIdentity(p)}` including the `~/.unimatrix` dir itself; write `trust.json` tmp+rename; if rename EPERM/EBUSY retry 3x with 10 ms sleep (Atomics.wait) then give up silently (logged to stderr only at init).
Key normalisation: forward slashes, lowercase drive letter.

## Error handling
All failures throw `TrustError`; callers (shim-entry, hook-route, mcp-stdio) print `error.message` to stderr, exit 1. Unexpected fs errors other than ENOENT are wrapped in TrustError with the errno code. Malformed `trust.json` triggers the full check (never trusted, never fatal by itself).

## Key test scenarios
- POSIX umask 002 init then check passes; group-writable shim/install.json/redirect/trust.json -> TrustError naming path; foreign owner (chown in container or mocked `opts.uid`); `~/.unimatrix` 0755 refused; symlinked redirect refused.
- Windows (fake `opts.spawn` returning canned SDDL): extra writer ACE refused; identity match -> zero spawn (spy); identity mismatch -> full check once, `trust.json` rewritten, next event no spawn; `trust.json` malformed/missing/world-writable -> full check; `icacls` invoked by absolute path; concurrent rewrite leaves valid JSON.
- `ino` changes after replace-with-same-size-and-mtime -> full check (R-30).
