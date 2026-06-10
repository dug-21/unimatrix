# Risk-Based Test Strategy: nan-016

> UDS dogfooding re-release capability + (delivered, unexecuted) switchover mechanism.
> Architecture-risk pass over the four components: `dogfood-install.sh`,
> `dogfood-switchover.sh`, `test/dogfood-effect.test.js`, `RUNBOOK.md`. Risks are specific
> to THIS design — the copy-install/clean-replace, the `mergeSettings`-routed switchover,
> the effect harness, fail-open under daemon-absent, and the deferred-flip boundary.
>
> Historical evidence applied: Unimatrix #4796 (gates asserting un-executed CI ACs as fact;
> macOS tmpdir-symlink state-dir split) directly informs R-03, R-08, R-11; #2928 (effect
> over string-diff; per-rule isolation tests) informs R-01, R-04.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Effect harness regresses to a string-diff / dry-run snapshot and never re-fires a real hook against the installed path — AC-02/AC-03 become vacuous | High | High | Critical |
| R-02 | Clean-replace is non-atomic — a crash/abort mid-extract leaves a partial tree observable as "the installed client"; or replace is an overlay, leaving stale shadowing files | High | Med | Critical |
| R-03 | Scratch project-root hash collides with or resolves to this repo's real `~/.unimatrix/{hash}/` (e.g. tmpdir under a symlinked path resolves to a shared root) — harness contacts the live daemon / shares live state | High | Med | Critical |
| R-04 | AC-03 isolation proof is structurally weak — asserts only "bytes unchanged after a no-op" without a behavior-changing in-repo edit, so it cannot detect a real `npm link`-style leak | High | Med | Critical |
| R-05 | Switchover via `mergeSettings` requires the **installed** `lib/merge-settings.js`; if the install is incomplete/absent the require throws and the switchover/harness fails opaquely (not fail-open — this is tooling) | Med | Med | High |
| R-06 | Rollback's legacy-string arm emits a Rust command that drifts from the true pre-flip form (wrong `LD_LIBRARY_PATH`/binary path, wrong event set, narrowed PreToolUse matcher carried over) — rollback does not restore pre-switchover behavior | High | Med | High |
| R-07 | Re-fired installed-path hook in the daemon-absent case exits non-zero (broken node resolution, partial tree, thrown error) — violates C-7 fail-open; the eventual live flip could break host sessions | High | Low | High |
| R-08 | A test/harness path defaults onto or writes this repo's live `.claude/settings.json` (missing/incorrect `--settings`, guard absent or bypassable) — reads as "flip executed", perturbing a future F6 soak | High | Low | High |
| R-09 | Matcher-narrowing delta (PreToolUse `"*"` → `PRETOOLUSE_CYCLE_MATCHER`) is not asserted, or asserted against a hardcoded string that drifts from shipped `EVENT_MATCHERS` — operator surprised at flip time | Med | Med | Medium |
| R-10 | Event-set assumption wrong — harness asserts 9 events on a scratch root where `SubagentStop` is opt-in (8 registered), or foreign/duplicate hooks not asserted — false pass/fail | Med | Med | Medium |
| R-11 | `npm pack` tarball drift — `files` array changes upstream, or postinstall is triggered by the chosen mechanism, so the frozen tree is incomplete or mutates the host; or `npm install --prefix` is silently used instead of `npm pack`, changing on-disk shape | High | Low | High |
| R-12 | Container-rebuild non-durability — install hardcodes a node-version-pinned or cwd-relative path instead of the fixed `~/.unimatrix/dogfood-client/`, so re-release after rebuild is not the same soak-reset point | Med | Low | Medium |
| R-13 | AC-03 in-repo edit (tracked source) is not restored cleanly — test crash/abort leaves `packages/unimatrix/lib/hook-client/` dirty, perturbing the working tree and other agents | Med | Med | Medium |
| R-14 | Init / size / zero-deps regression — adding scripts/runbook inadvertently touches `lib/init.js`/`merge-settings.js`/`config.js`/`package.json` runtime behavior; AC-05/AC-06/C-9 break | Med | Low | Medium |
| R-15 | Harness `before`-hook install writes to the real `~/.unimatrix/dogfood-client/`, disturbing a human-staged dogfood install / soak in progress | Med | Low | Medium |

