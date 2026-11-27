# C13 — Init (`lib/init.js`)

## Purpose
`installSkills` and `cleanStaleRemoteSubtree` write via C3 with project containment (FR-20, D-026). No new logic; size unchanged.

## installSkills

```
replace:  fs.copyFileSync(srcFile, dstFile)
with:     safeWrite.writeFileSync(dstFile, fs.readFileSync(srcFile), {newMode:0o600, containWithin: projectRoot})
keep: install-if-absent / --force logic, action strings, the ".." traversal throw.
dry-run: unchanged (no write call). Optionally call writeFileSync with dryRun:true to get containment errors in dry-run (FR-22a "dry-run runs the same check") — required: do it, so dry-run and real both throw outside-project.
mkdirSync(targetDir / dst): keep only in real runs (C3 mkdirs parents anyway; the explicit mkdir may be removed).
```
Note: `existsSync(dstFile)` on a dangling symlink returns false → treated as absent → writer throws `dangling-symlink` (exit 1). Intended (D-026 fixture).

## cleanStaleRemoteSubtree

```
compute(raw):
  if raw===null: none
  parsed = JSON.parse(raw) (throw → caught by existing best-effort catch, note action)
  if !(plain(parsed.unimatrix) and hasOwn(parsed.unimatrix,"remote")): return null
  delete parsed.unimatrix.remote
  return JSON.stringify(parsed, null, detectIndent(raw)) + "\n"      // indent reuse (D-022; was fixed 2)
res = safeWrite.updateFileSync(slPath, compute, {dryRun, containWithin: projectRoot})
if res.action !== "unchanged": push existing dry-run / real action strings
```
The existing try/catch stays best-effort for parse errors **but must rethrow `SafeWriteError`** (FR-31).

## Output
`printSummary` prints `actions` from `wire()`; wire now includes warnings in `actions` (FR-53) and also writes them to stderr. Check `printSummary`/caller for a separate warnings print loop and remove any duplicate. Top-level error handler: confirm it prints `unimatrix init failed: ${e.message}` and exits 1 for any thrown Error (covers SafeWriteError, symlink trust message).

## Key tests
- Skill file symlinked outside root → exit 1 `outside-project` (dry-run and real).
- `settings.local.json` with `unimatrix.remote` and tab indent → removed, tab kept.
- Existing init.test.js green.
