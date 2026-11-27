# Scope Risk Assessment: 1036

## User-Facing Entry Points & Behavioral Outcomes

| Entry point (how the user/agent actually invokes it) | Path-independent outcome they must observe |
|---|---|
| `unimatrix init` with no config-dir env vars | Each selected harness, started normally from the project, shows Unimatrix wiring (`claude mcp get unimatrix` lists it; OpenCode/Codex config holds one owned entry). Abandoned and shadowing owned entries are gone. Foreign entries are unchanged |
| `unimatrix init` with `CLAUDE_CONFIG_DIR` / `OPENCODE_CONFIG_DIR` / `XDG_CONFIG_HOME` / `CODEX_HOME` set | The harness, started with the same env, sees the entry. The default location keeps no owned twin unless it is the same real file |
| `unimatrix init` with a relative override value | A warning names the ignored var; the outcome matches the no-override case |
| `unimatrix init` where a harness config file is a symlink | Afterwards it is still a symlink, its target holds the change, and no temp file is left |
| `unimatrix init` that cannot verify its write | Exit 1 names the file; the harness still has wiring (old or new), never none |
| `unimatrix init` run a second time | Every leg reports `unchanged`; no file's mtime changes, including the exclude file |
| `unimatrix init --dry-run` | Lists exactly the writes, removals, twins and exclude append a real run would do; changes nothing on disk |
| `unimatrix init` in a git repo / outside git | `git status` never shows `.claude/settings.local.json`; no tracked file changes. Outside git the skip is reported |
| `unimatrix status` | Prints every resolved config file; names the `.claude.json` that lacks `projects[<root>].mcpServers.unimatrix` and gives the init hint; exits 0 |
| `unimatrix init` while Claude is running | Output tells the user to re-run init or check `status` (D-013) |

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | The `CLAUDE_CONFIG_DIR` layout is assumed, not measured: `<d>/.claude.json` and `<d>/settings.json` (no `.claude/` segment). If it is wrong, init writes files Claude never reads (G1 again) | High | Med | Confirm the layout with real `claude` in the devcontainer before you design (AC-1). Write the exact paths into the ADR |
| SR-02 | Edge cases in the hardened writer: dangling, relative or chained symlinks; a target dir that cannot be written although the link dir can; `realpath` racing a link swap; Windows symlinks and rename-onto-target | High | Med | The spec should list symlink cases explicitly: a dangling link fails loud and is never replaced with a regular file. Put the temp file in the resolved target's directory |
| SR-03 | Moving `trust.json`/`install.json` onto the new writer regresses existing `renameWithRetry` and mode behaviour. `trust.json` feeds the trust check, so a regression is security-relevant | High | Med | Carry the existing retry and mode tests over unchanged before you delete the private writers. Treat it as a migration, not a rewrite |
| SR-04 | Durability primitives under zero-deps Node: `openSync('wx')`, `fsyncSync` on the file and the dir (the dir fsync fails on Windows), and `crypto.randomBytes` names. A dir-fsync that throws on win32 would fail every write | Med | Med | Allow dir fsync to be best-effort per platform. Unit-test the win32 branch with an injected `fs` |
| SR-05 | OpenCode `opencode.json` may be JSONC (comments, trailing commas). Parse+stringify for G3/twin removal either aborts or strips comments, which breaks the "foreign entries byte-for-byte" promise | Med | Med | Define the parse-failure behaviour for each file (abort the leg, exit 1, like FR-07j). State what "byte-for-byte" means: entry values, or whole-file text |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `git check-ignore` does not report tracked files as ignored. If `.claude/settings.local.json` is already tracked, every run appends to the exclude file. That breaks AC-5 and AC-8 ("re-run adds nothing") and still leaves the file committable | High | Med | The spec (D-008) must handle the tracked case: detect it (`git ls-files`), skip the append, report a warning. Before appending, also check whether the entry text is already present |
| SR-07 | `git rev-parse --git-path` returns a path relative to the cwd. Hooks live at `<main root>`, which may differ from the cwd or worktree init runs in. Absent git binary, a bare repo and submodules all have distinct exit codes | Med | Med | Run git with `cwd = <main root>`, resolve the output against that cwd, and list each exit-code branch in the spec |
| SR-08 | Twin removal scope grows: every override × default location × harness, plus legacy nan-024 locations. The removal set has no single listing, so AC-4/AC-6 coverage gaps are likely | Med | High | Specify one table of removal targets (location, key, condition) as the source for code, dry-run and tests |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-09 | `status.js` has its own `codexHome` fallback and its own `.claude.json` path in `mcpNode`. If any reader keeps a private path, `status` reports a different file than init wrote | High | High | One resolver used by `wire`, `status` and `mcpNode`. Delete every private fallback. Add a test that `status` and `init` print the same resolved paths under each override |
| SR-10 | Test isolation: the new env vars may be set in the devcontainer or CI (XDG is common). Tests that inject only `home` (#5911) would read real config dirs or write to them | High | Med | Extend `assertTmpHome`/the env helper to scrub all four vars by default; overrides must be opt-in per test |
| SR-11 | Write-before-remove ordering spans files and harnesses. A crash between verify and twin removal leaves duplicates. Removal before verify leaves a harness dark | Med | Low | Define the order per harness. A leftover twin is recoverable (the next run removes it); a missing entry is not |
| SR-12 | Matching ownership of the legacy `plugin[]` string: version suffixes (`@x.y`), `file:` paths or other spellings are missed, or a foreign plugin whose name contains the prefix is wrongly removed | Med | Med | Add the exact accepted forms to `hook-ownership.js` with positive and negative fixtures |

## Path-Divergence Risks

| Risk ID | Entry point | Divergence (path that works vs. path the user invokes) | Recommendation |
|---|---|---|---|
| SR-13 | `init` with an override in the shell | Init resolves the env of its own shell, but the user runs the harness from an IDE, GUI or another shell without that var. The entry lands in the override dir, the default twin is removed, and the harness reads the default location and goes dark. The env-in-test path passes | Init output names each resolved file and the env var that caused it. `status` prints the same. Document that the harness must run with the same env. Do not assume init's env equals the harness env |
| SR-14 | `init` (verify step) | Verify re-reads through Unimatrix's own resolver and parser, so it passes even when the harness reads a different file (SR-01) | Discharge AC-1 with real `claude mcp get`, not a file read. Under D-012, record the Codex gap as file-level only |
| SR-15 | `init --dry-run` | Dry-run is computed by a separate branch and skips twin, exclude or verify-retry legs, so its list differs from a real run | Each writer returns legs from the same planning function in both modes. Test: the dry-run list equals the real-run diff for every fixture |
| SR-16 | `status` | `status` reports "present" because it found the entry at a twin or default path, not in the resolved file the harness reads | `status` checks only the resolved file and names it. A twin that is present is reported as stale, never as healthy |
| SR-17 | Relative override value | The value is ignored with a warning, but the harness itself honours the relative value (relative to its own cwd), so the paths diverge silently | Warn loudly and name the var. `status` repeats the warning |

## Assumptions

- **Background/Placement table:** `${CLAUDE_CONFIG_DIR:-~}/.claude.json` is the file Claude reads (SR-01). The user `settings.json` location under the override is not measured.
- **Scope In-2:** the env init sees equals the env the harness runs with (SR-13).
- **Scope In-6:** `git check-ignore` is enough to decide "already ignored". This is false for tracked files (SR-06).
- **Scope In-4:** realpath + same-dir rename keeps the symlink on every OS. Windows symlink semantics are not measured.
- **Current code table:** `install-store.js`/`trust.js` retry logic can be reused as-is in a writer that also has to handle symlinks.
- **AC-5:** the "no mtime change" check covers the exclude file and every twin location, so each needs an unchanged short-circuit.

## Design Recommendations

1. One config-dir resolver module, consumed by `wire`, `status` and `mcpNode`; no private fallbacks (SR-09, SR-16).
2. One planning function produces the legs (writes, removals, twins, exclude). Dry-run and real runs both execute it (SR-08, SR-15).
3. Writer spec: symlink cases enumerated, dangling link fails loud, platform-conditional fsync, existing retry/mode tests migrated first (SR-02, SR-03, SR-04).
4. AC-8 spec: tracked-file case, entry-already-present check, git cwd = main root, exit-code table (SR-06, SR-07).
5. Init and `status` output names each resolved file and the env var behind it (SR-13, SR-17).
6. Test env helper scrubs `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`, `XDG_CONFIG_HOME` and `CODEX_HOME` by default (SR-10, #5911).
7. AC-1 is proven with real `claude mcp get`, not the verify read (SR-01, SR-14).