## Risk-to-Scenario Mapping

### R-01: Vacuous effect verification (string-diff regression)
**Severity**: High **Likelihood**: High **Impact**: AC-02/AC-03 pass while the switchover/installed client are actually broken; the whole feature's value (a *provable* re-release mechanism) is hollow. SR-04 materialized. (Evidence: #2928 — "each rule must be individually proven", effect over snapshot.)

**Test Scenarios**:
1. Harness MUST `execFileSync("node", [installedIndexJs, "SessionStart"], {cwd: scratchRoot, input: JSON.stringify(payload)})` and assert exit 0 + empty stdout — a *real* invocation of the installed entrypoint, not a parse of the settings file.
2. Negative control: point the emitted command at a deliberately-broken path and assert the harness's re-fire assertion FAILS — proves the assertion is non-vacuous (it can detect a bad install).
3. Assert the parsed scratch `settings.json` command string equals `node <installed>/lib/hook-client/index.js <EVENT>` AND that `<installed>` is the real fixed/test-scoped install dir, not a placeholder.

**Coverage Requirement**: At least one test re-fires the installed-path hook and asserts runtime behavior; a negative control proves the re-fire assertion fails when the install is broken. A pure settings-file string assertion alone is insufficient.

### R-02: Non-atomic / overlay clean-replace
**Severity**: High **Likelihood**: Med **Impact**: A stale prior install shadows a new build (re-run ≠ soak reset, SR-02), or an interrupted install leaves a half-tree that the next switchover wires into the live repo.

**Test Scenarios**:
1. Install; mutate an installed file and add a stray extra file under the install dir; re-install; assert the tree is byte-identical to a fresh install and the stray file is GONE (overlay would keep it).
2. Assert the install stages to a sibling temp dir and `mv`s over the target (inspect for staged-then-rename, not extract-in-place) — partial extract never observable at the target path.
3. Pre-existing-target run: install onto a populated target dir; assert clean replacement, defined behavior, exit 0.

**Coverage Requirement**: Re-install removes all prior bytes (no overlay residue); install is staged + atomically renamed; NFR-1 byte-identical re-run verified by mutate-then-reinstall-then-compare.

