# install-store (`lib/install-store.js`) — new, JS

## Purpose
Writes the per-machine user-level footprint (ADR-001, D-030, D-031, D-036): stable package copy, shim + `shim-entry.js` + `trust.js`, `install.json` atomic swap, prune, root marker, explicit modes/ACLs, Windows `trust.json`. Used by wire-init for both local init and cloud attach (identical, D-040). Local and cloud both.

## Exports
```
installStore({home, platform, nodePath, sourcePackageRoot, projectRoot, dryRun}) -> {actions: string[], shimPath, version, prunedVersions: string[], rerecordedNode: boolean}
checkShimPathSafe(shimPath, platform) -> void         // throws UnsafeShimPathError (shared class from merge-settings? defined HERE and re-exported)
readInstallJson(home) -> InstallJson | null
writeRootMarker(home, projectRoot, dryRun)            // also exported for wire-init ordering
PRENODE_ENABLED = false                                // flip to true iff the ADR-005 not-a-project benchmark breaches on any OS (D-025)
```
(`UnsafeShimPathError` lives in install-store.js; merge-settings imports it. OVERVIEW lists it under merge-settings for the builder; single definition here.)

## Pseudocode
```
function installStore(o):
  uh = join(o.home,".unimatrix"); bin = join(uh,"bin"); pkgDir = join(uh,"pkg")
  shim = platform=="win32" ? join(bin,"unimatrix.cmd") : join(bin,"unimatrix")
  checkShimPathSafe(shim, platform)                                  // posix: no "'" ; win32: no "%" and no '"' -> throw, init exits 1 (FR-07g). also refuse nodePath with same chars
  version = JSON.parse(read(join(o.sourcePackageRoot,"package.json"))).version
  prev = readInstallJson(o.home)
  rerecorded = (prev == null) or prev.node != o.nodePath
  actions = []
  if dryRun: return list of "[dry-run] would write <file>" for every path below (parity with real run, FR-12d)

  1. mkdirp uh, bin, pkgDir; chmod 0700 each explicitly (independent of umask; umask 002 passes). win32: icacls /inheritance:r /grant:r <user>:(OI)(CI)F on uh and bin (via trust.js helper `lockdownWindows(path)`).
  2. dest = join(pkgDir, version)
     if !isCompleteCopy(dest):                                       // .complete marker file written last
        tmp = dest + ".tmp-" + process.pid
        rmrf(tmp); copyTree(o.sourcePackageRoot -> tmp, include: ["bin","lib","package.json"], exclude: node_modules, test, opencode-plugin)
        linux: copy platform binary package (resolve @dug-21/unimatrix-<plat>-<arch> from sourcePackageRoot; copy dir to tmp/node_modules/@dug-21/<name>) so resolve-runtime finds the binary + libonnxruntime from clientRoot; absent platform pkg (cloud-only linux) -> skip silently
        write tmp/.complete; rmrf(dest); rename(tmp, dest)           // killed mid-copy leaves only tmp; pointer still on previous version (R-24)
  3. render shim: template = read(lib/shim/unimatrix.sh | .cmd); substitute placeholders (see shim.md); if !PRENODE_ENABLED @@PRENODE_CALL@@ = ""
     atomicWrite(shim, text, mode 0o700)    // tmp in same dir + rename; windows .cmd uses CRLF
  4. atomicCopy(join(__dirname,"shim-entry.js") -> join(bin,"shim-entry.js"), 0o700 posix)
     atomicCopy(join(__dirname,"trust.js")      -> join(bin,"trust.js"), 0o700)
  5. install.json: tmp = join(uh,"install.json.tmp-"+pid); write {version, packageRoot: dest, node: o.nodePath} mode 0o600; renameWithRetry(tmp, join(uh,"install.json"))   // win32: retry EPERM/EBUSY up to 10x with 25 ms Atomics.wait back-off; concurrent inits last-writer-wins, both consistent
  6. prune: versions = readdir(pkgDir) filter dirs excluding ".tmp-"; keep {version, prev?.version}; rmrf the rest (best-effort; ignore EBUSY on win32)
     also rmrf any stale "*.tmp-*" older than the current pid (interrupted copies)
  7. writeRootMarker(o.home, o.projectRoot):                         // hash = computeProjectHash(walkToProjectRoot(projectRoot)); the marker value is that walked root
        mkdirp(uh/hash, 0700); atomicWrite(uh/hash/root, root + "\n", 0o600)    // one line, no CR
  8. trust: posix: checkTrust(trustSet(...), platform) as a self-check; failure -> throw TrustError (init exit 1)
            win32: fullCheckWindows(set); recordTrustIdentity(set, home)
  return {...}
```
`installStore` re-records node on every call (D-030) and returns `rerecordedNode`; wire-init rewrites the Windows fallback MCP entries when true (D-057).

## Data written (exact)
- `install.json` `{"version","packageRoot","node"}` (0600), `pkg/<version>/` tree + `.complete`, `bin/unimatrix[.cmd]`, `bin/shim-entry.js`, `bin/trust.js`, `<hash>/root`, Windows `trust.json`.
- Hook command string is unaffected by anything written here except the shim PATH (byte-fixed, D-028).

## Error handling
`UnsafeShimPathError` -> wire-init prints and exits 1 before any write. Copy failure: remove tmp, keep previous `install.json`, throw (init exit 1). Prune failures ignored. Windows rename contention after retries -> throw with clear message; pointer remains on previous version.

## Key test scenarios
- Modes 0700/0600 under umask 002; `install.json` never partially written (kill between tmp and rename); upgrade keeps current+previous, prunes older; interrupted copy leaves pointer on previous and no `.complete` dir; two concurrent runs produce one consistent pointer.
- Unsafe path refused (`'` posix; `%`/`"` win32) with nothing written.
- `root` marker content = one line, hash dir 0700; legacy dir without marker gets one on init.
- Dry-run action list equals real-run changed files (FR-12d).
- Hook string byte-identical before/after simulated upgrade (shim path unchanged).
