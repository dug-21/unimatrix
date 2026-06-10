# nan-016 Test Plan — OVERVIEW

> UDS dogfooding re-release capability + (delivered, unexecuted) switchover. Four components:
> `scripts/dogfood-install.sh`, `scripts/dogfood-switchover.sh`,
> `packages/unimatrix/test/dogfood-effect.test.js` (which IS the AC-02/AC-03 effect harness),
> `product/features/nan-016/RUNBOOK.md`. Tests root entirely in RISK-TEST-STRATEGY.md
> (15 risks, 4 Critical). Per-component files map 1:1 to the architecture Component Breakdown.

## Overall Test Strategy

nan-016 changes NO `lib/` code (C-8). It adds scripts, a runbook, and a `node --test` harness.
Therefore the test surface is:

1. **Effect/behavior tests** (the dominant surface) — the `node --test` harness
   (`dogfood-effect.test.js`) drives the *real* shell scripts against scratch fixtures and
   the *real* installed path, then asserts on resulting settings state and a *re-fired* hook's
   runtime behavior. There is **no string-diff of the scripts** anywhere (SR-04 / R-01).
2. **Regression gates** — existing `node --test` suites (`init.test.js`,
   `init-integration.test.js`, `merge-settings.test.js`, `init-remote.test.js`) plus the two
   gate scripts (`check-hook-client-size.js`, `check-zero-deps.js`) must stay green (R-14).
3. **Negative controls** — for the two Critical "vacuousness" risks (R-01, R-04), a test that
   FAILS when the install is broken / a leak exists is MANDATORY, not optional. A happy-path
   assertion alone does not satisfy these risks.

There is **no Rust/pytest surface** in nan-016 — no `crates/` or `lib/` logic changes. The
infra-001 pytest harness and `cargo test --workspace` are **not in scope** for this feature's
new tests (they remain available as untouched regression context only). "Integration" here
means the `node --test` harness + the existing `packages/unimatrix` `node --test` suite + the
size/zero-deps gate scripts. See "Integration Harness Plan" below.

### Test Naming & Structure

- Harness tests follow `node --test` `describe`/`test` blocks; names use
  `{component}_{scenario}_{expected}` phrasing, e.g.
  `promote_scratch-seeded-with-star_narrows-PreToolUse-to-imported-constant`.
- Every test is Arrange/Act/Assert. All fixtures under `os.tmpdir()`, `realpath`-resolved,
  cleaned up in `after`/`afterEach`. No flakiness: no network, no real daemon dependency
  (daemon-absent is asserted, not assumed).
- Shell-script behavior is asserted ONLY through `execFileSync` invocations + filesystem
  effects, never by reading/grepping the `.sh` source (R-01 guard).

## Risk-to-Test Mapping (from RISK-TEST-STRATEGY.md)