### R-03: Scratch-hash collision with live state
**Severity**: High **Likelihood**: Med **Impact**: Harness contacts the live daemon / shares the live `~/.unimatrix/{hash}/` — the daemon-absent fail-open case is not actually exercised, and the harness perturbs live runtime state. (Evidence: #4796 — macOS tmpdir-symlink state-dir split; on this codebase `config.js` `realpath`s the root, so a symlinked tmpdir can collapse to an unexpected hash.)

**Test Scenarios**:
1. Create the scratch root with a real `.git/` directory under `os.tmpdir()`; compute its project-root hash via the shipped `computeProjectHash` and assert it differs from this repo's hash.
2. Assert no `~/.unimatrix/{scratchHash}/unimatrix.sock` exists before the re-fire (the daemon-absent precondition) — and that the scratchHash is NOT this repo's hash.
3. `realpath` the scratch root explicitly and assert the hash is computed over the realpath'd path (mirror `config.js`), so a symlinked `os.tmpdir()` cannot collapse onto the live root.

**Coverage Requirement**: Scratch hash is asserted distinct from the live repo hash; the daemon-absent precondition (no scratch socket) is asserted before the re-fire; realpath handling matches `config.js`.

### R-04: Weak isolation proof (cannot detect a leak)
**Severity**: High **Likelihood**: Med **Impact**: A future `npm link` regression or overlay leak passes AC-03 undetected; SR-07's code-freeze guarantee is unproven. (Evidence: #2928 — isolation must be individually, behaviorally proven.)

**Test Scenarios**:
1. Capture installed `lib/hook-client/index.js` content hash; make a **behavior-changing** edit to in-repo source (e.g. `process.stderr.write` marker) in a *throwaway copy or stash* (per OQ-D / R-13); assert installed-path hash unchanged AND re-fired installed-path behavior unchanged (marker absent).
2. Structural guard: `fs.lstatSync(installedIndexJs).isSymbolicLink() === false` — the explicit anti-`npm link` (C-6) assertion.
3. Document/assert that AC-03 is code-freeze, NOT state-dir separation — a negative assertion that the test does NOT require separate `{hash}` dirs (SR-07, #4923).

**Coverage Requirement**: Isolation proven via a behavior-changing in-repo edit + installed-byte-and-behavior invariance + non-symlink assertion. A no-op "bytes unchanged" check alone is insufficient.

### R-05: `mergeSettings` require from installed client throws opaquely
**Severity**: Med **Likelihood**: Med **Impact**: When the install is absent/incomplete the switchover/harness dies with a require stack trace, not a clear "install first" message — and could mask a real completeness gap (SR-01).

**Test Scenarios**:
1. Run switchover with `--client` pointing at a non-existent/empty dir; assert a clear, loud non-zero exit with an actionable message (this is tooling, may be loud — distinct from C-7 hook fail-open).
2. Post-install completeness assert: `lib/merge-settings.js`, `lib/hook-client/index.js`, `lib/hook-client/config.js`, and the full `lib/hook-client/*.js` set exist before switchover requires them.
3. Harness `before`-hook skips with a clear message (not a hard error) when no install is present and none is staged.

**Coverage Requirement**: Missing/incomplete install produces a loud, actionable tooling error; completeness asserted before require; harness degrades to skip-with-message, not opaque crash.

### R-06: Rollback drifts from the true pre-flip Rust form
**Severity**: High **Likelihood**: Med **Impact**: Operator rolls back during a problem and does NOT restore working hooks — host session degraded with no clean recovery. The "no bespoke revert logic to drift" claim must be proven.

**Test Scenarios**:
1. On a scratch settings seeded with the real Rust-hook shape, promote then rollback; assert every command equals `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>` over the correct event set.
2. Assert `isUnimatrixHook` still owns the rolled-back entries (idempotent re-point) and foreign hooks survive both promote and rollback.
3. Assert rollback is idempotent (rollback twice = same result) and that the legacy-string arm of `normalizeCommandSource` is what emits it (require the installed `merge-settings.js`).

**Coverage Requirement**: Rollback reproduces the exact Rust command form over the correct events, is idempotent, preserves foreign hooks, and uses the shipped legacy arm — no nan-016 bespoke revert string.

### R-07: Daemon-absent re-fire exits non-zero (C-7 violation)
**Severity**: High **Likelihood**: Low **Impact**: The eventual live flip introduces a host-breaking hook path — the exact outcome C-7 forbids. SR-08 materialized.

**Test Scenarios**:
1. Re-fire the installed-path hook against a scratch root whose `{hash}` has no socket; assert exit 0 and empty stdout (FR-11).
2. Re-fire with a malformed/empty stdin payload; assert exit 0 (fail-open on bad input, not just absent daemon).
3. Assert the emitted command form is exactly what `buildHookClientCommand` produces (bare path, no whitespace) so the shipped fail-open path is the one actually invoked.

**Coverage Requirement**: Installed-path hook exits 0 in daemon-absent AND malformed-input cases; the invoked command is the shipped `buildHookClientCommand` form.

### R-08: Live-settings mutation (deferred-flip boundary breach)
**Severity**: High **Likelihood**: Low **Impact**: A transient or accidental write to `/workspaces/unimatrix/.claude/settings.json` reads as "switchover executed" → starts/perturbs the F6 soak clock prematurely. SR-06 materialized. (Evidence: #4796 — asserting an action as executed when it must not be.)

**Test Scenarios**:
1. Guard test: assert the `--settings` arg passed to the script is under `os.tmpdir()`; assert the guard rejects a path equal to the live settings path.
2. Pre/post suite hash of `/workspaces/unimatrix/.claude/settings.json`; assert byte-identical after the full test run (NFR-4, zero live writes).
3. Assert the harness only READS live settings (to copy shape into the scratch fixture) and never opens it for write.

**Coverage Requirement**: A tmpdir guard rejects live-settings paths; a pre/post suite hash proves zero live mutation; live settings opened read-only only.

### R-09: Matcher-narrowing delta not asserted / drifts
**Severity**: Med **Likelihood**: Med **Impact**: The intended PreToolUse narrowing is invisible until the live flip surprises the operator, or a hardcoded matcher string in the test drifts from shipped `EVENT_MATCHERS`. SR-05 materialized.

**Test Scenarios**:
1. After promote on a scratch seeded with `"*"` PreToolUse, assert the resulting PreToolUse matcher equals the **imported** `PRETOOLUSE_CYCLE_MATCHER` from the installed `merge-settings.js` (not a copy-pasted literal).
2. Assert the runbook documents this narrowing as an intended delta (AC-04 cross-check).

**Coverage Requirement**: Matcher delta asserted against the imported shipped constant; runbook documents it.

### R-10: Event-set / hook-shape assumptions wrong
**Severity**: Med **Likelihood**: Med **Impact**: False pass/fail from assuming 9 events when `SubagentStop` is opt-in (8 on a no-opt-in scratch root), or from not asserting duplicate/foreign behavior.

**Test Scenarios**:
1. On a scratch root with no `settings.local.json` opt-in, assert exactly the 8 non-opt-in events are registered; with opt-in, assert 9.
2. Assert no duplicate Unimatrix entries after promote (idempotent re-point updates in place).
3. Seed a foreign hook; assert it survives promote and rollback unchanged.

**Coverage Requirement**: Event count asserted against actual opt-in state; no-duplicate and foreign-preservation asserted.

### R-11: `npm pack` tarball drift / postinstall side-effect
**Severity**: High **Likelihood**: Low **Impact**: Frozen tree incomplete or host mutated by a triggered postinstall (SR-01); or mechanism silently swapped (`npm install --prefix`) changing on-disk shape and the isolation/run assumptions.

**Test Scenarios**:
1. After install, assert the full `files`-array set (`bin/ lib/ skills/ postinstall.js protocols/`) is present and `lib/hook-client/index.js` is runnable.
2. Assert NO postinstall side-effect occurred (e.g. no ONNX model downloaded into the host; `postinstall.js` present-but-inert).
3. Assert the platform binary is absent from the frozen tree (it is an optionalDependency the client never spawns) — confirms the client-only freeze and that no host-binary fetch was triggered.

**Coverage Requirement**: Frozen tree completeness asserted for client needs; postinstall proven inert; no host mutation; pinned mechanism (ADR-001 `npm pack`) verified.

### R-12: Container-rebuild non-durability
**Severity**: Med **Likelihood**: Low **Impact**: Re-release after a node-bump rebuild lands at a different path → not the same soak-reset point (SR-02/NFR-7).

**Test Scenarios**:
1. Assert the install target is the fixed `~/.unimatrix/dogfood-client/` (or explicit `--target`), never an npm global / node-version-pinned prefix.
2. Assert the emitted switchover command embeds the fixed absolute path (rebuild-stable).

**Coverage Requirement**: Install target and emitted command both use the fixed absolute path, independent of node version.

### R-13: AC-03 in-repo tracked-source edit not restored
**Severity**: Med **Likelihood**: Med **Impact**: Test crash leaves the working tree dirty, perturbing other agents/delivery (OQ-D).

**Test Scenarios**:
1. Prefer editing a *throwaway copy* of `lib/hook-client/` (or git stash/restore) rather than the live tracked file; assert the working tree is clean after the test (no diff in `packages/unimatrix/lib/hook-client/`).
2. If a live edit is unavoidable, restore in a `finally`/`after` hook; assert restoration even on assertion failure.

**Coverage Requirement**: Working tree is provably clean after the isolation test, including on failure paths (restore in teardown, not inline).

### R-14: Init / size / zero-deps regression
**Severity**: Med **Likelihood**: Low **Impact**: AC-05/AC-06/C-9 break from inadvertent edits to frozen surfaces (SR-09).

**Test Scenarios**:
1. Run existing `test/init.test.js`, `test/init-integration.test.js`, `test/merge-settings.test.js`; assert green.
2. Run `node test/check-hook-client-size.js` and `node test/check-zero-deps.js`; assert exit 0.
3. `git diff` shows zero behavioral change to `lib/init.js`, `lib/merge-settings.js`, `lib/hook-client/config.js`, `package.json` runtime.

**Coverage Requirement**: Init/merge tests green; size + zero-deps gates pass; no diff to frozen runtime surfaces.

### R-15: `before`-hook install disturbs human-staged dogfood install
**Severity**: Med **Likelihood**: Low **Impact**: Running the suite clobbers a real `~/.unimatrix/dogfood-client/` mid-soak (OQ-2 in arch).

**Test Scenarios**:
1. Harness `before`-hook installs into a **test-scoped temp dir**, never the real fixed dir; assert the test `--client` arg is under `os.tmpdir()` (or a clearly test-scoped path).
2. Assert the real `~/.unimatrix/dogfood-client/` content hash is unchanged across the suite if it pre-exists.

**Coverage Requirement**: Suite installs only into a test-scoped temp dir; real dogfood install untouched.

## Integration Risks

- **Installed `mergeSettings` is the integration seam (R-05, R-06, R-09).** Both switchover and rollback `require` the *installed* `lib/merge-settings.js` — promotion validates the frozen copy's own merge logic is runnable. A broken/incomplete freeze breaks the seam; cover with the completeness assertion (R-05) and the imported-constant matcher assertion (R-09).
- **Shared `~/.unimatrix/{hash}/` state vs. scratch isolation (R-03, R-15).** The installed copy deliberately shares socket/state with the Rust daemon (#4923). The harness's only isolation lever is the **scratch project-root hash**; if that collapses onto the live hash (symlinked tmpdir, realpath), the harness is neither isolated nor a true daemon-absent test.
- **Settings ownership regex across command forms (R-06, R-10).** `isUnimatrixHook` must recognize Rust-binary, legacy `unimatrix-server`, and node-client forms for idempotent promote↔rollback. Cover round-trip promote→rollback→promote with foreign-hook preservation.

## Edge Cases

- Pre-existing populated install target (overlay vs clean-replace) — R-02.
- Scratch root reached via a symlinked `os.tmpdir()` collapsing to an unexpected realpath hash — R-03 (mirrors #4796 macOS symlink split).
- `SubagentStop` opt-in absent → 8 events, not 9 — R-10.
- Install dir or client path containing whitespace (path-quoting in `buildHookClientCommand`) — assert bare vs quoted emission matches the path.
- Malformed/empty hook stdin payload in the re-fire — R-07.
- Missing/empty `--client` dir at switchover — R-05.
- Interrupted/partial extract (staged-mv guarantee) — R-02.

## Security Risks

Untrusted input surface is narrow but real — these are operator-run dev scripts, not network-facing, yet they wire executable hook commands into a settings file and shell out.

- **Path injection into emitted hook command.** `--client`/`--target`/`--settings` flow into the emitted `node <path> <EVENT>` command and into filesystem ops. A path with shell metacharacters or whitespace must be quoted exactly as `buildHookClientCommand` does; assert injection-shaped paths are quoted/rejected, not interpolated into an unquoted shell command. Blast radius: a malformed client path could produce a hook command that runs arbitrary node — but only against the operator's own settings the operator chose.
- **Clean-replace `rm -rf` of the target dir.** The install removes `~/.unimatrix/dogfood-client/` (or `--target`). A mis-resolved/empty `--target` must NOT degrade to `rm -rf` of a parent or `$HOME`. Assert the target is validated non-empty and resolves under `~/.unimatrix/` (or an explicit absolute `--target`) before removal. Blast radius if compromised: local data loss.
- **tmpdir guard is a safety boundary (R-08), not just a test concern.** The `--settings under os.tmpdir()` guard prevents the harness from ever writing the live settings; treat it as a security/correctness invariant with its own negative test.
- **Inert postinstall (R-11).** The copied `postinstall.js` must never execute during copy-install — it would fetch the ONNX model and mutate the host. Extraction is a file copy; assert no execution occurs.

## Failure Modes

- **Daemon absent at re-fire / at eventual flip:** hook exits 0, empty stdout, host session uninterrupted (C-7). The script never probes/manages the daemon (ADR-004).
- **Install incomplete/absent at switchover:** loud, actionable non-zero tooling error ("run dogfood-install.sh first") — tooling may be loud; hooks may not.
- **Interrupted install:** staged temp + atomic `mv` means the target is either the old complete tree or the new complete tree, never a partial one.
- **Rollback during an incident:** restores the exact Rust command form via the shipped legacy arm; idempotent; foreign hooks preserved.
- **Test abort mid-isolation-edit:** working tree restored in teardown (R-13); zero live-settings writes regardless of failure path (R-08).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (incomplete frozen tree / postinstall side-effect) | R-11, R-05 | ADR-001 `npm pack` honors `files`; postinstall copied-but-inert; post-extract completeness + smoke assertion; completeness asserted before `mergeSettings` require |
| SR-02 (stale install shadowing / non-idempotent) | R-02, R-12 | Staged extract + atomic `mv` clean-replace; mutate-then-reinstall byte-compare; fixed durable dir asserted |
| SR-03 (build reproducibility) | R-02, R-14 | C-9 dependency-free build ⇒ byte-stable; NFR-1 re-run compare; zero-deps gate |
| SR-04 (vacuous verification) | R-01 | Real re-fired hook (`execFileSync`) + negative control proving the assertion is non-vacuous |
| SR-05 (matcher-narrowing delta) | R-09 | Assert PreToolUse equals imported `PRETOOLUSE_CYCLE_MATCHER`; runbook documents the delta |
| SR-06 (transient live-flip read as executed) | R-08 | tmpdir guard with negative test; pre/post suite hash of live settings; read-only access only |
| SR-07 (isolation = code-freeze vs state) | R-04 | Behavior-changing in-repo edit + installed byte/behavior invariance + non-symlink assertion; explicit negative assertion that state dirs are NOT separated (#4923) |
| SR-08 (fail-open / daemon-absent) | R-07, R-03 | Re-fire on scratch hash with no socket asserts exit-0; scratch hash proven distinct from live |
| SR-09 (init / size-gate regression) | R-14 | Existing init/merge tests green; size + zero-deps gates pass; no diff to frozen surfaces |

All nine SR-XX scope risks trace to at least one architecture risk. No scope risk is accepted/out-of-scope without a mitigating R-ID.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | 12 |
| High | 5 (R-05, R-06, R-07, R-08, R-11) | 15 |
| Medium | 6 (R-09, R-10, R-12, R-13, R-14, R-15) | 14 |
| Low | 0 | 0 |

Critical risks (vacuous verification, non-atomic replace, scratch-hash collision, weak isolation
proof) each require a *non-vacuous* proof — for R-01 and R-04 specifically, a negative control
that fails when the install/leak is broken is mandatory, not optional.

## Knowledge Stewardship
- Queried: context_search (category lesson-learned + pattern) for switchover/copy-install/effect-harness risks — most relevant: #4796 (gates asserting un-executed CI ACs as fact; macOS tmpdir-symlink state-dir split → informs R-03/R-08/R-11), #2928 (effect over string-diff, per-rule isolation → informs R-01/R-04), #4328 (npm package test hardcoded version → informs R-09 imported-constant guidance).
- Stored: nothing novel to store — the recurring pattern here (effect-harness must re-fire, not string-diff; deferred-action boundary must be guarded with a negative test) is already captured by #2928 and #4796; no 2+-feature pattern not already present.
