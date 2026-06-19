# Gate 3c Report: crt-056

> Gate: 3c (Final Risk-Based Validation) — RE-VALIDATION after REWORKABLE FAIL
> Date: 2026-06-19
> Result: **PASS**
> Validator: crt-056-gate-3c-rework · Branch `feature/crt-056` · HEAD `1c670799`

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | All 12 risks now proven by passing tests. R-05 (High) AC-1 and the AC-2 half of R-12 are now closed with real, non-vacuous tests (prior FAIL closed). |
| 2. Test coverage completeness | PASS | AC-1 8-field field-by-field parity test and AC-2 `Arc::ptr_eq` shared-model test exist, are non-vacuous, and assert the FULL checklist. RISK-COVERAGE-REPORT rows cite them truthfully. |
| 3. Specification compliance | PASS | "Sole tick path" claim corrected to "sole ON THE DAEMON path"; stdio single-store carve-out documented consistently across code + both docs (prior WARN closed). |
| 4. Architecture compliance | PASS | ADR-001..006 faithfully implemented; no drift (re-confirmed). |
| 5. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT `## Knowledge Stewardship` has `Queried:` + `Stored:` (#5175). |

**Both prior findings are genuinely closed. Gate PASSES.**

## Re-validation of the two prior findings

### Prior FAIL (Check 2 — coverage): AC-1/AC-2 vacuous green → CLOSED

Two real tests added (`project_routing_integration.rs`, commit `1c670799`); both verified to exist, run green (21/21 in the binary), and assert the full checklist:

**`test_per_slug_service_layer_config_parity_8_fields` (AC-1)** — builds a per-slug `ServiceLayer` via the literal `build_project_server` assembly (`ServiceLayer::new(<threaded resolved Arcs>)` → `UnimatrixServer::new(.., Some(layer))`) from an NLI-ENABLED, NON-DEFAULT `ResolvedDaemonConfig`. Asserts all 8 parity fields field-by-field — NOT a subset:
1. `nli_enabled()` true (and a second disabled-config build asserts `false` — both directions);
2. `nli_top_k()` == 37 (resolved non-default, not the 20 default);
3. `nli_handle()` `Arc::ptr_eq` the daemon's;
4. `fusion_weights()` == resolved value AND `!= FusionWeights::default()`;
5. `confidence_params()` `Arc::ptr_eq` + value `!=` default (alpha0=7.0/beta0=9.0);
6. `category_allowlist()` `Arc::ptr_eq` + operator-only-category present;
7. `observation_registry()` `Arc::ptr_eq`;
8. `ml_inference_pool().pool_size()` == 5 (not the size-1 test default).
`session_capabilities` correctly NOT asserted (ADR-006). `boosted_categories` operator hint also asserted.

**Non-vacuity VERIFIED against source, not just claimed.** The test-default `None` arm (`server.rs:323-341`) builds `RayonPool::new(1, "test-pool")`, fresh `NliServiceHandle::new()`, `nli_top_k: 20`, `nli_enabled: false`, and default fusion/confidence. Every one of the 8 assertions would FAIL against that fallback (pool 5≠1, top_k 37≠20, enabled true≠false, `Arc::ptr_eq` to a fresh handle fails, non-default weights/params ≠ defaults). The mutation claim (fallback `nli_top_k`=20 ⇒ RC=101) is structurally sound. Accessors (`services/mod.rs:333-376`) delegate to the `search`/`status` sub-services built from the threaded params — they read the threaded config, not hardcoded values.

**`test_shared_nli_model_across_n2_slugs` (AC-2)** — N=2 per-slug servers from the SAME `cfg`; each slug's `nli_handle` is `Arc::ptr_eq` to the daemon's ONE handle, the two slugs are `Arc::ptr_eq` to each other, and `Arc::strong_count >= 3` (cfg + 2 ServiceLayers). The shared Arc IS the proof no per-slug `NliServiceHandle::new()` runs.

**RISK-COVERAGE-REPORT truthful:** AC-1/AC-2 rows (Coverage table R-05/R-12, AC table lines 154-155, new-tests table lines 98-99) cite the two tests by name with accurate descriptions.

**Embedding-share residual — honest and acceptable.** Gap 4 labels the embedding-model share "Partial (structural)": the model-free harness constructs a fresh per-slug `EmbedServiceHandle::new()` (no ONNX load), so there is no single daemon embed Arc to compare in-test. Production `build_project_server` threads the daemon's ONE `embed_handle` `Arc::clone` to every slug (source-confirmed) — the same shared-Arc shape the NLI test proves behaviorally. This is an honest, bounded residual (a model-loaded multi-slug embed assertion is the same deferred infra enhancement as the AC-5 search-delta), NOT a hidden gap. The model-sharing INVARIANT (one model in memory) is the shared-Arc shape, demonstrated for NLI and source-confirmed for embedding. Accepted.

### Prior WARN (Check 3 — accuracy): "sole tick path" claim → CLOSED

Verified accurate across code + both docs:
- **Code (`9ccde2a9`):** `tick_loop.rs:11-24` — global-handle `spawn_background_tick` "**RETIRED on that daemon path**"; explicit stdio single-store carve-out documented. `main.rs:1197-1200` — "SOLE tick path ON THIS multi-project HTTP daemon path; the legacy global-handle spawn_background_tick is retired here. The stdio... carve-out." `main.rs:1598` carve-out comment.
- **Actual call sites confirmed:** daemon path `main.rs:1216` calls `spawn_per_slug_tick`; stdio path `main.rs:1606` calls the legacy `spawn_background_tick` — exactly what the corrected docs describe.
- **RISK-COVERAGE-REPORT §Tick-path (lines 131-148):** daemon RETIRED / stdio accepted carve-out, with the N≥2-required corruption-hazard rationale.
- **wave2-gating-audit §Scope correction (lines 139-148):** Part-B "SOLE / removed entirely" language scoped to the daemon path; stdio carve-out called out as accepted, not a Part-B violation.

All three artifacts are now consistent and accurate.

## Re-confirmation of still-solid items

- **AC-4 N=2 corruption guard** — `test_tick_b_leaves_a_unchanged_n2` ★ intact + non-vacuous (A=7/B=3 distinct populations, byte-for-byte four-state snapshot both directions; a global-handle write would flip the equality). `test_distinct_state_survives_empty_b_tick` intact. PASS.
- **AC-3/5/6/7 + AC-harness + AC-wave2-gate + AC-7-stepb** — all intact, source-confirmed. AC-5 search-delta altitude gap remains adequately covered (handle-identity `Arc::ptr_eq` + model-free serving-accessor read; `search()` is model-bound + crate-private). PASS.
- **ADR-001..006** — faithfully implemented, no drift. PASS.
- **A2 (#5171)** — correctly scoped as a Step-B precondition (IMPLEMENTATION-BRIEF HQ-2 ACCEPTED), not a crt-056 gap. PASS.
- **Smoke gate** — RISK-COVERAGE-REPORT records 24 passed / 0 failed (release build, ONNX dylib). Accepted as reported (not re-run; 208s + release build).
- **No integration tests deleted** — `git diff 29585e14 HEAD` on `tests/` shows 808 insertions, 0 deletions; no `xfail`/`#[ignore]` added. PASS.

## Test execution

`cargo test -p unimatrix-server --jobs 1` (hardened `setsid -w` + ceiling + file-not-pipe form):
- **Run 1 (full suite):** `project_routing_integration.rs` **21/21 PASS** including both new parity tests; all crt-056 background/tick tests green.
- **Run 2 (full suite):** one lib test failed — `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` (RC=101).

**The lib failure is the documented pre-existing search-ranking eval flake, NOT a crt-056 regression:**
- crt-056 touched ZERO eval files (`git diff 4ac65254^ HEAD -- src/eval/` is empty); it adds no scoring math and no eval scenarios.
- The failing test PASSES in isolation (`cargo test ... test_ac14_correlated_sweep_non_vacuous` → RC=0). It is an order/parallelism-dependent, embedding-driven eval flake — the exact "known search-ranking eval flake (passes in isolation)" the tester documented and that prior sessions (Memory) flagged as a known flake class.
- It did not appear in the smoke selection (tester's report) nor in Run 1.

This flake is a pre-existing condition unrelated to crt-056's per-slug tick contract and does not block the gate. (It is, separately, a candidate for a tracked flake-quarantine issue outside this feature — not a crt-056 obligation.)

## Detailed Findings

### 1. Risk mitigation proof — PASS
All 12 risks (R-01..R-12) map to passing tests. The prior High-priority gap (R-05 config-parity assertion; AC-2 half of R-12 shared model) is closed by the two new non-vacuous tests. R-04 (A2) correctly a Step-B precondition.

### 2. Test coverage completeness — PASS
AC-1 8-field field-by-field test and AC-2 `Arc::ptr_eq` shared-model test exist, run green, assert the FULL checklist (not a subset), and are non-vacuous (would fail against the test-default `None` arm). The vacuous-green anti-pattern (#4202/#3935) is resolved. RISK-COVERAGE-REPORT cites the real tests truthfully.

### 3. Specification compliance — PASS
FR-1..FR-18 implemented. NFR-5/C-6 "one isolation seam" claim corrected: scoped to the daemon path, with the stdio single-store carve-out documented consistently. The corruption hazard NFR-5 guards against requires N≥2 sharing global handles — impossible on the stdio single store; the carve-out is genuinely safe and honestly labeled.

### 4. Architecture compliance — PASS
ADR-001 (additive `Option<ServiceLayer>`), ADR-002 (params-at-end threading), ADR-003 (`ServiceLayer` owns sole handle set; ctx borrows `Arc::clone`s), ADR-004 (`BackgroundJob` seam shape-only), ADR-005 (serial loop, per-slug counter), ADR-006 (`adapt_service` per-slug, `session_capabilities` OUT) — all faithfully implemented. New crt-056 files under 500 lines.

### 5. Knowledge stewardship — PASS
RISK-COVERAGE-REPORT `## Knowledge Stewardship` has `Queried:` (context_briefing → #5147/#4202/#3935/#724/#4258) and `Stored:` (#5175 config-parity test technique + the multi-slug `TickTestHarness` pattern). Both present with reasons.

## Notes
- Both prior findings are genuinely and verifiably closed. The two new tests are real, run green, assert the full checklists, and are provably non-vacuous against the actual test-default fallback.
- The Run-2 lib failure is a pre-existing, isolation-passing eval flake in code crt-056 never touched — it does not implicate the feature and does not block the gate.
