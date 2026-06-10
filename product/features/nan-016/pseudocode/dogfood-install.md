# Component 1 — `scripts/dogfood-install.sh` (Build + copy-install)

## Purpose

Build (`npm pack`) the in-repo TS hook client `packages/unimatrix` and clean-replace install a
frozen runtime tree to `~/.unimatrix/dogfood-client/` (overridable `--target`). Idempotent.
The F6 soak-reset point (AC-01). Tooling — MAY be loud / exit non-zero on failure (unlike hooks).
Covers FR-1, FR-2, FR-3, FR-4, NFR-1, NFR-7; addresses SR-01, SR-02, SR-03 / R-02, R-11, R-12.

ARCH-OQ-3: POSIX `sh` wrapping native `npm` + a tiny no-Node-contract file copy. No bashisms;
`set -eu` (loud error). No `mergeSettings` here (that is Component 2).

## Interface / invocation

```
scripts/dogfood-install.sh [--target <dir>]
  --target <dir>   install destination. DEFAULT: ${HOME}/.unimatrix/dogfood-client
  (no other args)
  exit 0  = installed + completeness/smoke asserts passed
  exit !=0 = loud failure (missing tool, pack fail, incomplete tree, smoke fail, guard reject)
```

## Initialization sequence

```
main(argv):
  set -eu                        # fail loud; no silent partial installs
  TARGET <- default ${HOME}/.unimatrix/dogfood-client
  parse argv:
    --target <dir> -> TARGET <- <dir>   (error if value missing)
    unknown flag   -> die("unknown arg", code 2)
  REPO   <- `git rev-parse --show-toplevel`   (die if not in a repo)
  PKGDIR <- REPO/packages/unimatrix
  assert PKGDIR/package.json exists           (die "client package not found")
  require_cmd npm ; require_cmd node ; require_cmd tar   # die loud if absent (R-05-adjacent tooling)
  TARGET <- expand_tilde_and_vars(TARGET)     # ~ and $HOME -> absolute

  guard_target TARGET                         # SHARED clean-replace safety guard (below)
```

## Clean-replace safety guard (SHARED invariant — OVERVIEW)

```
guard_target(TARGET):
  [ -n "$TARGET" ]                         || die "empty --target" 3
  RP_PARENT <- realpath -m "$(dirname "$TARGET")"
  BASE      <- basename "$TARGET"
  [ "$BASE" != "" -a "$BASE" != "." -a "$BASE" != ".." ] || die "unsafe target basename" 3
  RESOLVED  <- "$RP_PARENT/$BASE"
  HOME_RP   <- realpath -m "$HOME"
  # accept iff: under $HOME/.unimatrix/  OR  an explicit absolute target that is not a
  # forbidden ancestor.
  case "$RESOLVED" in
    "$HOME_RP"/.unimatrix/*) ok ;;          # default + temp installs under ~/.unimatrix/
    /*)                                      # explicit absolute --target
        for forbidden in "$HOME_RP" "/" "$HOME_RP/.unimatrix" "$REPO" ; do
          [ "$RESOLVED" = "$forbidden" ] && die "refusing to rm forbidden path: $RESOLVED" 3
        done
        is_ancestor_of "$RESOLVED" "$REPO" && die "target is ancestor of repo" 3
        ok ;;
    *) die "target must be absolute or under ~/.unimatrix/: $RESOLVED" 3 ;;
  esac
  # Harness ARCH-OQ-2 note: harness passes --target under os.tmpdir(); that is an explicit
  # absolute path and passes this guard, while the real ~/.unimatrix/dogfood-client is never
  # touched unless explicitly chosen.
```

## New functions (pseudocode bodies)

### `pack(PKGDIR, STAGING) -> TGZ`
```
pack:
  STAGING <- mktemp -d "${TMPDIR:-/tmp}/dogfood-install.XXXXXX"   # sibling-of-target staging
  # npm pack into STAGING; --pack-destination keeps the repo dir clean.
  ( cd "$PKGDIR" && npm pack --silent --pack-destination "$STAGING" ) || die "npm pack failed" 4
  # tarball name is dug-21-unimatrix-<version>.tgz — GLOB, never hardcode version (#4328).
  TGZ <- single match of "$STAGING"/dug-21-unimatrix-*.tgz
  [ -f "$TGZ" ] || die "tarball not produced" 4
  echo "$TGZ"
```

### `extract(TGZ, STAGING) -> EXTRACTED`
```
extract:
  EXTRACTED <- "$STAGING/extracted"
  mkdir -p "$EXTRACTED"
  # tarball root is package/ ; strip it so EXTRACTED/ is the client tree root.
  tar -xzf "$TGZ" -C "$EXTRACTED" --strip-components=1   # -> EXTRACTED/{bin,lib,skills,postinstall.js,protocols}
  # NO postinstall runs: extraction is a pure file copy. postinstall.js is copied INERT (FR-3).
  echo "$EXTRACTED"
```

### `clean_replace(EXTRACTED, TARGET)`  — staged + atomic mv (R-02)
```
clean_replace:
  PARENT <- dirname "$TARGET"
  mkdir -p "$PARENT"
  STAGED <- "$PARENT/.dogfood-client.staging.$$"      # sibling of TARGET, same filesystem -> atomic mv
  rm -rf "$STAGED"
  mv "$EXTRACTED" "$STAGED"                            # move extracted tree to sibling of target
  # Atomic swap: never leave a partially-extracted tree observable AT the target path.
  if [ -e "$TARGET" ]; then
     OLD <- "$PARENT/.dogfood-client.old.$$"
     rm -rf "$OLD"
     mv "$TARGET" "$OLD"                               # move prior install aside (clean-replace, not overlay)
  fi
  mv "$STAGED" "$TARGET"                               # atomic rename into place
  rm -rf "$OLD" 2>/dev/null || true                    # drop prior install AFTER successful swap
  # Result: TARGET is the OLD complete tree or the NEW complete tree, never a partial one.
  # Overlay residue impossible: prior install fully replaced (R-02 scenario 1).
```

