# C12 — Install store (`lib/install-store.js`)

## Purpose
Every store file via C3 with enforced modes (D-025); per-file legs `created/updated/unchanged` (D-017, FR-26a); atomic package-dir swap (§3.8, D-020); symlinked `trust.json` refusal + TrustError rewrap (§3.7, D-021).

## Removed
Private `atomicWrite`, `tmpName`, local `renameWithRetry` and `sleepMs` (import `renameWithRetry`, `tempPath` from safe-write). **Delete only after the ported tests pass on C3 (FR-33).**

## installStore(o)

```
const sw = require("./safe-write.js")
binMode = platform === "win32" ? 0o600 : 0o700
legs = []
legFor = (file, action) => ({harness:"unimatrix", surface:"store", action, path:file})

... existing checks (shim path, node) unchanged ...
if !o.dryRun: mkdir0700(uh, bin, pkgDir); win32 lockdown as today
   // dry-run: no dirs (FR-62); C3 dryRun does not mkdir

// 2. package copy
pkgAction = isCompleteCopy(dest) ? "unchanged" : "created"
if !o.dryRun and pkgAction === "created": copyPackage(o, dest, platform, fault)
legs.push(legFor(dest, pkgAction))

// 3. shim, 4. siblings
for [file, data] in [[shim, render(...)], [bin/shim-entry.js, read(libDir/shim-entry.js)], [bin/trust.js, read(libDir/trust.js)]]:
  fault("sibling-"+basename) for siblings (keep existing stage names)
  r = sw.writeFileSync(file, data, {dryRun:o.dryRun, enforceMode:binMode, newMode:binMode, platform})
  legs.push(legFor(file, r.action))

// 5. install.json
fault("install-json")
r = sw.writeFileSync(installJson, JSON.stringify({version, packageRoot:dest, node:o.nodePath}),
                     {dryRun:o.dryRun, enforceMode:0o600, platform}); legs.push(...)

// 6. prune (real only) — unchanged except tmp detection (below)
// 7. root marker
r = writeRootMarker(o.home, o.projectRoot, o.dryRun)  -> returns {file, action}; legs.push
// 8. trust
tj = join(uh, "trust.json")
refuseSymlinkedTrustJson(tj)                              // before any trust write, both modes
set = trust.trustSet(...)
try
  if win32: trust.fullCheckWindows(...) (real only); a = trust.recordTrustIdentity(set, o.home) (real only)
            legs.push(legFor(tj, o.dryRun ? plannedTrustAction(set, o.home) : a))
  else: if !o.dryRun: trust.checkTrust(set, platform, {home:o.home})
catch e:
  if e instanceof trust.TrustError and /symlink not allowed/.test(e.reason||e.message) and e.path:
     throw new Error(`unimatrix: ${e.path} is a symlink (-> ${safeReadlink(e.path)}); trust files must be regular files. Remove it ('rm "${e.path}"') and re-run 'npx unimatrix init'`)
  throw e
return {legs, actions: legs.map(l => (o.dryRun ? "[dry-run] would write " : "wrote ") + l.path), shimPath, version, prunedVersions, rerecordedNode}
```
(Keep `actions` for existing callers; `wire.js` now uses `legs`.)

`plannedTrustAction(set, home)`: compute the record JSON exactly as `recordTrustIdentity` would and compare to current bytes → `unchanged` / `updated` / `created`. Expose from trust.js as `trustRecordFor(paths, home) -> string` used by both (keeps one serialisation). On POSIX no trust.json leg (install-time trust.json is win32 only, FR-20).

Dry-run on a fresh machine: C3 `resolveTarget`/`readState` on absent files work without dirs; `mkdir` only in real runs. Dry-run in a fresh HOME must create nothing (FR-62).

```
function refuseSymlinkedTrustJson(tj)
  try lst = fs.lstatSync(tj) catch: return
  if lst.isSymbolicLink():
     link = fs.readlinkSync(tj)
     throw new Error(`unimatrix: ${tj} is a symlink (-> ${link}); trust state is never written through a symlink. Remove it ('rm "${tj}"') and re-run 'npx unimatrix init'`)

function writeRootMarker(home, projectRoot, dryRun) -> {file, action}
  root = walkToProjectRoot(projectRoot); dir = join(home,".unimatrix",computeProjectHash(root)); file = join(dir,"root")
  if !dryRun: mkdir0700(dir)
  r = sw.writeFileSync(file, root+"\n", {dryRun, enforceMode:0o600})
  return {file, action:r.action}
```
Return-shape change: callers of the exported `writeRootMarker` (grep: wire/install-store tests) must use `.file`. Flagged Q7.

## copyPackage (§3.8)

```
function copyPackage(o, dest, platform, fault)
  tmp = join(dirname(dest), "." + basename(dest) + ".tmp-" + randomHex16)
  fs.mkdirSync(tmp)                          // non-recursive: EEXIST => exclusive (retry name ×5)
  try
    copy bin/lib/package.json (unchanged filter); fault("copy-mid"); copyBinaryPackage(...)
    if platform !== "win32": chmodTree(tmp)
    fd = open(join(tmp,".complete"),"wx",0o600); fsync(fd); close; try fsync(dir tmp) catch {}
    fault("before-pkg-rename")
    for i in 0..4:
      if isCompleteCopy(dest): break                    // concurrent init won
      try sw.renameWithRetry(tmp, dest, platform); return
      catch:
        if isCompleteCopy(dest): break
        if !exists(dest): continue
        aside = join(dirname(dest), "." + basename(dest) + ".tmp-" + randomHex16)
        try fs.renameSync(dest, aside); rmrf(aside) catch {}      // atomic move-aside then delete
    if !isCompleteCopy(dest): throw Error("unimatrix: could not install package copy")
    rmrf(tmp)
  catch e: rmrf(tmp); throw e
```
Only exempt `renameSync` outside core (FR-20 grep test whitelist: this function).

## prune
Tmp detection: `n.includes(".tmp-")` (catches new `.<ver>.tmp-<hex>` and old `<ver>.tmp-<pid>-x`). Remove the `.tmp-${pid}` exemption (names no longer carry pid); age check (`TMP_AGE_MS`) protects concurrent runs.

## Error handling
`SafeWriteError` propagates (exit 1). Symlinked trust.json → Error with canonical message (exit 1), link untouched. TrustError rewrap as above.

## Key tests
- Ported FR-33 tests (retry, modes, cleanup) green before deletion.
- D-025: pre-chmod install.json/root/bin/* to 0644/0755 → after init 0600/0700; legs `updated`; second run `unchanged`, mtimes equal.
- Second run: every store leg `unchanged` (incl. package copy).
- Fault between staging and rename → no `pkg/<ver>/`; tmp dir removed.
- Symlinked trust.json (dangling and live) → exit 1 exact message; link text unchanged.
- Symlinked `bin/trust.js` → rewrapped message.
- Dry-run fresh HOME → `~/.unimatrix` not created.
