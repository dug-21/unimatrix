# Component: Skills Installer — `lib/init.js` (`copySkills` → `installSkills`)

> ADR-004 (install-if-absent + `--force`, skills only). Modify `init.js` only (thin, ~+60 lines; stays < 800). No new module; no harness/wire logic added here.

## Purpose

Replace `copySkills`' blanket `fs.copyFileSync`-every-run overwrite with a per-file existence check. Default: install-if-absent (an edited installed skill survives byte-for-byte, AC-01). `--force`: overwrite Unimatrix-owned skills with shipped versions, foreign files never touched (AC-02). Skills only — protocols/agents excluded (C-13, SR-05).

## Public signature (Integration Surface — exact)

```
installSkills(projectRoot, { force: boolean, dryRun: boolean }) -> string[]   // returns action lines
```

`copySkills` is removed from the public surface and replaced by `installSkills`. Keep `copySkills` as a thin deprecated alias ONLY if `initRemote` still calls it — better: update both call sites (`init` Step 5 and `initRemote` Step 5) to `installSkills(root, { force, dryRun })`. Update `module.exports` to export `installSkills` (and drop or alias `copySkills`).

## Ownership model (ADR-004 §1)

"Unimatrix-owned" = present in the package's shipped `skills/` source tree (`__dirname/../skills`). `installSkills` only ever considers files that exist under that shipped source. Foreign files under `.claude/skills/` (skills the package does not ship) are NEVER read, written, or deleted — on ANY path, including `--force` (AC-02). This is enforced structurally: the loop iterates the SHIPPED source tree, never the destination tree.

## Algorithm

```
function installSkills(projectRoot, { force, dryRun }):
  actions = []
  targetDir = projectRoot/".claude/skills"
  sourceDir = __dirname/"../skills"

  if not exists(sourceDir):
    actions.push("No bundled skills found (skipped)")
    return actions

  if not dryRun:
    mkdirp(targetDir)

  skillDirs = readdir(sourceDir).filter(isDirectory).map(name)

  for skillDir in skillDirs:
    src = sourceDir/skillDir
    dst = targetDir/skillDir
    if not dryRun: mkdirp(dst)

    for file in readdir(src):
      if file contains "..": throw PathTraversalError(file)     # retained guard (ADR-004 §6)
      srcFile = src/file
      dstFile = dst/file
      if not isFile(statSync(srcFile)): continue                # only files, not subdirs (parity w/ copySkills)

      destExists = exists(dstFile)

      if destExists AND not force:
        # install-if-absent default: leave the existing (possibly edited) file byte-for-byte (AC-01).
        actions.push(line(dryRun, "Kept skill file (exists): " + skillDir + "/" + file))
        continue

      # write path: absent (install) OR force (overwrite Unimatrix-owned).
      verb = destExists ? "Overwrote (--force)" : "Installed"
      if dryRun:
        actions.push("[dry-run] Would " + lower(verb) + " skill file: " + skillDir + "/" + file)
      else:
        fs.copyFileSync(srcFile, dstFile)
        actions.push(verb + " skill file: " + skillDir + "/" + file)

  # Skills-only boundary line (SR-05): stated so "parity" is not misread as all-definitions.
  actions.push("Definition scope: skills only (protocols/agents not installed)")
  return actions
```

### Behavior matrix

| Dest state | `force=false` | `force=true` |
|------------|---------------|--------------|
| absent | Install (write) | Install (write) |
| exists, Unimatrix-owned | Kept (no write) — AC-01 | Overwrite with shipped — AC-02 |
| foreign file (not in shipped tree) | Never visited | Never visited — AC-02 |

## `--force` blast radius (C-12, SR-04, AC-02)

- `--force` changes ONLY skill file writes. It is **never** forwarded to `wire()` (enforced in `bin` / `init` — see cli-routing.md). Wiring stays always-additive; `--force` performs zero wiring re-assertion. The zero-wiring guarantee is asserted at the wiring surface, not here.
- Foreign skill directories/files remain untouched because the loop never enumerates the destination tree.

## Dry-run (AC-14, NFR-10)

Every branch pushes a `[dry-run]`-prefixed line and writes nothing when `dryRun`. The action set (which files install / keep / overwrite) is identical between dry-run and real run — same loop, same branch decisions, only the side effect and prefix differ.

## Error handling

- Path traversal in a shipped filename → throw (retained; this is a package-integrity defect, not consumer input — loud is correct).
- `installSkills` does not parse consumer JSON/TOML, so it has no malformed-input branch. fs errors on a specific file (e.g. permission) propagate as today (`copyFileSync` throws) — unchanged posture; init is the loud checkpoint.
- Ownership scoping means there is no path where a foreign file is deleted or overwritten, so no data-loss branch exists.

## Callers to update

- `init()` Step 5: `actions.push(...installSkills(projectRoot, { force: opts.force || false, dryRun }))`.
- `initRemote()` Step 5: same (remote mode DOES install skills — preserve that; ADR-004 does not change remote-installs-skills).
- Thread `force` from `bin` into `init`/`initRemote` options (see cli-routing.md). `wire` never receives `force`.

## Key test scenarios (hints)

- Fresh repo `init` → all shipped skills installed (first-run works, AC-01/NFR-06, R-16).
- Modified installed skill on disk + `init` (no force) → byte-diff of that skill = empty (AC-01).
- Modified skill + `init --force` → skill == shipped version; a foreign file in `.claude/skills/foreign/` byte-identical (AC-02).
- `--force` run → assert zero wiring-surface changes (asserted at wiring layer, cross-referenced here; SR-04).
- Dry-run → `[dry-run]` lines, zero filesystem changes, action set == real run (AC-14).
- Shipped filename containing `..` → throws (traversal guard).
- Help/output contains the skills-only boundary line (SR-05).
