# Gate 3b Report: nan-018

> Gate: 3b (Code Review)
> Date: 2026-06-09
> Result: PASS
> Branch: feature/nan-018

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Engine penalty, trust, cost, shape, loader, sweep all match validated pseudocode + Integration Surface signatures |
| 2. Architecture compliance | PASS | ADR-001/002/003/004/005/006 all honored; component boundaries + module tree per §2/§5 |
| 3. Interface implementation | PASS | Exact names/types from Integration Surface §6 used (GraphPenaltyParams, graph_penalty_with, ExpectedAssertions, TrustOutcome, ShapeManifest) |
| 4. Test case alignment | PASS | Truth tables, dual-default triangulation, hash determinism, AC-14 capstone all present and green |
| 5. Code quality (compile/stubs/unwrap/500-line) | WARN | Compiles 0 errors; no new stubs/unwrap; loader.rs 551 / trust.rs 582 source-over-500 (test precedent on main: 687/720) |
| 6. Security | PASS | Path-traversal guard (safe_join), config range-validation, no hardcoded secrets, cargo-audit baseline pre-existing (no nan-018 regression) |
| 7. Knowledge stewardship | PASS | All 10 implementation agent reports carry complete ## Knowledge Stewardship blocks (Queried + Stored/nothing-novel-with-reason) |
| AC-13 HARD GATE | PASS | `git diff --name-only origin/main -- .claude/protocols/` is EMPTY; no eval-as-gate wiring |

**9/9 load-bearing invariants verified.** 7/7 check categories pass (one WARN on file-length, non-blocking).

---

## AC-13 Hard Gate (verified first)

`git diff --name-only origin/main -- .claude/protocols/` → **empty**. Zero protocol-file edits.
The drift guard (`shape/guard.rs`) hard-aborts only on primary-corpus shape mismatch — a corpus-*validity* precondition, not an eval-*results* standing gate. `report::run_report` always returns `Ok(())` for regressions (exit-0 convention preserved). No eval-execution-as-workflow-gate wiring added. **AC-13 PASS.**

---

## Load-Bearing Invariants (all 9 verified)

| # | Invariant | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Bit-for-bit default equivalence `graph_penalty == graph_penalty_with(&default) == const` | PASS | `graph.rs:536` thin wrapper; `GraphPenaltyParams::default()` references consts (`graph.rs:97-104`); engine tests `test_graph_penalty_with_default_equals_named_const_*` (orphan/clean/partial/dead_end) + `test_graph_penalty_is_thin_wrapper` all green |
| 2 | Clamp ceiling = `params.clean_replacement` (NOT const) | PASS | `graph.rs:623` `let ceiling = params.clean_replacement;` + sub-floor guard (no panic); test `test_graph_penalty_with_clamp_ceiling_tracks_swept_clean_replacement` + `test_graph_penalty_with_depth2_le_depth1_monotonicity` green |
| 3 | Dual-default discipline (serde fn ↔ Default impl ↔ engine const) | PASS | `config.rs` `default_*()` fns + `Default` impl resolve to consts; 7 triangulation tests `test_graph_penalty_config_dual_default_*_triangulates` all green (R-02) |
| 4 | Two penalty sites in `search.rs` ONLY; `background.rs` NOT threaded | PASS | `search.rs:744` (fallback) + `:747` graph_penalty_with — the only two; `background.rs:583` is a `tracing::error!` log string only; guards `test_enumerated_penalty_sites_route_through_config` + `test_background_rs_not_a_penalty_site` green (R-01) |
| 5 | Loader rejects literal-ID + null expected (primary corpus) | PASS | `loader.rs:188-192` `LiteralIdExpected`/`NullExpected` errors; tests `test_loader_rejects_literal_id_expected_primary`, `test_loader_rejects_null_expected_primary`, `test_primary_corpus_audit_zero_literal_id_zero_null` green (R-09) |
| 6 | rank-below B-absent⇒FAIL asymmetry | PASS | `trust.rs:164-170` `(Some(_), None) ⇒ rank_pass=false`; A-absent⇒pass (`:162`); redirect head-absent⇒fail (`:182`); 11 truth-table tests incl. `test_rank_below_b_absent_fail` green (R-11) |
| 7 | Hash determinism + severity split (primary HARD ERROR / snapshot WARN) | PASS | `hash.rs` structural ordering + fixed `{:02x}`/Display formatting (no Debug, no raw f64); `guard.rs:178-185` PrimaryFixture⇒HardAbort, ProductionSnapshot⇒warn+Ok; determinism/permute/cross-process/severity tests green (R-03/R-06) |
| 8 | Exit-code invariance R-17 (only shape-hash hard-abort exits non-zero) | PASS | `report/mod.rs:328` "Always returns Ok(()) — never exits non-zero due to regression count"; test `test_distribution_gate_exit_code_zero` green; shape abort is the lone non-zero path, on the run/sweep precondition not the report verdict |
| 9 | ADR-006 eval-only boundary (production `with_rate_config` stays default; lever live ONLY on eval path) | PASS | `services/mod.rs:358` production `ServiceLayer::new` passes `GraphPenaltyParams::default()`; `eval/profile/layer.rs:391` passes `graph_penalty.resolve_params()`; test `test_with_rate_config_default_resolves_to_const_params` green |

