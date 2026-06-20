# Gate 3c Report: vnc-041

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-14) | PASS | RISK-COVERAGE-REPORT maps all 14 risks to passing tests; verified bins 123/0 + vnc-041 lib subset all green |
| 2. Test coverage completeness | PASS | All Phase-2 risk→scenario mappings exercised; integration regression net (smoke/protocol/lifecycle) run per plan |
| 3. Specification compliance | PASS | AC-01..AC-06 all PASS; FR/NFR addressed; no scope additions |
| 4. Architecture compliance | PASS | Component structure (C1..C5) + ADR-001..005 honored in code; http.enabled gate + single per-slug join site confirmed |
| 5. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT has `## Knowledge Stewardship` with Queried: + Stored:/nothing-novel-with-reason |
| INTEGRATION — smoke gate | PASS | 24/24 smoke recorded; suites unchanged, no tests deleted/commented |
| INTEGRATION — xfail hygiene | PASS | All xfail markers reference GH Issues (#111/#405/#406); all PRE-EXISTING; vnc-041 added none |
| JUDGMENT — R-05/AC-02 substitute | PASS | Behavioral equivalence genuinely holds (see Detailed Findings) |

Checks: 8/8 PASS. 0 WARN. 0 FAIL.

## Detailed Findings

### 1. Risk mitigation proof (R-01..R-14)
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md table (lines 27-41) maps every risk to named
passing tests. Verified empirically this session:
- `cargo test -p unimatrix-server --bins` → 123 passed, 0 failed (matches report).
- `cargo test -p unimatrix-server --lib` → vnc-041 subset all green (spot-checked
  `test_write_if_absent_*` x6, `test_render_legend_flips_when_disposition_flips`,
  `test_register_seeded_b_path_is_the_resolver_formula`,
  `test_register_seeded_b_is_resolver_loadable`, `test_resolve_warns_*` — all `... ok`).

### 2. Test coverage completeness
**Status**: PASS
**Evidence**: All Critical (R-01..R-05), High (R-06..R-08), Medium (R-09..R-14), and Low
(R-11) scenarios from RISK-TEST-STRATEGY.md have corresponding tests. Cross-component /
integration risks covered: register→resolver path equality (R-05), two-writer non-collision
(R-05, byte-unchanged (a)/(c)), dual-(a)-writer idempotency (R-11), A→B registry binding via
the flip test (R-04/R-06). The infra-001 suites ran as a no-regression net: vnc-041 adds
filesystem provisioning + a `tracing::warn` only — no MCP tool/parameter/schema/resolution
change — so per the plan no new integration tests were required.

### 3. Specification compliance
**Status**: PASS
**Evidence**: ACCEPTANCE-MAP AC-01..AC-06 all PASS with named tests (RISK-COVERAGE-REPORT
lines 152-161). No new config knob/section (NFR-02 honored — confirmed: no `UnimatrixConfig`
surface change in the diff). Workspace rules (NFR-07): no `.unwrap()` in vnc-041 non-test
code; `write_per_slug_seed`/`warn_locked_keys` are infallible-by-design (best-effort, no
error propagation). No vnc-041-introduced stubs/TODO (the two `TODO(W2-4)` in main.rs:855/1551
predate this feature — present at base b210c9f7, authored 2026-03; out of scope).

### 4. Architecture compliance
**Status**: PASS
**Evidence**: Code matches ARCHITECTURE §3-§7 and all five ADRs:
- ADR-004 (http.enabled gate): global seed `write_default_config_if_absent(...)` is at
  main.rs:1023, lexically INSIDE the `if config.http.enabled` block (main.rs:1011). Local
  `else` branch has no seed call. Gate is `http.enabled`, NOT `base_dir` — confirmed.
- ADR-002 (per-slug seed in register, both states): `write_per_slug_seed` called at
  projects.rs:313 (State C genesis) AND projects.rs:352 (State B re-attach); writes ONLY (b).
- ADR-001 (no-clobber primitive): `config::write_if_absent` (create_new), single-sourced.
- ADR-005 (WARN derives from registry): `warn_locked_keys` (http_provision.rs:399) iterates
  present keys and gates on `!is_per_slug_overlayable(key)` — no hand-list; content-free
  (key + slug only, never the value).
- Single per-slug join site (SR-09): `per_slug_data_dir(&self.base_dir, slug).join(PROJECT_CONFIG_NAME)`.

### 5. Knowledge stewardship compliance
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md §Knowledge Stewardship (lines 168-177) has a
`## Knowledge Stewardship` block with `Queried:` (context_briefing + context_get(5241) —
ADR-005 #5239, resolve_slug_config gotchas #5212, bin-vs-lib test trap #5241) and `Stored:`
("nothing novel" with a concrete reason — the deferred-round-trip infeasibility is a
feature-specific instance of the already-stored #5241 pattern).

### INTEGRATION TEST VALIDATION
**Status**: PASS
- **Smoke (mandatory gate)**: RISK-COVERAGE-REPORT records 24/24 smoke pass. Suites present
  with `@pytest.mark.smoke` markers across protocol/lifecycle/edge_cases/tools/etc.
- **Relevant suites run**: smoke + protocol + lifecycle, exactly the regression scope the
  plan (infra-001) prescribed for a no-MCP-surface feature.
- **No tests deleted/commented**: `git diff b210c9f7..HEAD -- product/test/infra-001/suites/`
  is EMPTY — vnc-041 made zero changes to any integration suite.
- **Integration counts in report**: yes (RISK-COVERAGE-REPORT lines 73-92: 24 smoke,
  87 protocol/lifecycle, 5 xfailed, 2 xpassed).
- **xfail hygiene**: every `@pytest.mark.xfail` references a GH Issue (#111, #405, #406, and
  the multi-line markers). All are PRE-EXISTING (authored for col-028/bugfix-491, not vnc-041);
  confirmed vnc-041 added none. The 2 XPASS are pre-existing timing-sensitive markers, correctly
  flagged for the human's periodic xfail review and correctly left untouched in a feature PR
  that did not author them.

### JUDGMENT CALL — R-05 / AC-02 empirical round-trip substitute
**Status**: PASS (substitute accepted — equivalence genuinely holds)
**Assessment**: The end-to-end `register → resolve_slug_config` call is genuinely infeasible
from a test-only scope on this branch:
- `resolve_slug_config` is a binary-target symbol (`mod http_provision` declared in main.rs
  only) — unreachable from the lib-crate `projects/tests.rs` where the private `register` lives.
- `register` + `with_dirs` are `#[cfg(test)]`-private — inactive when the lib compiles as a
  bin-target dependency, so the bin-target module that CAN reach the resolver cannot call them.
- The only `pub` entry (`run_project_command`) is HOME-keyed — HOME isolation is forbidden
  under Rust 2024.
Closing it would require a production-visibility edit (widening `with_dirs` to a `test-support`
cfg), which exceeds the 3c test-execution mandate. The deferral is justified and a post-PR
follow-up is planned.

**Why the substitute is equivalent (verified, not taken on faith)**: The resolver's two
load-bearing behaviors are (i) the probe-path derivation and (ii) the file-present load arm.
I verified both halves against source:
- Probe path: `http_provision.rs:318` computes `base_dir.join(slug.as_str()).join(PROJECT_CONFIG_NAME)`.
  Register writes (b) at `per_slug_data_dir(&self.base_dir, slug).join(PROJECT_CONFIG_NAME)`
  (projects.rs:370), where `per_slug_data_dir(base, slug) == base.join(slug.as_str())`. These
  are the SAME join. `test_register_seeded_b_path_is_the_resolver_formula` asserts the seed
  lands at `fx.base_dir.join("alpha").join("config.toml")` — byte-identical to the resolver
  formula — against the file `register` actually wrote (zero hand-placement).
- File-present arm: `test_register_seeded_b_is_resolver_loadable` reproduces the resolver's
  exact arm (`load_single_config → validate_config → merge_configs → validate_config`, the
  same four `pub infra::config` calls in the same order) on the register-written (b), proving
  it parses, per-file-validates, merges, and post-merge-validates with no error.
- `resolve_slug_config` itself is independently/empirically covered against real on-disk (b)
  files by slug_config_tests.rs + per_slug_loop_tests.rs (verified passing in the bins run).
The substitute proves register writes the seed at the exact path the resolver reads, with a
resolver-loadable body. The (a)≡(c) vs (b) confusion risk (R-05) is genuinely mitigated.
R-05 / AC-02 coverage is Full.

### Pre-existing flaky test (NOT a vnc-041 regression)
**Status**: noted, non-blocking
**Evidence**: My `--lib` run surfaced ONE failure — `eval::corpus::fixtures_tests::
test_ac14_scenario_search_returns_non_empty_ranked_list` (a DIFFERENT test than the
`sweep_tests::test_ac14_correlated_sweep_non_vacuous` the report named, but the SAME
parallelism-sensitive AC14 eval flake class). Confirmed:
- Passes in isolation: `cargo test -p unimatrix-server --lib <filter>` → 1 passed.
- Lives in `eval/` — `git diff b210c9f7..HEAD -- crates/unimatrix-server/src/eval/` is EMPTY;
  vnc-041 made zero changes to that module.
This is a pre-existing host/parallelism flake, NOT a vnc-041 regression. All 54 vnc-041 tests
and all 123 bin tests pass. Clippy on the server crate (`--lib --bins -D warnings`) is clean.
The report's flake attribution conclusion (pre-existing, not vnc-041) holds; only the specific
named test differed across runs, which is expected for a parallelism-order-sensitive flake.

## Rework Required

None.

## Scope Concerns

None.