| Risk | Pri | Primary Component Test Plan | Test family (high level) |
|------|-----|------------------------------|--------------------------|
| R-01 vacuous verification | **Critical** | dogfood-effect | Real `execFileSync` re-fire of installed entrypoint **+ negative control** that fails on broken install |
| R-02 non-atomic / overlay replace | **Critical** | dogfood-install | mutate-then-reinstall byte-compare; staged-temp+atomic-`mv`; stray-file-gone |
| R-03 scratch-hash collision | **Critical** | dogfood-effect | scratch project-root hash asserted DISTINCT from live repo hash; no scratch socket; realpath-mirrors-config.js (tmpdir symlink guard, #4796) |
| R-04 weak isolation proof | **Critical** | dogfood-effect | behavior-changing in-repo edit (throwaway copy) + installed byte/behavior invariance + non-symlink **+ negative control** |
| R-05 mergeSettings require throws opaquely | High | dogfood-switchover | missing `--client` → loud actionable non-zero; completeness assert before require; harness skip-with-message |
| R-06 rollback drift | High | dogfood-switchover / dogfood-effect | promote→rollback round-trip reproduces exact Rust form over correct events; idempotent; shipped legacy arm |
| R-07 daemon-absent re-fire non-zero | High | dogfood-effect | re-fire on socket-less scratch hash exits 0; malformed/empty stdin exits 0; emitted form == `buildHookClientCommand` |
| R-08 live-settings breach | High | dogfood-effect | pre/post suite hash of live `.claude/settings.json` (untouched); tmpdir guard + negative guard test; live read-only |
| R-09 matcher delta not asserted / drifts | Medium | dogfood-effect | PreToolUse asserted == **imported** `PRETOOLUSE_CYCLE_MATCHER` (not a literal); runbook documents delta |
| R-10 event-set 8-vs-9 / dup / foreign | Medium | dogfood-effect | event count asserted against actual opt-in state; no-duplicate; foreign-hook preserved |
| R-11 npm pack drift / postinstall side-effect | High | dogfood-install | full `files`-array set present, binary ABSENT; postinstall inert (no ONNX/host mutation); mechanism is `npm pack` |
| R-12 container-rebuild non-durability | Medium | dogfood-install / dogfood-effect | fixed `~/.unimatrix/dogfood-client/` target; emitted command embeds fixed absolute path |
| R-13 in-repo edit not restored | Medium | dogfood-effect | working tree provably clean after isolation test, incl. failure paths (restore in teardown) |
| R-14 init / size / zero-deps regression | Medium | runbook / cross-cutting | existing init/merge tests green; size + zero-deps gates exit 0; no diff to frozen surfaces |
| R-15 before-hook install disturbs human install | Medium | dogfood-effect | suite installs only to test-scoped temp `--target`; real dogfood install hash unchanged |

### AC-to-Component coverage

| AC | Components carrying it | Critical risks anchored |
|----|------------------------|--------------------------|
| AC-01 idempotent copy-install | dogfood-install | R-02, R-11, R-12 |
| AC-02 switchover by effect | dogfood-switchover, dogfood-effect | R-01, R-03, R-07, R-08, R-09, R-10 |
| AC-03 isolation (code-freeze) | dogfood-effect | R-04, R-13 |
| AC-04 runbook + rollback | runbook, dogfood-switchover, dogfood-effect | R-06 |
| AC-05 init byte-identical | cross-cutting regression | R-14 |
| AC-06 size + zero-deps gates | cross-cutting regression | R-14 |

## Cross-Component Test Dependencies

- **dogfood-effect depends on dogfood-install.** The harness `before` hook runs
  `dogfood-install.sh --target <test-scoped temp dir>` so AC-02/AC-03 assert against a real
  frozen tree. If install is absent and cannot be staged, the harness **skips with a clear
  message** (R-05/R-15) — it never hard-crashes and never installs into the real
  `~/.unimatrix/dogfood-client/`.
- **dogfood-effect depends on dogfood-switchover.** AC-02/AC-04 drive the real `promote` /
  `rollback` subcommands; the harness is the executor that turns the switchover script's
  behavior into assertable effect.
- **Both switchover and rollback require the INSTALLED `lib/merge-settings.js`** (the
  integration seam) — so a complete install is a precondition for the switchover tests; the
  completeness assertion (R-05) gates this seam.
- **runbook (AC-04) cross-checks dogfood-effect's rollback test** — the runbook claims rollback
  restores the Rust form; the harness rollback test proves the claim by effect.

## Integration Harness Plan (Stage 3c)

nan-016's "integration" surface is entirely `node --test` + gate scripts. There is no
Rust/pytest surface (no `lib/` changes; infra-001 / `cargo test` are NOT exercised by new
nan-016 tests). Stage 3c MUST run, in order:

### A. New harness suite (the deliverable)
- **`node --test packages/unimatrix/test/dogfood-effect.test.js`** — the AC-02/AC-03/AC-04
  effect harness. New tests this file adds (none exist today):
  1. `install_*` precondition tests (or `before`-hook install into test-scoped temp `--target`).
  2. `promote_*` — repoint-by-effect: installed-path commands, **imported**
     `PRETOOLUSE_CYCLE_MATCHER`, foreign-hook survival, no duplicates, actual event count.
  3. `refire_*` — `execFileSync` real re-fire of installed entrypoint, exit-0/empty-stdout
     (daemon-absent), malformed-stdin exit-0.
  4. `refire_negative-control_*` — **broken install path makes the re-fire assertion FAIL**
     (proves non-vacuousness, R-01).
  5. `scratch-hash_*` — scratch root hash DISTINCT from live repo hash; no scratch socket;
     realpath-mirrors-config.js (R-03).
  6. `isolation_*` — installed byte/behavior invariance under a behavior-changing in-repo edit
     in a throwaway copy; non-symlink assertion; **negative control** that a leaked symlink
     install WOULD be detected (R-04); working tree clean in teardown (R-13).
  7. `rollback_*` — promote→rollback round-trip == exact Rust form, idempotent, foreign-hook
     preserved, shipped legacy arm (R-06).
  8. `live-settings_*` — tmpdir guard + negative guard test; pre/post suite hash of live
     settings unchanged (R-08).

### B. Existing regression suites (must stay green — R-14 / AC-05)
- `node --test packages/unimatrix/test/init.test.js`
- `node --test packages/unimatrix/test/init-integration.test.js`
- `node --test packages/unimatrix/test/merge-settings.test.js`
- `node --test packages/unimatrix/test/init-remote.test.js`
- (Or a single `node --test packages/unimatrix/test/` run covering all of the above + the new
  harness; per-file invocation acceptable for triage.)

### C. Gate scripts (must exit 0 — AC-06 / NFR-5 / NFR-6 / C-9)
- `node packages/unimatrix/test/check-hook-client-size.js` (size gate)
- `node packages/unimatrix/test/check-zero-deps.js` (zero-deps gate)

### D. Frozen-surface diff check (AC-05 / R-14)
- `git diff` proves zero behavioral change to `lib/init.js`, `lib/merge-settings.js`,
  `lib/hook-client/config.js`, `package.json` runtime. (nan-016 adds NO `lib/` bytes.)

### Gate semantics
- **Mandatory minimum gate:** the new harness suite green + both gate scripts exit 0 + existing
  init/merge suites green. A green harness with NO `execFileSync` re-fire and NO negative
  control is a FAILED gate (vacuous, R-01/R-04) even if assertions pass.
- **No integration test deleted/commented out.** No `xfail` markers expected (no pytest surface).
  If an existing init/merge test fails, triage per RISK-TEST-STRATEGY: a failure caused by
  nan-016 (an inadvertent frozen-surface edit) MUST be fixed in this feature; a pre-existing
  unrelated failure is documented, not fixed here.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-001..005 (#4924,
  #4925, #4926, #4928), #2928 (effect-over-string-diff / per-rule isolation), #4796
  (tmpdir-symlink state-dir split; un-executed-AC-as-fact), #4915 (manifest completeness needs
  code-derived cross-check). All applied to the negative-control and scratch-hash mandates.
- Stored: nothing novel at plan time — the load-bearing patterns (effect harness must re-fire
  not string-diff; deferred-action boundary needs a negative-guard test; matcher asserted
  against imported constant) are already captured by #2928 / #4796 / #4328. Stage 3c may store a
  reusable "scratch project-root hash isolation fixture" pattern if the implementation yields a
  generalizable helper.
