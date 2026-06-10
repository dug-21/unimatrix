# Agent Report — report-extensions (INTEGRATOR)

**Agent**: nan-018-agent-3-report-extensions
**Component**: ProfileResult plumbing + run-loop wiring (trust + cost into eval run + report)
**Commit**: 81921c1b `impl(report-extensions): wire trust + cost into run loop and report (#716)`

## 1. Files modified / created

Modified:
- `crates/unimatrix-server/src/eval/runner/output.rs` — `ScoredEntry.content` (carry-flag); `ProfileResult.cost_tokens` + `trust`; producer round-trip + backward-compat tests.
- `crates/unimatrix-server/src/eval/runner/trust.rs` — `TrustOutcome` now derives `Serialize/Deserialize` + `Default` (=trivial_pass); `scored()` test helper.
- `crates/unimatrix-server/src/eval/runner/cost.rs` — `payload_text` = `title + content`; module doc updated; new title+content payload test.
- `crates/unimatrix-server/src/eval/runner/replay.rs` — populate `content`; compute cost + trust in the same pass as P@5/MRR; thread `Option<&AliasMap>`.
- `crates/unimatrix-server/src/eval/runner/mod.rs` — pass `None` alias_map for JSONL (log-sourced) runs.
- `crates/unimatrix-server/src/eval/report/mod.rs` — report-side dual `TrustOutcome`; `cost_tokens`+`trust` on report `ProfileResult`; `content` on report `ScoredEntry`; `RegressionRecord` gains `reasons`, `trust_violations`, `cost_delta`; `Default` derives; `render_correlated` module decl.
- `crates/unimatrix-server/src/eval/report/aggregate/mod.rs` — removed `find_regressions`/`render_reason` (moved out); module decl + re-export.
- `crates/unimatrix-server/src/eval/report/render.rs` — call correlated section; import.
- `crates/unimatrix-server/src/eval/report/render_zero_regression.rs` — Δ Cost, Trust Violations, Reason Codes columns; advisory note.
- 7 report test fixture files (`tests*.rs`) + 2 runner test files — `..Default::default()` / explicit new fields.

Created:
- `crates/unimatrix-server/src/eval/report/aggregate/regression.rs` — `find_regressions` (OR-fold) + `render_reason` (split for 500-line limit).
- `crates/unimatrix-server/src/eval/report/render_correlated.rs` — `## 5C.` correlated trust/relevance/cost section (split for 500-line limit).

All files ≤500 lines (render.rs 499, aggregate/mod.rs 401 post-split).

## 2. Tests pass/fail

- Eval lib suite: **273 passed, 0 failed** (`cargo test -p unimatrix-server --lib eval::`).
- Full workspace: **3878 passed, 1 failed** — the single failure is `http::token::tests::test_concurrent_creation_no_corruption`, a pre-existing flaky concurrency test UNRELATED to nan-018 (passes in isolation; no eval/report/runner code involved).
- `cargo build --workspace --lib` clean; `cargo clippy` clean on all touched/new files; `cargo fmt` applied.
- Note: `cargo test --workspace` must be run with `CARGO_BUILD_JOBS=2` in this sandbox — the parallel `bin "unimatrix"` link step OOMs (`ld` signal 9) at full job count. This is an environment memory limit, not a code defect; lib+tests compile and pass.

New tests added (per test plan):
- trust-flip regression; no-flip; trust-repair-not-regression; OR-composition (trust-holds-MRR-regresses flagged; relevance-holds-trust-flips flagged); cost-growth advisory reported; cost-only advisory-only; cost-equal-not-growth; baseline-sorts-profile-keys (#2610).
- Pipeline: exit-code invariance with trust regression; exit-code invariance with cost growth; correlated-section surfacing (Trust+P@K+MRR+Cost, same scenarios); pre-nan-018 backward-compat deserialization (#3548 named test).
- Producer-side: `test_profile_result_cost_trust_roundtrip_nontrivial`, `test_profile_result_backward_compat_pre_nan018_json`, `test_payload_includes_title_and_content`.

## 3. Issues / blockers

- **ScoredEntry.content availability — RESOLVED, not a blocker.** The eval `ScoredEntry` build site in `replay.rs` has the full entry via `se.entry: EntryRecord`, which carries `content`. Populated from `se.entry.content.clone()`; cost is no longer under-counting (title+content). Confirmed the fixture loader also materializes `content` (loader.rs:350), so corpus-sourced results carry real body text.
- **alias_map threading — design note.** `run_eval`/`run_replay_loop` is JSONL-driven (log-sourced) and currently has no corpus `AliasMap` in scope. I threaded `Option<&AliasMap>` additively through `run_replay_loop → replay_scenario → run_single_profile`; the JSONL path passes `None` (⇒ trivial pass, since log-sourced scenarios carry no assertions). A future fixture-corpus run path (the AC-14 sweep harness) passes `Some(alias_map)` from `load_fixture_corpus`. No corpus-run entry point exists yet in `run_eval`; that wiring is a separate seam (corpus-loader/sweep harness owns it). This is the only place a downstream consumer must remember to pass the map — flagged for the sweep-harness author.
- **R-17 exit-code invariance preserved**: trust flips + cost growth are body-only; `run_report` still always returns `Ok(())` (verified by the two exit-code pipeline tests). The only non-zero-exit path (shape-hash hard abort) lives elsewhere.
- **Section-heading collision avoided**: used `## 5C.` (no period after 5) so the existing `matches("## 5.")` / `find("## 5. ...")` pipeline assertions stay green.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern #3582/#3604/#3574/#3555/#3529 — eval dual-type + render-extension conventions; #3574 dual-type constraint was load-bearing); decision search surfaced ADR-003 #4896 / ADR-004 #4898 / R-17 exit-code invariance. Applied the dual-type rule (two ProfileResult/ScoredEntry copies) and the render-extension parameter-passing convention.
- Stored: entry #4908 "Eval ProfileResult/ScoredEntry: dual-type field additions + section-heading substring trap" via /uni-store-pattern (topic unimatrix-server) — captures the dual-copy field-addition discipline, the named-backward-compat-test requirement (#3548), the explicit-Default-for-complex-types gotcha, and the `## 5.`-substring section-heading collision trap (invisible until a pipeline test fails).
