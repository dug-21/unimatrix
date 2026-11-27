# SPECIFICATION — 1036: Harden Per-Machine Harness Wiring Placement

Feature: 1036 (GH #1036). Inputs: `SCOPE.md` (approved, D-005), `DECISIONS.md` D-001..D-030 (D-011 supersedes D-004; later amendments as cited), `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-17), `product/research/ass-110/ass-110-findings.md`, `packages/unimatrix/lib/`.

## Objective

Keep nan-024's wiring destinations (D-001) and harden them. `unimatrix init` must write each harness's wiring to the file that harness actually reads under its config-dir override. It must remove every owned entry from abandoned or shadowing locations, and write every file through one symlink-safe, race-checked, verified writer. `unimatrix status` must make a lost Claude local-scope entry visible, and init must keep `.claude/settings.local.json` git-ignored through the per-repo exclude file, without touching any tracked file.

---

## Domain Model (Ubiquitous Language)

| Term | Definition |
|---|---|
| **Harness** | One of `claude-code`, `codex-cli`, `opencode`. |
| **Surface** | One wiring concern of a harness: `hooks`, `mcp` (OpenCode legs keep their existing `retrieval` surface name), plus the new `git-exclude` surface owned by `claude-code`. |
| **Main root (R)** | The project root that `wire()` receives. It comes from `detectProjectRoot` (init) or `walkToProjectRoot` (status). A linked worktree resolves to the main checkout, and a submodule whose `.git` file points into a superproject's `.git/modules/` resolves to the superproject root (existing `resolveGitFile` behaviour, unchanged). With `--project-dir`, R is `path.resolve(projectDir)`. |
| **Override variable** | `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`, `XDG_CONFIG_HOME` or `CODEX_HOME`. Init and status read these and never set them (R1). |
| **Valid override** | A non-empty string value for which `path.isAbsolute(value)` is true on the running platform and which contains no NUL byte. |
| **Resolved config dir / Destination file** | The file the harness reads, computed by the Config-Dir Resolver (FR-01..FR-06). This is the only file a destination write goes to. |
| **Chosen-by** | The variable that decided a destination (`CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`, `XDG_CONFIG_HOME`, `CODEX_HOME`), or `default`. |
| **Default location** | The path a harness uses when no override is set: `~/.claude.json`, `~/.claude/settings.json`, `~/.config/opencode/opencode.json`, `~/.codex/`. Being a default location does not make a path removable: only RT-03 and RT-10 are twins, and the rest are AP-1..AP-3 under an override (D-019, D-028). |
| **Twin** | An owned entry at a nan-024 legacy removal location, RT-03 (`~/.claude/settings.json` hooks) or RT-10 (`~/.config/opencode/opencode.json` `mcp.unimatrix`). It is a twin only while a valid override routes the destination elsewhere and the location is not the same file as the destination (D-007, D-019, FR-12). The default `~/.claude.json` is never a twin; under an override it is AP-3 (D-028). |
| **Alternate-profile location** | A lower-tier config location that is not a nan-024 legacy location (AP-1..AP-3, FR-10). It is treated as a separate live profile: init never removes from it, and status lists it (D-019). |
| **Same file** | Two paths name the same file under the rule in FR-12. |
| **Owned entry** | An entry recognised by `lib/hook-ownership.js`: `isUnimatrixCommand` for hooks, `isUnimatrixMcpEntry` for MCP, or `isLegacyOpencodePlugin` (new, FR-15) for `plugin[]`. Ownership is shape-based and uses no marker fields (D-040). |
| **Foreign entry** | Any entry that is not owned, including a non-owned value at a `unimatrix` key (D-065). |
| **Local-scope entry** | `projects[<key for R>].mcpServers.unimatrix` in the resolved `.claude.json`. The key comes from `resolveProjectKey`. |
| **User-scope entry** | The top-level `mcpServers.unimatrix` in the resolved `.claude.json` (G2). |
| **Leg / Wire manifest** | One `{harness, surface, action, path, reason?, entry?, command?}` record per write, removal or skip. `wire()` returns the manifest, and init renders one action line per leg. |
| **Removal Target Table** | The single list of removal locations in FR-10. Code, dry-run and tests all derive from it (SR-08). |
| **Hardened writer** | The single module through which every file Unimatrix writes is written (D-010, FR-20..FR-33). |
| **CAS** | Compare-and-swap. The writer re-reads the real target right before rename and proceeds only if its SHA-256 still equals the hash of the bytes the caller read. |
| **Verify** | After rename, the writer re-reads the target and the caller confirms the intended entry is present (and the removed entry is absent). |
| **Exclude file** | The per-repo ignore file at the path printed by `git rev-parse --git-path info/exclude`, run in R (D-011). |
| **Exclude entry** | The single line init appends to the exclude file (FR-44). |
| **Dry-run parity** | `--dry-run` produces the same legs a real run would produce from the same starting state, and writes nothing (FR-60..FR-63). |

---

## Functional Requirements

### A. Config-Dir Resolver (Scope In-2; G1; SR-01, SR-09, SR-13, SR-17)

- **FR-01** A new module (for example `lib/config-dirs.js`) is the only source of harness config paths. `wire.js`, `claude-json.js`, `merge-settings.js` (user hook removal), `opencode-install.js`, `codex-install.js`, `status.js`, including `mcpNode` and the hook-state lookup, all call it. Every private path computation is deleted, including `status.js`'s `codexHome` fallback, the hardcoded `path.join(home, ".claude.json")` and `path.join(home, ".config", "opencode", ...)` sites, and the hardcoded `~/.claude/settings.json` in `dispatchClaude`. Test: a grep-style test asserts that no lib file other than the resolver joins `".claude.json"`, `".config"/"opencode"` or `".codex"` onto a home path.
- **FR-02** The resolver takes `(env, home, platform)` and reads only the `env` object it is given, never `process.env` directly. `wire()` passes `o.env || process.env`, and `status` passes `process.env`.
- **FR-03 Claude.** If `CLAUDE_CONFIG_DIR` is a valid override `<d>`:
  - `claudeJson = <d>/.claude.json`;
  - `claudeUserSettings = <d>/settings.json`;
  - chosen-by = `CLAUDE_CONFIG_DIR`.

  Otherwise: `claudeJson = <home>/.claude.json`, `claudeUserSettings = <home>/.claude/settings.json`, chosen-by = `default`. This layout was measured with Claude Code 2.1.332 and recorded in ADR-001 (#5937): with the override, Claude reads hooks only from `<d>/settings.json` (SR-01 discharged).
- **FR-04 OpenCode.** The resolver takes the first match from this list:
  1. if `OPENCODE_CONFIG_DIR` is valid: `<OPENCODE_CONFIG_DIR>/opencode.json`, chosen-by `OPENCODE_CONFIG_DIR`;
  2. if `XDG_CONFIG_HOME` is valid: `<XDG_CONFIG_HOME>/opencode/opencode.json`, chosen-by `XDG_CONFIG_HOME`;
  3. otherwise `<home>/.config/opencode/opencode.json`, chosen-by `default`.

  The same order applies on every platform.
- **FR-05 Codex.** `codexHome(env, home)` keeps its current semantics, including the D-085 warning text. The resolver re-exports it, so `status` and `wire` call one function.
- **FR-06 Edge values.**
  - An unset variable, or one set to the empty string `""`, is treated as unset, with no warning.
  - Any other value that is not a valid override is ignored, and the resolver emits exactly one warning naming the variable: `unimatrix: <VAR> is not an absolute path ("<value>"); ignoring it and using <fallback>`. This covers relative paths, whitespace-only values, values starting with a literal `~`, and values containing NUL. For OpenCode, the fallback named is the next step in FR-04.
  - A trailing path separator on a valid value is removed before joining.
  - Both init (on stderr, and in the actions list) and `status` print the warning (SR-17).
  - Resolution with an ignored variable gives the same destinations as if that variable were unset. Test: one fixture per variable × {relative, `""`, `"  "`, `~/x`}.
- **FR-07** Resolution happens once per `wire()` call, before any write. All legs of a run use the same resolved paths.

### B. Removal Set (Scope In-3; G2, G3; D-007; SR-08, SR-11, SR-12)

- **FR-10 Removal Target Table.** Init removes owned entries only from these locations. Foreign entries stay in place. If a foreign value sits at a `unimatrix` key, init leaves it and emits a warning naming the file and key (D-065).

  | ID | Harness | Location | Key / entry | Condition | Status |
  |---|---|---|---|---|---|
  | RT-01 | claude-code | `<R>/.claude/settings.json` | owned hook commands | always | existing |
  | RT-02 | claude-code | resolved `claudeUserSettings` | owned hook commands | always | existing; path now resolved (D-026) |
  | RT-03 | claude-code | `<home>/.claude/settings.json` | owned hook commands | twin: `CLAUDE_CONFIG_DIR` valid and not same file as RT-02 | new; resolved: kept (D-030) |
  | RT-04 | claude-code | resolved `claudeJson` | top-level `mcpServers.unimatrix` (user scope) | always (G2) | new |
  | RT-05 | — | retired (D-028) | — | Under a `CLAUDE_CONFIG_DIR` override, the default `~/.claude.json` is never modified. It is AP-3 | — |
  | RT-06 | claude-code | `<R>/.mcp.json` | owned `mcpServers.*` | always | existing |
  | RT-07 | codex-cli | `<R>/.codex/hooks.json`, `<R>/.codex/config.toml` `[mcp_servers]`, `$CODEX_HOME/config.toml` `[hooks]` | owned entries | always | existing, unchanged |
  | RT-08 | opencode | `<R>/opencode.json` | `mcp.unimatrix` (owned) | always | existing |
  | RT-09 | opencode | `<R>/opencode.json` | legacy plugin elements in `plugin[]` (FR-15) | always (G3) | new |
  | RT-10 | opencode | `<home>/.config/opencode/opencode.json` | `mcp.unimatrix` (owned) | twin: chosen-by ≠ `default` and not same file as the resolved OpenCode file | new |

  RT-01..RT-04 and RT-06 run whenever the Claude leg runs. RT-08..RT-10 run whenever `removeProjectFileMcp`/`dispatchOpencode` run today. RT-10 runs only when the OpenCode leg runs. Only the locations listed here are removed.

  "Default-location twins" (Scope In-3) means the nan-024 legacy write locations only (D-019): RT-03 (`~/.claude/settings.json` hooks) and RT-10 (`~/.config/opencode` only). The **alternate-profile locations** listed below are never read-modified-written by init, so no removal leg exists for them. `status` lists any owned entry found there (FR-38):
  - AP-1: `$XDG_CONFIG_HOME/opencode/opencode.json`, when `OPENCODE_CONFIG_DIR` wins and the two are not the same file;
  - AP-2: `<home>/.codex/{hooks.json,config.toml}`, when `CODEX_HOME` is valid and not the same dir;
  - AP-3: `<home>/.claude.json`, both `projects[<key for R>].mcpServers.unimatrix` and top-level `mcpServers.unimatrix`, when `CLAUDE_CONFIG_DIR` is valid and not the same file (D-019, D-028). Init never reads it for writing, never rewrites it, and never removes from it. AP-3 keys use `resolveProjectKey`.

  The legacy plugin shim in `.opencode/plugins/` and the `.opencode/package.json` dependency are not removed (D-014).
- **FR-11 Write-before-remove (SR-11).** Order within each harness:
  1. destination write;
  2. verify;
  3. removals.

  A twin removal (RT-03, RT-10) and the G2 removal (RT-04) run only if the same surface's destination leg ended `created`, `updated` or `unchanged` with verify passed. If the destination leg is `skipped-*` or fails, the twin and G2 entries for that surface stay in place, each gated removal emits a `skipped-dest-unwired` leg (in both modes; ADR-004), and the harness keeps its old wiring. RT-04 and the local-scope write touch the same file, so they may be applied in one hardened write. They still produce two legs: the mcp leg, and a `removed` leg with reason `owned user-scope MCP entry removed`.
- **FR-12 Same-file rule (D-007).** For destination path T and default path D:
  - If both exist: they are the same file iff `fs.statSync` gives equal `(dev, ino)`, with symlinks followed.
  - If neither exists, or only T exists: there is nothing to remove, and no leg is emitted.
  - If only D exists: they are the same file iff `realpath(D)` equals the real-path form of T. The real-path form of T is the realpath of T's deepest existing ancestor joined with the remaining components. On `win32`, compare after `normalizeKey`-style normalisation (case-insensitive drive letter, `/` separators).

  A same-file D is never a twin: it gets no removal and no leg. Required fixtures:
  - `CLAUDE_CONFIG_DIR=<home>`;
  - `CLAUDE_CONFIG_DIR=<home>/` (trailing slash);
  - `CLAUDE_CONFIG_DIR` set to a symlink pointing at `<home>`;
  - `XDG_CONFIG_HOME=<home>/.config`;
  - `OPENCODE_CONFIG_DIR=<home>/.config/opencode`.

  In each, the destination entry survives, no `removed` leg exists for the default path, and `status` does not list the default path as `other profile`.
- **FR-13 Migration intent for OpenCode.** An owned `mcp.unimatrix` at the RT-10 twin location counts as an existing opt-in when the writer applies the `skipped-intent` rule, so the resolved file is written and the twin then removed. Without this, AC-2's migration would leave OpenCode dark (G1). Decided in D-015.
- **FR-14 Parse failure.**
  - An unparseable resolved `.claude.json` aborts that leg untouched and makes init exit 1 naming the file (nan-024 FR-07j). Init does not read an AP-3 `.claude.json`. `status` reports one it cannot parse as `other profile: <file> unreadable (not valid JSON)`.
  - An unparseable resolved or twin (RT-10) user `opencode.json`, for example JSONC with comments, gives a `skipped-malformed` leg with the file unchanged, a stderr warning naming the file, and exit 0 (D-016 as amended by D-024). `status` names the resolved file as leaving OpenCode unwired (FR-38).
  - An unparseable project `opencode.json` during RT-08/RT-09 keeps its existing policy: throw, exit 1, file unchanged (D-029).
  - Codex `hooks.json`/`config.toml` keep their existing `skipped-malformed` + warning posture.
- **FR-15 Legacy plugin ownership (SR-12).** `hook-ownership.js` gains `isLegacyOpencodePlugin(el)`. It returns true only for a string `el` that equals `@dug-21/unimatrix-opencode-plugin`, or that equals that name followed by `@` and a non-empty version spec with no `/` (for example `@1.2.3`, `@latest`, `@^0.4`). Every other value returns false. Negative fixtures:
  - `@dug-21/unimatrix-opencode-plugin-extra`
  - `@other/unimatrix-opencode-plugin`
  - `unimatrix-opencode-plugin`
  - `file:./x/unimatrix-opencode-plugin`
  - `@dug-21/unimatrix-opencode-plugin@`
  - non-string elements

  RT-09 removes only matching elements and keeps every other element in its original order. If `plugin[]` becomes empty, the `plugin` key is kept as `[]`. If `plugin` is absent or not an array, there is no change and no leg.
- **FR-16 "Byte-for-byte" (SR-05, D-022).**
  - **Untouched files:** a file with no owned change is never rewritten. Bytes and mtime are unchanged.
  - **JSON, rewritten:** every foreign value deep-equals its prior parsed value, and every object keeps its key order. The file is re-serialised with the indentation detected from the existing file (`detectIndent`: tab or N spaces; 2 for a new file). Trailing newline is kept.
  - **TOML (Codex):** every byte outside the owned table being written or removed is identical (the existing surgical `codex-toml.js` edit).
  - A JSON file with comments is unparseable, so FR-14 applies and it is never rewritten.
  - Tests: deep-equal plus key-order assertions on JSON fixtures with tab, 4-space and 2-space indents. Byte-equality outside the owned table on TOML fixtures. Bytes and mtime on untouched-file fixtures.
- **FR-17 Uniqueness (AC-6).** After any init, across every resolved location and every RT location (AP-1..AP-3 are separate profiles and are excluded, D-019, D-028), at most one owned entry exists per Claude hook event, per Codex hook event, and per harness MCP key. For Claude MCP, only the local-scope entry in the resolved `.claude.json` exists: no user-scope entry in that file, and no `.mcp.json` entry. An entry in an AP-3 default file belongs to a separate profile and is not counted.

### C. Hardened Writer (Scope In-4; D-010, D-013; #1035 F2 + temp naming; SR-02, SR-03, SR-04)

- **FR-20 Coverage (D-010 as read by D-020).** Every file that `init`, `init --bundle` or `wire` writes goes through the hardened writer:
  - `.claude.json`;
  - user and project `settings.json`, `settings.local.json` (including the `cleanStaleRemoteSubtree` rewrite in `init.js`);
  - `.mcp.json`;
  - project and user `opencode.json`;
  - `.codex/*` and `$CODEX_HOME/{hooks.json,config.toml}`;
  - the exclude file;
  - `.claude/skills/*` (`installSkills`, today `copyFileSync`);
  - `~/.unimatrix/bin/*`: shim, `shim-entry.js`, `trust.js`;
  - `~/.unimatrix/install.json`, the `<hash>/root` marker, and install-time `trust.json` (win32 `recordTrustIdentity` called from `installStore`);
  - `~/.unimatrix/<hash>/remote.json` (`credstore.write`; credential).

  **Excluded (D-020):**
  - Hook-time runtime state written by the hook client and launcher: `last-hook.json`, queue, latches, and hook-time `trust.json` rewrites. These may use the core (D-021) but are not required to and keep their non-fatal posture.
  - The package-copy swap (`pkg/<ver>/`). It must stay a fully staged directory published by one atomic directory rename (`renameWithRetry`), and no file is ever written into a live version directory. Test: a fault injected between staging and rename leaves no partial `pkg/<ver>/`.

  **Placement (D-021, ADR-005):** the single writer core lives in `lib/trust.js`, which stays built-ins-only and byte-identical to its installed copy. `lib/safe-write.js` is the API that every other installer module imports. There is no second copy.

  `lib/atomic-write.js` is deleted. The `atomicWriteText` re-export in `claude-json.js` is removed, and so are the private tmp+rename functions in `install-store.js` (`atomicWrite`, `tmpName`) and `trust.js` (the `recordTrustIdentity` body). Test: a grep-style test finds no file `renameSync`/`writeFileSync`/`copyFileSync` on an FR-20 path outside the core and API. The package-dir `renameWithRetry` is the only exempt rename.
- **FR-20a Symlinked `trust.json` (D-021).** If `~/.unimatrix/trust.json` is a symlink (dangling or not), the install-time write does not follow it. Init exits 1 with exactly this message: `unimatrix: <path> is a symlink (-> <link text>); trust state is never written through a symlink. Remove it ('rm "<path>"') and re-run 'npx unimatrix init'`. The link and its target are unchanged. A hook-time write in the same situation skips the write without following the link and stays non-fatal. This is the only FR-20 file where a symlink is refused instead of followed. Separately, a symlinked trust-set file (for example `bin/*`) that the writer preserved is caught by the existing trust check. Its `TrustError` is rewrapped with the message `unimatrix: <path> is a symlink (-> <link text>); trust files must be regular files. Remove it ('rm "<path>"') and re-run 'npx unimatrix init'`, and init exits 1.
- **FR-21 API is synchronous (D-013).** The read → modify → CAS → rename sequence runs in one synchronous call stack with no `await`, callback or timer between the read and the rename. The caller supplies the modify step as a synchronous function `(currentBytes|null) => newBytes | UNCHANGED`. The windows retry sleep (`Atomics.wait`) is synchronous and allowed.
- **FR-22 Target resolution.**
  - The writer calls `lstat` on the given path. If it is a symlink, the real target is `fs.realpathSync(path)`, which resolves absolute, relative and chained links.
  - If the path does not exist, the real target is the real-path form of the path (realpath of the deepest existing ancestor plus the remaining components). Missing parent directories are created (`mkdir -p`) before the temp file is created. Callers that need `0700` dirs (`~/.unimatrix`) keep creating them first.
- **FR-22a Project-file containment (D-026).** A project file is any FR-20 path built under R: `.claude/settings.local.json`, `.claude/settings.json`, `.mcp.json`, `opencode.json`, `.codex/*`, `.claude/skills/*`. For these:
  - The writer first computes `realRoot = fs.realpathSync(R)`.
  - It then resolves the real target (FR-22).
  - The write proceeds only if the real target equals `realRoot` or starts with `realRoot + path.sep`. On win32, the comparison is case-insensitive with `/` normalised.
  - Otherwise the writer throws `SafeWriteError{code:"outside-project"}` (canonical in ARCHITECTURE §6a), with a message naming the link, its target and R. Init exits 1, and nothing is written.
  - The check is repeated with the CAS re-resolve (FR-25).

  The exclude file is git metadata reported by git, not repo content, so it is not a project file. User and global files follow AC-3 unchanged.

  Required fixtures:
  - a link to a file inside R: followed;
  - a link to a file outside R: exit 1, link and target unchanged;
  - R itself reached through a symlink (macOS-style `/var` → `/private/var`, simulated by a symlinked temp root) with an in-root target: followed;
  - a dangling project link: `dangling-symlink`, exit 1;
  - a project link to a directory: `not-regular`, exit 1;
  - a link whose 2-hop chain leaves and re-enters R: decided by the final real target only.
- **FR-23 Symlink cases (SR-02).** Each case has a test. Every writer error is the single class `SafeWriteError extends Error {file, code}`, with `code` ∈ `"dangling-symlink"` | `"not-regular"` | `"outside-project"` | `"conflict"` | `"verify-failed"` | `"io"`. There are no subclasses. The test seam is `opts.fault(stage)` with `stage` ∈ `"after-read"` | `"before-rename"` | `"after-rename"`.

  | Case | Required behaviour |
  |---|---|
  | S1 regular file | Replaced in place; existing mode kept |
  | S2 absent, parent exists | Created with the new-file mode |
  | S3 absent, parent absent | Parents created, then file created |
  | S4 symlink → file (absolute, relative, chained 2+ hops) | Real target receives the bytes; every link in the chain is still a symlink with unchanged `readlink` text |
  | S5 dangling symlink | Throws `SafeWriteError{code:"dangling-symlink"}`, with a message naming the link and its target; link untouched; nothing created |
  | S6 symlink loop (`ELOOP`) | Throws naming the path; nothing written |
  | S7 path is a directory or another non-regular file | Throws naming the path; nothing written |
  | S8 parent directory is a symlink | Temp and rename happen in the real directory |
  | S9 real target's directory not writable (link dir writable) | Throws naming the real target; no temp remains |
  | S10 link retargeted between resolve and rename | Detected by re-resolving before rename (FR-25); treated as a CAS conflict |
  | S11 win32 | Same semantics where `realpathSync` resolves the link; rename retry per FR-27 |
  | S12 hard-linked target | Rename creates a new inode, so the hard link is not preserved. Documented, not tested as preserved |

- **FR-24 Temp file.**
  - Location: the real target's directory.
  - Name: `.<basename>.tmp-<16hex>` (ADR-003), where `<16hex>` is 16 hex characters from `crypto.randomBytes(8)`.
  - Open: `fs.openSync(tmp, "wx", mode)` (`O_CREAT|O_EXCL|O_WRONLY`). On `EEXIST`, regenerate the name, up to 5 tries, then throw.
  - Write all bytes, `fchmod` to the final mode (umask-proof), `fsyncSync(fd)`, then close.
  - Final mode (D-023 as amended by D-025):
    - **Enforced-mode files.** These get their mode re-applied on every write, whether new or existing (`opts.mode`): `~/.unimatrix/<hash>/remote.json`, `~/.unimatrix/install.json`, `~/.unimatrix/trust.json`, the `<hash>/root` marker (all `0o600`), and `~/.unimatrix/bin/*` (`0o700` on POSIX; win32 `0o600`, as today). `credstore.write` keeps forcing `0o600`.
    - **Every other file.** An existing file keeps its current `mode & 0o777`, read from the real target. A new file gets `0o600`: harness config, project files, `.claude/skills/*`, the exclude file.
    - Unchanged short-circuit (FR-26): an enforced-mode file whose bytes are equal but whose mode differs is not "unchanged". The writer re-applies the mode with `chmod` on the real target and reports `updated`. No temp file and no rename are needed. A second run is then `unchanged` (AC-5). This also stops the hook-time trust-check drift loop that a loose mode on trust inputs would cause.
    - Callers pass `newMode` (new files) or `mode` (enforced) explicitly. The writer default for new files is `0o600`.
- **FR-25 CAS.**
  - The caller's read records `sha256(currentBytes)`, or `absent`, together with the resolved real target.
  - Immediately before `rename`, the writer re-resolves the real target (FR-22) and re-reads it.
  - If the real target changed, or the hash or `absent` state differs, the writer removes the temp, re-reads and re-invokes the modify function.
  - Total attempts: 3, including the first. When attempts are exhausted, it throws `SafeWriteError{code:"conflict", file}`.
- **FR-26a Store legs (D-017).** `installStore` returns one leg per FR-20 store file, plus one for the package copy. Each leg reports `created`, `updated` or `unchanged` from the writer result instead of the fixed `written`. The package-copy leg reports `unchanged` when `isCompleteCopy(dest)` holds. Dry-run reports the same actions (FR-61). Install-time `trust.json` (win32) is `unchanged` when the recorded identities are byte-equal.
- **FR-26 Unchanged short-circuit.** If the modify function returns `UNCHANGED`, or returns bytes equal to the current bytes (and, for an enforced-mode file, the mode already matches, FR-24), the writer creates no temp, calls no rename, and leaves the mtime unchanged. The caller reports `unchanged`.
- **FR-27 Rename.** `renameSync(tmp, realTarget)`. On `win32` only, retry on `EPERM`, `EBUSY` or `EACCES` up to 10 attempts with a 25 ms synchronous sleep (the values in today's `install-store.js` `renameWithRetry`). On other platforms there is no retry.
- **FR-28 Directory fsync (SR-04).** On POSIX, after rename, the writer opens the real target's directory read-only and calls `fsyncSync`. Any error is ignored. On `win32` this step is skipped. A unit test injects an `fs` whose directory open throws and asserts that the write still succeeds.
- **FR-29 Verify.**
  - After rename, the writer re-reads the real target and checks that `sha256(read) === sha256(written)`.
  - It also checks that the original path still resolves to the same real target. If the original was a symlink, `lstat` must still report a symlink.
  - Fixpoint check (ADR-003): re-running the caller's mutate on the read-back bytes must return "no change". This means the entry is present and every removed entry is absent.
  - A verify failure counts as an attempt and triggers a re-read and recompute (FR-25 loop). When attempts are exhausted, it throws `SafeWriteError{code:"verify-failed", file}`.
- **FR-30 Cleanup (AC-10).** On every error path (open, write, fsync, CAS conflict, rename failure, verify failure, thrown modify function), the temp file is unlinked before the writer returns or throws. Test: after each injected failure, the target directory contains no `.*.tmp-*` file. Temp files from a crashed process (SIGKILL) are out of scope.
- **FR-31 Error propagation (ADR-003).**
  - Every `SafeWriteError` (`dangling-symlink`, `not-regular`, `outside-project`, `conflict`, `verify-failed`, `io`) propagates to `init`/`wire`. This includes errors raised inside the Codex and OpenCode writers, which rethrow it instead of turning it into a `skipped*` leg. Init then exits 1 with `unimatrix init failed: <message naming the file>`.
  - Parse failures keep the FR-14 posture, which is not a writer error.
  - Hook-time writes (excluded by D-020) swallow writer errors, as today.
  - Because an error ends the run before any later removal step (FR-11), a destination is never left without wiring because of a removal.
- **FR-32 Zero dependencies.** Only Node built-ins are used: `fs`, `path` and `crypto` in the writer, plus `child_process` for the git calls in the exclude step (FR-41). `test/check-zero-deps.js` passes.
- **FR-33 Migration before deletion (SR-03).** Existing `install-store.test.js` and `trust.test.js` tests for retry, mode (`0o600`/`0o700`) and temp cleanup, plus `credstore` mode tests, keep running unchanged and pass against the new writer before the private writers are deleted.

### D. `status` (Scope In-2, In-5; SR-09, SR-16, SR-17)

- **FR-34** `status` resolves paths only through the Config-Dir Resolver (FR-01). After the existing output, the text output adds these lines in this order:
  ```
  config claude-code mcp: <abs claudeJson> (<source>)
  config claude-code user hooks: <abs userSettings> (<source>)
  config codex-cli: <abs hooksJson>, <abs configToml> (<source>)
  config opencode mcp: <abs configJson> (<source>)
  ```
  `<source>` is the chosen-by value. For a source other than `default`, the parenthesis reads `<VAR>=<value>`. Each FR-06 warning is printed on its own line, prefixed `warning: `.
- **FR-35 Claude local-scope line.** Exactly one line:
  - `mcp claude-code: n/a`: the cwd is not a Unimatrix project (no store or credential).
  - `mcp claude-code: wired (<abs claudeJson>)`: the local-scope entry for R exists and is owned.
  - `mcp claude-code: MISSING projects["<key>"].mcpServers.unimatrix in <abs claudeJson>; run 'npx unimatrix init'`: the file is absent, the `projects` key for R is absent (for example after `claude purge`), or the entry is absent.
  - `mcp claude-code: foreign entry at projects["<key>"].mcpServers.unimatrix in <abs claudeJson>; not managed by Unimatrix`: the entry is not owned.
  - `mcp claude-code: unreadable (<abs claudeJson> is not valid JSON)`: the file cannot be parsed.

  `<key>` is `resolveProjectKey(projects, R, platform)`. Only the resolved file is consulted. An entry at a twin or default path never makes the line read `wired` (SR-16).
- **FR-36 JSON output.** `status --json` adds:
  - `configDirs: {claude: {claudeJson, userSettings, source}, opencode: {configJson, source}, codex: {hooksJson, configToml, source}, warnings: string[]}`;
  - `claudeMcp: {status, file, key}`, where `status` ∈ `"wired"` | `"missing"` | `"foreign"` | `"unreadable"` | `"n/a"`. `file` and `key` are null for `n/a`;
  - `opencodeConfig`, `staleLegacy` and `alternateProfiles` (FR-38).
- **FR-37** `mcpNode` reads the resolved `claudeJson`, and Codex hook state reads `codexHome` from the resolver. `status` stays read-only, makes no network calls, and exits 0 under every input, including an unparseable or missing file.

- **FR-38 Other wired locations and unparseable OpenCode (D-019, D-024, ADR-007).** `status` adds one line per finding. Each finding is evaluated against the resolver's twin candidates and AP-1..AP-3, using the FR-12 same-file rule:
  - `stale: <file> <key> holds an owned Unimatrix entry the harness no longer reads; re-run 'npx unimatrix init' to remove it`. Used for an owned entry at a removable twin location (RT-03, RT-10) while an override routes elsewhere.
  - `other profile: <file> <key> (<VAR> points elsewhere; separate <harness> config profile, init leaves it in place)`. Used for an owned entry at AP-1, AP-2 or AP-3.
  - `opencode mcp: <abs resolved opencode.json> is not valid JSON; OpenCode is left without Unimatrix wiring until it is fixed`. Used when the resolved OpenCode file exists and cannot be parsed. A legacy twin that cannot be parsed is reported as `stale: <file> unreadable (not valid JSON)`.

  `status --json` mirrors these as follows:
  - `staleLegacy: [{harness, file, key}]`. An unparseable legacy file is listed with `key: null, unreadable: true`.
  - `alternateProfiles: [{harness, file, key, var}]`.
  - `opencodeConfig: {status, file}`, where `status` ∈ `"ok"` | `"absent"` | `"unreadable"`, for the resolved `opencode.json`. None of these findings changes the exit code (0).

### E. Git-Ignore Ensure via the Exclude File (Scope In-6; D-008, D-011; SR-06, SR-07)

- **FR-40 When it runs.** The `claude-code` `git-exclude` leg runs exactly when the Claude hooks leg runs, in both dry-run and real runs. It runs after the hooks destination leg (W1 in ADR-004). If that leg did not end `created`, `updated` or `unchanged`, it emits `skipped-dest-unwired` instead. Its outcome never changes init's exit code, except for a `SafeWriteError` from the append (FR-31). `GITIGNORE_WARNING`, `gitIgnored()` and their export are removed.
- **FR-41 Git invocation (SR-07).**
  - Every git call is `execFileSync("git", args, {cwd: R, stdio: ["ignore","pipe","pipe"], timeout: 10000})`.
  - The child env is the inherited env minus `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_COMMON_DIR` and `GIT_PREFIX`, so git answers for the repo that contains R. The user's env is not modified (R1).
  - The path argument is always the literal `.claude/settings.local.json` (forward slashes on every OS), preceded by `--`.
  - Every git command used is read-only. Git never writes the exclude file and never writes the index.
- **FR-42 Decision sequence.** Evaluate in order. The first terminal row decides the leg.

  | Step | Command | Result | Leg `action` | Reason / effect |
  |---|---|---|---|---|
  | 1 | `git rev-parse --is-inside-work-tree` | spawn `ENOENT` | `skipped-no-git` | `git not found; cannot ensure .claude/settings.local.json is git-ignored` |
  | 1 | same | non-zero exit (for example 128: not a repo), or stdout ≠ `true` (bare repo, inside `.git`) | `skipped-not-repo` | `not inside a git work tree; nothing to ignore` |
  | 1 | same | timeout or other spawn error | `skipped-git-error` | `git failed: <message>` |
  | 2 | `git ls-files --error-unmatch -- .claude/settings.local.json` | exit 0 (tracked) | `skipped-tracked` | Warning: `.claude/settings.local.json is tracked by git, so ignore rules do not apply and local wiring is committable; untrack it with 'git rm --cached .claude/settings.local.json'`. No append. Index untouched |
  | 3 | `git check-ignore -q -- .claude/settings.local.json` | exit 0 | `unchanged` | `already git-ignored`; exclude file not read or written |
  | 3 | same | exit ≠ 0 and ≠ 1 | `skipped-git-error` | `git check-ignore failed (exit <n>)` |
  | 4 | `git rev-parse --git-path info/exclude` and `git rev-parse --show-prefix` | path X (resolved against R if relative), prefix P | — | Continue |
  | 5 | read X | contains a line equal to the entry E (FR-44), compared after stripping trailing `\r` and trailing whitespace | `appended-not-effective` (no write) | Warning: `<X> already lists <E> but git does not ignore .claude/settings.local.json; a higher-precedence rule (for example '!' in .gitignore) overrides it`. No append |
  | 6 | append E to X (FR-45) | — | `created` (X was absent) / `updated` | `path` = X, `reason` = `added <E>` |
  | 7 | real run only: `git check-ignore -q` again | exit ≠ 0 | `appended-not-effective` | Reason `added <E>; still not ignored (higher-precedence rule)`, plus a warning. Dry-run cannot observe this, so its leg stays `created`/`updated` (a documented FR-61 exception) |

  Step 5 makes a re-run idempotent even when ignoring is overridden (SR-06, AC-8 "re-run adds nothing"). In that state the leg is `appended-not-effective`, not `unchanged`, because a warning is still owed. AC-5 fixtures therefore do not include an overriding `!` rule.
- **FR-43 Worktrees and submodules.** The exclude path always comes from `--git-path` and is never built as `.git/info/exclude`.
  - Linked worktree: init from a linked worktree resolves R to the main checkout, and git returns the common dir's `info/exclude`.
  - Submodule: R resolves to the superproject root by default. With `--project-dir <submodule>`, git returns `<super>/.git/modules/<name>/info/exclude`.

  Each case has a fixture that asserts the entry lands in the git-reported file and that `git status --porcelain` in R does not list `.claude/settings.local.json` afterwards.
- **FR-44 Entry text.**
  - E = `/` + P + `.claude/settings.local.json`, where P is the `--show-prefix` output (empty, or ending in `/`).
  - Before concatenation, the characters `*`, `?`, `[`, `\`, `!` and a trailing space in P are backslash-escaped.
  - E is anchored to R, so it ignores only that file and no file of the same name elsewhere.
  - E is one line. No comment line is added.
- **FR-45 Append mechanics.**
  - New content = existing bytes, plus `\n` if the existing content is non-empty and does not end in `\n`, plus E, plus `\n`.
  - If X is absent, its parent `info/` is created and X is created with the new-file mode (FR-24).
  - The write goes through the hardened writer with CAS and verify (the verify predicate is "the file contains line E"). A symlinked X stays a symlink.
- **FR-46 Rendering.** The action line reads `claude-code git-exclude: <action> <path> (<reason>)` through the existing `legToActionLine`. For `skipped-no-git`, `skipped-not-repo` and `skipped-git-error`, `path` is R. Warnings go to stderr and the actions list, as other warnings do.

### F. Init Output (Scope In-2, D-013; SR-13)

- **FR-50 Resolved-file lines.** For each harness leg init processes (selected or detected), init prints one line per destination file:
  ```
  resolved: <harness> <surface> -> <abs destination> (<VAR>=<value>|default)
  ```
  The surfaces are Claude `mcp` (`claudeJson`), Claude user hooks removal target (`claudeUserSettings`), Codex `hooks` and `mcp`, and OpenCode `mcp`. These lines are identical in dry-run and real runs, apart from the `[dry-run] ` prefix.
- **FR-51 Same-env note (SR-13).** If any chosen-by is not `default`, init prints once: `note: start each harness with the same <VAR list> so it reads the files above; a harness started without them reads the default location`.
- **FR-52 Claude-running notice (D-013).** Whenever a run writes the resolved `.claude.json`, meaning any `claude-code` leg on that path with action `created`, `updated` or `removed`, init prints exactly once: `If Claude Code was running during this install, it may overwrite <abs claudeJson>; re-run 'npx unimatrix init' or check 'npx unimatrix status'`. Dry-run prints the same notice with the `[dry-run] ` prefix. A run where every `.claude.json` leg is `unchanged` prints no notice.
- **FR-53** Warnings from FR-06, FR-10 (foreign at key), FR-42 and FR-14 go to stderr and the actions list and never change the exit code. Exit 1 happens only for: unparseable resolved `.claude.json`, an unparseable project `settings.json`/`settings.local.json`/`.mcp.json`/`opencode.json` (existing throw), any `SafeWriteError` (FR-31), including `outside-project` (FR-22a), a symlinked `trust.json` (FR-20a), and existing hard failures (unsafe shim path, trust failure). An unparseable user-scope `opencode.json` and Codex files only warn, with exit 0 (FR-14). The message always names the file.

### G. Dry-Run Parity (FR-12d; AC-9; SR-15)

- **FR-60 One planning path.** Each writer, removal and the exclude leg computes its leg from one planning function, which both modes call. In dry-run the function returns before calling the hardened writer. In a real run it calls the writer with the planned bytes. No leg is computed only in one mode.
- **FR-61** From the same starting state, the dry-run manifest equals the real-run manifest. Equality is compared on `harness`, `surface`, `action`, `path`, `reason`, and `entry`/`command` where present, including twin removals (RT-03, RT-10), G2 (RT-04), G3 (RT-09), the `git-exclude` leg, the resolved-file lines (FR-50) and the D-013 notice (FR-52). Verify-retry attempts are not legs. The one allowed difference: the FR-42 step-7 `appended-not-effective` outcome can only be observed after a real append.
- **FR-62** Dry-run writes nothing: no file, no temp, no directory, no mtime change. Git commands in dry-run are only the read-only ones from FR-42 steps 1–5.
- **FR-63** The set of files whose bytes differ after a real run equals the set of paths in non-`unchanged`, non-`skipped*` legs of the preceding dry-run. Store paths under `~/.unimatrix/` are included (D-017).

---

## Non-Functional Requirements

- **NFR-01 Zero dependencies.** Only Node built-ins (`fs`, `path`, `crypto`, `child_process`, `os`). `test/check-zero-deps.js` passes. `lib/trust.js` remains requirable only with built-ins (`test/trust-selfcontained.test.js` passes unchanged), and it hosts the writer core (D-021).
- **NFR-02 Modularity (PL-10).** New logic lives in new focused modules (resolver, hardened writer, exclude-ensure). Each is ≤ 300 code lines. The existing files (`wire.js`, `status.js`, `claude-json.js`, `merge-settings.js`, `codex-install.js`, `opencode-install.js`, `install-store.js`, `trust.js`) only gain call sites and lose private writers and paths. No production file exceeds 1,000 code lines.
- **NFR-03 Synchronous writes (D-013).** No `async` function, Promise or callback in the writer or between any read and rename. A test asserts that the writer module contains no `async`, `await`, `.then(` or `setTimeout`.
- **NFR-04 Performance.** On a Linux local disk, a hardened write of a file ≤ 1 MB completes in ≤ 50 ms p95. Measured in a unit benchmark over 100 writes, reported, and not a CI gate. A second `init` with every leg `unchanged` performs zero renames.
- **NFR-05 Test isolation (SR-10, #5911).** `test/helpers/wire-fixture.js` `wireOpts` and the shared env helper build `env` with `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`, `XDG_CONFIG_HOME` and `CODEX_HOME` removed by default. A test that needs an override passes it explicitly. `assertTmpHome` additionally asserts that every override value in the injected env lies under the temp home or a temp dir. Any test that spawns init or status passes the scrubbed env.
- **NFR-06 Compatibility.**
  - Linux, macOS and Windows code paths. Windows branches (rename retry, skipped dir-fsync, path normalisation) are unit-tested with an injected `fs`/`platform`.
  - Node version floor unchanged.
  - Entry text unchanged per OS family (D-040). No marker fields.
- **NFR-07 Cumulative tests.** Extend `test/helpers`, `test/fixtures`, `merge-settings.test.js`, `wire.test.js`, `init-integration.test.js`, `install-store.test.js`, `trust.test.js`, `opencode-install.test.js`, `codex-install.test.js` and `hook-ownership.test.js`. New test files are allowed only for new modules (resolver, writer, exclude-ensure).

---

## Acceptance Criteria

AC-IDs and wording come from SCOPE.md (D-005). Each AC lists its verification method.

| AC | Criterion | Verification |
|---|---|---|
| **AC-1** | (amended, D-028) With `CLAUDE_CONFIG_DIR=<d>`, init writes MCP only to `<d>/.claude.json`. The default `~/.claude.json` entry, including `projects[root]`, is kept and listed by `status` as `other profile`. `claude mcp get unimatrix` from the project shows the entry. | (a) `wire.test.js`: seed an owned local-scope entry and a top-level entry in `<home>/.claude.json`, then run with `env={CLAUDE_CONFIG_DIR:<d>}`. Assert `<d>/.claude.json` holds the entry, `<home>/.claude.json` bytes and mtime are unchanged, and no manifest leg names `<home>/.claude.json`. `status` (text and `alternateProfiles`) lists `<home>/.claude.json` with its keys. (b) FR-12 same-file fixtures: one entry, nothing listed as `other profile`. (c) Integration, run when `claude` is on PATH (devcontainer), otherwise skipped with a message: spawn real `claude mcp get unimatrix` with `HOME=<tmp>`, `CLAUDE_CONFIG_DIR=<d>`, cwd = R, and assert exit 0 and output naming `unimatrix`. Proves SR-01/SR-14 with the real binary |
| **AC-2** | (amended, D-027) With `OPENCODE_CONFIG_DIR` or `XDG_CONFIG_HOME` set, the OpenCode MCP entry lands in the resolved dir; an owned twin at `~/.config/opencode` is removed. Real `opencode debug config --standalone` (temp HOME, cwd = project) shows the entry in the resolved `opencode.json` (D-027). | (a) `opencode-install.test.js`/`wire.test.js`: one case per chosen-by (`OPENCODE_CONFIG_DIR`, `XDG_CONFIG_HOME`, both set → `OPENCODE_CONFIG_DIR` wins and `$XDG_CONFIG_HOME/opencode` is untouched and listed as AP-1). Seed an owned twin and assert the resolved file holds the entry and the twin has no `mcp.unimatrix`, with foreign keys unchanged. FR-13 migration case without `--harness opencode`. FR-12 same-file fixtures. (b) Integration with the real binary (D-027), modeled on AC-1(c). Run when `opencode` is on PATH (devcontainer). Otherwise skip with the message `opencode not on PATH; real-binary check tracked in #1053`. Run two cases, `OPENCODE_CONFIG_DIR` and XDG-only (`XDG_CONFIG_HOME` set, `OPENCODE_CONFIG_DIR` unset). In each, spawn real `opencode debug config --standalone` with `HOME=<tmp>`, the override env and cwd = R. `--standalone` is mandatory: without it the CLI attaches to the user's background service and real config. `opencode mcp list` is not used; it was measured unusable (ARCHITECTURE §1). Parse the JSON output and select the document whose `path` equals the resolved `opencode.json`. Assert that it holds the Unimatrix entry, at `mcp.servers.unimatrix` or `mcp.unimatrix`. Assert that no document with path `<tmp>/.config/opencode/opencode.json` holds an owned entry. The observed precedence must match FR-04: `OPENCODE_CONFIG_DIR` > `XDG_CONFIG_HOME` > `~/.config/opencode`. |
| **AC-3** | A symlinked `.claude.json`, `$CODEX_HOME/config.toml`, `hooks.json` or `opencode.json` is still a symlink after init, and its target holds the change. Extension (D-010): applies to every file Unimatrix writes, including `settings.local.json`, `.mcp.json`, project `opencode.json`, `.codex/*` and `~/.unimatrix/{install.json,trust.json}`. Per D-021, a symlinked `trust.json` is refused with exit 1 instead (FR-20a). Per D-026 (amended), a project-file link is written through only if its real target is inside the realpath of the project root; otherwise init exits 1 naming the link (FR-22a). User and global links are unchanged. | Writer unit tests S1–S12 (FR-23). Integration: for each file in the FR-20 list, make it a symlink (absolute, relative and 2-hop chain) to a file in another dir, run init (or the store/trust writer), and assert `lstat(...).isSymbolicLink()`, unchanged `readlink`, and that the target content holds the change. Dangling link: init exits 1 naming the file, and the link is unchanged (FR-31). `trust.json` as a symlink: exit 1 with the FR-20a message, link and target unchanged. Project files: the FR-22a fixture set (inside, outside, symlinked root, dangling, link to dir) |
| **AC-4** | Seeded owned entries (user-scope `mcpServers.unimatrix`, legacy `opencode.json` `plugin[]`, every nan-024 legacy location) are removed; foreign entries preserved byte-for-byte. | Table-driven test over RT-01..RT-04 and RT-06..RT-10 (RT-05 retired, D-028). Under a `CLAUDE_CONFIG_DIR` override, the nan-024 location `~/.claude.json` `projects[root]` is asserted kept (AP-3), not removed. Without an override it is the destination itself. Each row seeds an owned entry plus foreign siblings, and also a foreign value at the `unimatrix` key. Assert owned entries are removed, the foreign value at the key stays with a warning, and FR-16 equality holds (bytewise on canonical fixtures). FR-15 positive and negative plugin fixtures |
| **AC-5** | A second `init` reports every leg `unchanged` and changes no file's mtime. | `wire.test.js`/`init-integration.test.js`: run init twice (with and without overrides, with twins seeded before run 1). On run 2, every leg is `unchanged`: harness legs, the `git-exclude` leg and store legs (D-017, FR-26a). `mtimeMs` and bytes of every resolved file, twin location, project file, `.claude/skills/*`, the exclude file and every `~/.unimatrix/` file written by init (bin, `install.json`, root marker, `remote.json` for `--bundle`) are equal before and after run 2. No D-013 notice on run 2 |
| **AC-6** | Across all resolved scopes, at most one owned entry per hook event and per harness MCP key. | After seeding duplicates in every RT location and running init, a scanner over resolved files and RT locations (AP-1..AP-3 excluded as separate profiles, D-019/D-028) counts owned entries per (harness, event) and (harness, MCP key). Every count ≤ 1 (FR-17) |
| **AC-7** | If the destination changes between read and rename (simulated), init retries; if verify still finds no entry, init exits 1 naming the file and never leaves the harness without wiring. | Writer tests use `opts.fault("before-rename")` to mutate the target between read and rename. Attempt 2 succeeds → leg `updated`, final file holds both the concurrent change and the entry. Mutation on every attempt → `SafeWriteError{code:"conflict"}`. `opts.fault("after-rename")` stripping the entry → `SafeWriteError{code:"verify-failed"}`. Init exits 1, stderr names the file, and no twin removal ran, so the old wiring is still present (FR-11). No temp remains |
| **AC-8** | (reworded, D-011; intent unchanged) If `git check-ignore .claude/settings.local.json` fails, init appends one entry to the exclude file resolved by `git rev-parse --git-path info/exclude`; if already ignored (any source), that file is untouched. Outside a git repo, init skips and reports it. Re-run adds nothing. `--dry-run` reports it. Edge cases per FR-40..FR-46 (D-008). | Fixtures with real `git` in temp repos, one per FR-42 row: not ignored → `updated` or `created` (exactly one new line E; `git status --porcelain` omits the file). Ignored via `.gitignore`, via `core.excludesFile` and via the existing exclude → `unchanged`, exclude bytes and mtime unchanged. Tracked file → `skipped-tracked`, warning, exclude and index unchanged. Exclude already lists E but `!` in `.gitignore` → `appended-not-effective`, no append on repeat runs. `--project-dir` in a non-repo dir → `skipped-not-repo`. `PATH` without git → `skipped-no-git`. Linked worktree and submodule (FR-43). Subdirectory R → prefixed E (FR-44). Missing exclude file or `info/` → created. Symlinked exclude kept. Run twice → second `unchanged`. Dry-run → `created`/`updated` leg, no write. No tracked file changes in any case (`git diff --exit-code` and `git diff --cached --exit-code`) |
| **AC-9** | `--dry-run` lists exactly the writes and removals of a real run, including twins. | Parity harness over every fixture from AC-1..AC-8: snapshot dry-run manifest and action lines, assert the tree hash is unchanged (FR-62), run real, assert manifest equality (FR-61) and changed-file-set equality (FR-63) |
| **AC-10** | Temp files use a random `O_EXCL` name; none remain after a failed write. Extension (D-010): applies to every file Unimatrix writes (same set as AC-3). | Writer tests: a spy on `fs.openSync` asserts flag `wx` and a name matching `/^\.[^/]+\.tmp-[0-9a-f]{16}$/`. A pre-created colliding name forces a regenerate. Failure injection at open, write, fsync, rename, CAS and verify leaves no `.*.tmp-*` in the target dir. Each FR-20 caller is exercised with a rename failure, and no temp remains. A grep test confirms no `.pid`-suffixed temp names and no file write outside the core and API (FR-20) |

---

## User Workflows

1. **Default install.** The user runs `npx unimatrix init` in a repo.
   - Resolved-file lines show `default`.
   - The `git-exclude` leg is `created`, `updated` or `unchanged`.
   - Hooks go to `.claude/settings.local.json`, and Claude MCP goes to the `~/.claude.json` local scope.
   - Legacy and G2/G3 entries are removed.
   - The D-013 notice is printed if `.claude.json` changed.
2. **Override install.** With `CLAUDE_CONFIG_DIR` (or the OpenCode vars) exported:
   - init writes under the override, and after verify removes owned entries from the legacy twins RT-03 and RT-10 only. The default `~/.claude.json` and the AP locations are kept and listed by `status` as `other profile`;
   - it prints the resolved lines with `<VAR>=<value>` and the same-env note.
3. **Relative override.** Init warns, naming the variable, and behaves as if the variable were unset. `status` repeats the warning.
4. **Lost entry.** After `claude purge`, `unimatrix status` prints `mcp claude-code: MISSING ... in <file>; run 'npx unimatrix init'`. Re-running init restores the entry.
5. **Dry-run.** `init --dry-run` prints the exact leg list, resolved lines and notices of a real run, and touches nothing.
6. **Tracked settings file.** Init warns that the file is tracked and gives the `git rm --cached` hint. It changes no git state.

---

## Constraints

- **R1:** no PATH, profile or repo-layout changes. Env vars are read, never set (the scrubbed child env in FR-41 affects only the git child). No tracked file is edited, and the exclude file is untracked git metadata (D-011).
- **D-001/D-003:** no destination moves. Claude MCP stays in the `.claude.json` local-scope entry.
- **D-002:** ADR-004/D-043 `mcp-stdio` behaviour and the launcher are unchanged.
- **D-046 placement rule:** the architect records it in a new ADR that supersedes the placement rationale of nan-024 ADR-001 (#5896) and ADR-003 (#5897). The ADR names the exceptions, the exact `CLAUDE_CONFIG_DIR` layout (FR-03), and the intent gate (store/credential).
- **D-040 / D-065 / D-010 (ADR-003 uniqueness)** as stated in SCOPE.md. Ownership stays in `lib/hook-ownership.js`.
- **FR-07j:** an unparseable `.claude.json` aborts that leg untouched with exit 1. The hardened writer never weakens this.
- **No Rust.** Node only, in `packages/unimatrix/lib/`.
- **D-012:** Codex checks are file-level. The Claude real-binary check is AC-1(c) only, and #1053 tracks CI.
- **D-013:** the lost-update window against a running Claude session is accepted. It is mitigated only by FR-21, FR-52 and FR-35.

## Dependencies

- nan-024 base code (merged, PR #1033).
- Node built-ins: `fs` (`openSync` `wx`, `fsyncSync`, `realpathSync`, `lstatSync`, `renameSync`), `crypto` (`randomBytes`, `createHash('sha256')`), `child_process` (`execFileSync`).
- The `git` binary (optional at runtime; FR-42 handles its absence). The `claude` binary (optional; used only for AC-1(c)).
- Existing modules: `hook-ownership.js`, `claude-json.js` `resolveProjectKey`, `codex-install.js` `codexHome`, `hook-client/config.js` `walkToProjectRoot`/`resolveGitFile`, `opencode-install.js` `detectIndent`/`readJsonSafe`.
- Closes #1035 F2 and the temp-naming item. Related: #998, #1014, #1034, #1053.

## NOT in Scope

- Shim, trust-check logic, launcher hook path, latency budgets (nan-024 ADR-001 shim form, ADR-002, ADR-005).
- The zero-tool `mcp-stdio` handshake outside a Unimatrix project (D-002).
- OpenCode hooks (#998). Claude plugin packaging. Uninstall.
- Moving any destination. Claude hook exec form. OpenCode `project_overlay`. `{env:VAR}` forms.
- Trust-check TOCTOU (#1035 F4). Broader `status` diagnostics (#1034) beyond FR-34..FR-38.
- Removing entries at alternate-profile locations AP-1..AP-3 (D-019). Status only lists them.
- Pre-nan-024 OpenCode artifacts other than `plugin[]` (`.opencode/plugins/` shim, `.opencode/package.json` dep) (D-014).
- Hook-time runtime state writes (D-020).
- Cleanup of temp files left by a killed process. Preserving hard links. Preserving file ownership/ACLs across rename.
- OpenCode `opencode.jsonc` global files. `OPENCODE_CONFIG` (explicit file var).
- Modifying the git index or `.gitignore` in any case.

---

## Open Questions

None.

Closed:
- Credential mode is decided by D-025: `remote.json`, `install.json`, `trust.json`, the root marker and `bin/*` get their mode re-applied on every write (FR-24).
- Removal locations follow D-019/D-028: under an override, the default `~/.claude.json` is AP-3 and is never modified.
- RT-03 (owned hooks in the default `~/.claude/settings.json` are removed under `CLAUDE_CONFIG_DIR`) is kept, as In-3 is written (D-030).

Naming follows ARCHITECTURE §6a "Canonical Contract Names", which is the naming authority.