### `assert_complete(TARGET)`  — completeness (FR-2, SR-01 / R-11)
```
assert_complete:
  for required in \
     lib/hook-client/index.js \
     lib/merge-settings.js \
     lib/hook-client/config.js ; do
       [ -f "$TARGET/$required" ] || die "incomplete install: missing $required" 5
  done
  # full files[] set present:
  for d in bin lib skills protocols ; do [ -d "$TARGET/$d" ] || die "missing dir $d" 5 ; done
  [ -f "$TARGET/postinstall.js" ] || die "missing postinstall.js (expected inert copy)" 5
  # full hook-client/*.js set present (count > 0; index.js + siblings):
  count <- number of *.js under "$TARGET/lib/hook-client"
  [ "$count" -ge 1 ] || die "no hook-client js files" 5
  # platform binary MUST be absent (optionalDependency, never bundled) — confirms client-only freeze.
  if find "$TARGET" -type f -perm -u+x -name 'unimatrix' | grep -q . ; then
     die "unexpected platform binary in frozen tree" 5
  fi
  # non-symlink entrypoint (C-6 structural anti-npm-link guarantee; Component 3 re-asserts):
  [ -L "$TARGET/lib/hook-client/index.js" ] && die "entrypoint is a symlink (npm link leak)" 5
  # postinstall inertness is structural: extraction never executes it. No ONNX model fetch.
```

### `smoke(TARGET)`  — fail-open smoke (SR-08, AC-01 verification step b)
```
smoke:
  # tooling smoke: the entrypoint runs and fail-opens with no daemon and empty stdin.
  node "$TARGET/lib/hook-client/index.js" SessionStart </dev/null >/dev/null 2>&1
  rc <- $?
  [ "$rc" -eq 0 ] || die "smoke failed: entrypoint exit $rc (expected fail-open 0)" 6
```

## State machine / main flow

```
main:
  init + parse + guard_target            (above)
  STAGING   <- (created in pack)
  trap 'rm -rf "$STAGING" "$PARENT"/.dogfood-client.old.$$ "$PARENT"/.dogfood-client.staging.$$' EXIT
  TGZ       <- pack(PKGDIR, STAGING)
  EXTRACTED <- extract(TGZ, STAGING)
  clean_replace(EXTRACTED, TARGET)
  assert_complete(TARGET)
  smoke(TARGET)
  log "installed dogfood client -> $TARGET"
  exit 0
```

## Data flow

- IN: repo working tree (`packages/unimatrix`), optional `--target`.
- OUT: `InstalledClientTree` at `TARGET` (the package/ files[] set, binary excluded; entrypoint
  `lib/hook-client/index.js`; merge API `lib/merge-settings.js`).
- Side effects: writes only under `TARGET`'s parent (staging + swap) and a `mktemp` dir; never
  writes the repo, never writes any `settings.json`, never runs `postinstall.js`.

## Error handling

| Condition | Behavior |
|-----------|----------|
| not in a git repo / package missing | die, exit !=0, actionable message |
| missing `npm`/`node`/`tar` | die loud (tooling, not a hook) |
| `--target` empty / unsafe / forbidden | guard die BEFORE any rm (exit 3) |
| `npm pack` failure / no tarball | exit 4 |
| incomplete tree / binary present / symlink entrypoint | exit 5 |
| smoke (entrypoint) non-zero | exit 6 |
| interrupted mid-run | trap cleans staging; TARGET only ever swapped atomically (old or new) |

All errors are LOUD non-zero — this is tooling. (Contrast C-7: the *emitted hook command*
fail-opens; this build script does not.)

## Idempotency (NFR-1 / SR-03 / R-02)

Clean-replace + dependency-free build (C-9) ⇒ second run yields a byte-identical tree (modulo
mtimes). Mutating or adding a stray file under `TARGET` then re-running restores the fresh tree
and removes the stray (no overlay residue — prior install moved fully aside before swap).

## Key test scenarios (hints for tester — Component test-plan owns the plan)

1. Run twice; assert mechanism is copy not symlink (entrypoint not a symlink); full files[]
   tree present; `lib/hook-client/index.js` present + runnable. (AC-01 a/b)
2. Mutate an installed file + add a stray file; re-install; assert byte-identical to fresh and
   stray GONE. (AC-01 c, NFR-1, R-02-1)
3. Inspect that install stages to a sibling + atomic `mv` (no extract-in-place at TARGET). (R-02-2)
4. Install onto a pre-populated TARGET; assert clean replacement + exit 0. (R-02-3)
5. Assert platform binary absent + postinstall inert (no ONNX fetch / host mutation). (R-11-2/3)
6. `--target` = `$HOME` / `""` / `/` -> guard rejects BEFORE any rm (exit 3). (security guard)
7. Assert TARGET is the fixed absolute path (or explicit `--target`), never a node-version
   prefix. (R-12-1)

## Gaps / flags

- None blocking. The `find ... -perm -u+x -name unimatrix` binary-absence check is a belt-and-
  suspenders assert; if the package ever legitimately ships a JS file named `unimatrix` it would
  need narrowing — current `files[]` has no such file (verified).