---

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status**: PASS
**Evidence**: Every component matches its validated pseudocode and the Integration Surface (§6) exactly:
- `graph_penalty_with(node_id, graph, entries, params: &GraphPenaltyParams) -> f64` — signature and branch structure identical to `graph_penalty`, every const replaced by `params.*` (graph.rs:559-632).
- Cost metric is the two-tier proxy the pseudocode + ADR-003 specify (faithful `tokenizers` subword + word×1.3 documented fallback; char/4 explicitly rejected; payload = `title + content`; `OnceLock` load-once determinism; tier logged). `cost.rs` matches `pseudocode/cost-metric.md` line-for-line in structure.
- Trust evaluator lowers `ExpectedAssertions` into an `Assertion` enum (3 Wave-1 variants, no speculative types) — the metric *class* design from ADR-004.
- Shape manifest is the ordered/versioned/enumerated form ADR-002 specifies.

### Check 2 — Architecture compliance
**Status**: PASS
**Evidence**: Component breakdown (§2) and integration points (§5) realized as designed. The `tokenizers = "0.21"` dependency added to unimatrix-server is authorized by ADR-003 ("already in the embed dependency tree for all-MiniLM") — not a scope addition. The AC-14 capstone (`sweep.rs`) is a legitimate integration spine wiring drift-guard-on-load + alias-map threading + live lever, beyond the original Component Map but necessary to make AC-14 achievable (per spawn note) and faithful to §3.7 Wave-1 exit semantics.

### Check 3 — Interface implementation
**Status**: PASS
**Evidence**: Exact Integration Surface types used downstream without invention: `GraphPenaltyParams` (7 fields, Copy, Default=consts), `graph_penalty_with`, `UnimatrixConfig.graph_penalty: GraphPenaltyConfig` (`#[serde(default)]` + `multiplier: Option<f64>`), `ExpectedAssertions { redirect_to_head, forbidden_absent, rank_below }` with `EntryRef = String`, `TrustOutcome { absence_pass, rank_pass, violations }`. Embedding identity read live from `EmbeddingModel::default()` (R-05 — not literal-embedded; test `test_shape_hash_reads_embed_model_live_not_literal` green).

### Check 4 — Test case alignment
**Status**: PASS
**Evidence**: Risk-Strategy scenarios are exercised: R-01 default-equivalence + enumerated-site + empty-TOML; R-02 7-lever triangulation; R-03 in-process/permuted/cross-process hash determinism; R-06 deliberate-mismatch + message + severity-split; R-09 loader rejection + corpus audit; R-10 renumber-survival + missing/duplicate alias; R-11 full per-property truth tables; R-13 multiplier severity-only + precedence; R-15 `test_ac14_correlated_sweep_non_vacuous` (5 conditions) + each-shape-exercised; R-17 exit-code invariance. AC-14 proof passes.

### Check 5 — Code quality
**Status**: WARN (non-blocking)
**Evidence**:
- Build: `cargo build --workspace` → 0 errors (25 pre-existing dead-code warnings, none nan-018-introduced behavior; not `-D warnings` failures in this build).
- Anti-stub: zero `todo!()`/`unimplemented!()`/`FIXME`/new `TODO` in nan-018 source. The single `TODO(W2-4)` in `services/mod.rs:260` is **pre-existing on origin/main** (confirmed), not introduced here.
- `.unwrap()`: zero in nan-018 non-test source (trust.rs deliberately keeps `resolve -> Option` and handles `None` loudly per no-unwrap rule).
- Full suite: `cargo test -p unimatrix-server --lib` → 3879 passed. The lone failure `http::token::tests::test_concurrent_creation_no_corruption` is a **pre-existing flaky concurrency test in `http/token.rs` (NOT touched by nan-018)** — passes deterministically in isolation (re-ran: rc=0). Engine penalty + config + trust + shape + corpus + AC-14 suites all green in isolation.
**WARN issue**: `loader.rs` (551 source lines, no inline tests) and `trust.rs` (582 total; ~248 source + 334 inline test) exceed the 500-line workspace guideline. Pure-test file `shape/tests.rs` is 578. This is a soft WARN, not a FAIL: the project already ships test files over 500 on main (`scenarios/tests.rs` 687, `report/tests_distribution_gate.rs` 720, `tests_metrics.rs` 518→523), establishing that the rule is not strictly enforced on test-bearing files. `loader.rs` at 551 source lines is the one genuine source-over-500 — a candidate for a follow-up split (e.g. extracting the materialize-DB step), but it does not block the gate.

### Check 6 — Security
**Status**: PASS
**Evidence**:
- Path traversal: `assertions::safe_join` rejects absolute/`..` author-supplied file references; test `test_corpus_fixture_path_traversal_rejected` green. Loader materializes DB only under caller-controlled `target_db`.
- Input validation: `validate_graph_penalty` rejects NaN/non-finite, out-of-`[0,1]` severities, zero `max_traversal_depth`, multiplier outside `(0,1]` — tests green.
- No hardcoded secrets in nan-018 source.
- Deserialization: malformed manifest version → `UnknownManifestVersion` error (no silent mis-hash, no panic).
- `cargo audit`: 1 vuln (`RUSTSEC-2023-0071` rsa Marvin Attack, "no fixed upgrade available") + unmaintained-crate warnings — all **transitive and pre-existing** in the workspace baseline, unrelated to nan-018. nan-018's only Cargo.lock change is `+tokenizers` (already in embed tree), introducing no new advisory.

### Check 7 — Knowledge stewardship
**Status**: PASS
**Evidence**: All 10 implementation agent reports (engine-penalty, corpus-loader, penalty-config, cost-metric, search-threading, shape-hash, trust-metric, report-extensions, docs, ac14-harness) contain a `## Knowledge Stewardship` block with `Queried:` evidence and `Stored:`/`nothing novel to store -- {reason}` entries. "nothing novel" entries carry reasons (instances of already-recorded #4064/#4070/ADR-001) — no bare WARN.

---

## Rework Required

None. The single WARN (loader.rs 551 source lines) is advisory; a follow-up split is recommended but does not block Wave-1 exit.

---

## Notes for Delivery

- **R-04 named-human-delivery-gate (ARCHITECTURE §7.3 / AC-08f)**: column-manifest completeness must be certified by a **named human reviewer** confirming no retrieval-relevant entry column was mis-classified as display-only. This is explicitly NOT closable by automated tests or routine code review — flagged here so the coordinator routes it to the human before delivery acceptance. The `test_shape_hash_insensitive_to_display_only_column` + per-input sensitivity tests prove sensitivity to the *declared* set only; the *completeness* of that set is the human's call.
