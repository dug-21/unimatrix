# Gate 3c Report: crt-045

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-03
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | RISK-COVERAGE-REPORT.md maps all 10 risks; 8 automated, 1 accepted (R-07), 1 manual (R-09) |
| Test coverage completeness | PASS | All 4 non-negotiable scenarios confirmed present and passing |
| Specification compliance | PASS | All 8 FRs implemented; manual ACs (AC-02, AC-04) correctly deferred per spec |
| Architecture compliance | PASS | Implementation matches architecture exactly; no drift |
| Knowledge stewardship compliance | PASS | Tester agent report has Queried and Stored entries with reasons |
| Integration smoke gate | PASS | 22/22 smoke tests passed; no xfail markers; no deleted tests |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md provides risk-to-test mapping for all 10 risks.

- R-01 (post-construction write-back propagation): covered by `test_from_profile_typed_graph_rebuilt_after_construction` layer 1 + layer 3. Both assertions confirmed passing.
- R-02 (wired-but-unused): covered by three-layer assertion (AC-06 + ADR-003). All three layers pass: handle state (`use_fallback == false`, `all_entries.len() >= 2`), graph connectivity (`find_terminal_active` returns `Some(id_a)`), and live search call returns `Ok` or `EmbeddingFailed`.
- R-03 (vacuous quarantined-entry fixture): C-09 complied with — `seed_graph_snapshot()` inserts two `Active` entries with `CoAccess` edge (`bootstrap_only=0`). Confirmed at `layer_graph_tests.rs:65–83`.
- R-04 (rebuild error aborts from_profile): `test_from_profile_returns_ok_on_cycle_error` passes. Uses `entries.supersedes` UPDATE to create A→B→A cycle (matching Pass 2a cycle detection in `build_typed_relation_graph()`). Asserts `result.is_ok()` and `guard.use_fallback == true`.
- R-05 (TOML parse failure): `test_parse_no_distribution_change_flag` passes. `ppr-expander-enabled.toml` confirmed correct with `distribution_change = false`, `mrr_floor = 0.2651`, `p_at_5_min = 0.1083`, and explanatory comment.
- R-06 (baseline regression): All 9 pre-existing `layer_tests.rs` tests and 27 `eval::profile::tests` pass unchanged. Confirmed by direct test run.
- R-07 (rebuild hang): Accepted residual risk per SPECIFICATION.md. sqlx query timeout is the implicit guard. No test needed; documented in architecture.
- R-08 (accessor visibility): `pub(crate)` confirmed at `layer.rs:452`. Compile-time enforcement; no runtime test needed.
- R-09 (mrr_floor drift): Manual pre-merge verification required per RISK-TEST-STRATEGY.md. Correctly deferred; not automatable without live populated snapshot.
- R-10 (write-back race): Covered incidentally by `test_from_profile_typed_graph_rebuilt_after_construction`; architecture establishes no concurrency is possible in `from_profile()`.

**Integration risks IR-01 through IR-04** are documented; IR-01 is addressed in ADR-001, IR-02 and IR-03 are pre-existing known risks, IR-04 was resolved by using direct node count assertion when `find_terminal_active` is used via `graph connectivity` layer.

---

### Check 2: Test Coverage Completeness

**Status**: PASS

**Evidence**: All four non-negotiable scenarios from RISK-TEST-STRATEGY.md Coverage Summary confirmed present and passing:

| Scenario | Test Function | Confirmed |
|----------|--------------|-----------|
| `use_fallback == false` AND `typed_graph` non-empty with Active-entry + edge snapshot | `test_from_profile_typed_graph_rebuilt_after_construction` | Yes — `cargo test --list` output confirms function name; test passes |
| Live `search()` returns `Ok` or `EmbeddingFailed` on graph-enabled layer | `test_from_profile_typed_graph_rebuilt_after_construction` (layer 3) | Yes — CI-compatible assertion, `EmbeddingFailed` accepted |
| `Ok(layer)` on cycle-detected rebuild error with `use_fallback == true` | `test_from_profile_returns_ok_on_cycle_error` | Yes — function confirmed present; passes |
| All existing tests pass unchanged | 9 layer_tests + 27 profile unit tests | Yes — 38/38 eval::profile tests pass |

Three-layer ADR-003 assertion is fully implemented in `layer_graph_tests.rs:92–163`.

Workspace test totals: 4,426 passed, 0 failed, 28 ignored. This matches the RISK-COVERAGE-REPORT.md claim exactly.

Integration smoke gate: 22/22 passed (`python -m pytest suites/ -v -m smoke --timeout=60`). No xfail markers exist in the integration test suite. No integration tests were deleted or commented out. The report correctly explains why no additional infra-001 suites were selected: crt-045 changes are not observable through the MCP JSON-RPC interface.

---

### Check 3: Specification Compliance

**Status**: PASS

**Evidence**:

- **FR-01** (`TypedGraphState::rebuild` called after store open, before `with_rate_config`): Implemented at `layer.rs:188`. `.await` used directly, no `spawn_blocking`. Constraint C-01 satisfied.
- **FR-02** (post-construction write-lock swap via `typed_graph_handle()`): Implemented at `layer.rs:389–395`. `inner.typed_graph_handle()` returns the shared Arc; write lock acquired and released immediately after swap.
- **FR-03** (rebuild error → `tracing::warn!`, `use_fallback=true`, `Ok(layer)`): Implemented at `layer.rs:199–215` (match arm sets `None`) and `layer.rs:397` (`Ok(EvalServiceLayer {...})`). Tested by `test_from_profile_returns_ok_on_cycle_error`.
- **FR-04** (`tracing::info!` on successful rebuild): Implemented at `layer.rs:192–196` and `layer.rs:394`. NFR-04 satisfied (`info!` on success, `warn!` on failure).
- **FR-05** (`typed_graph_handle()` accessor, `pub(crate)`): Implemented at `layer.rs:452–454`. Delegates to `self.inner.typed_graph_handle()`. Mirrors `embed_handle()` and `nli_handle()` pattern. C-04 and C-08 satisfied.
- **FR-06** (`ppr-expander-enabled.toml` fixed): Confirmed. File at `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` has `distribution_change = false`, `mrr_floor = 0.2651`, `p_at_5_min = 0.1083`, `ppr_expander_enabled = true`, and an explanatory comment. Specification's required comment preventing silent future regression is present.
- **FR-07** (integration test in `layer_graph_tests.rs`): Both `test_from_profile_typed_graph_rebuilt_after_construction` and `test_from_profile_returns_ok_on_cycle_error` present and passing. All sub-requirements of FR-07 satisfied: two Active entries, one S1/S2/S8 (CoAccess) edge, `from_profile()` called, `use_fallback == false`, `typed_graph` non-empty (via `all_entries.len() >= 2` and `find_terminal_active`), live search invoked.
- **FR-08** (existing tests continue to pass): 38/38 eval::profile tests pass.

**Non-functional requirements**:
- NFR-01 (performance): No timeout introduced; sqlx implicit guard accepted per specification.
- NFR-02 (memory): Single allocation, immediate swap, prior value dropped.
- NFR-03 (concurrency): Write lock released with explicit `drop(guard)` at `layer.rs:393`. Lock held only for duration of swap.
- NFR-05 (API stability): `ServiceLayer::with_rate_config()` signature unchanged; `SearchService` fields unchanged; result types unchanged. Confirmed by compilation and all existing tests passing.
- NFR-06 (test suite): `cargo test --workspace` — 4,426 passed, 0 failed.

**Deferred ACs (not failures)**:
- AC-02 (manual eval run, live snapshot) — correctly deferred per specification; no automated path.
- AC-04 (baseline regression, live snapshot) — correctly deferred per specification; pre-existing tests serve as automated proxy.

---

### Check 4: Architecture Compliance

**Status**: PASS

**Evidence**:

- **Component boundaries**: Fix is single-file (`eval/profile/layer.rs`) plus new test file (`eval/profile/layer_graph_tests.rs`). No changes to `services/mod.rs`, `search.rs`, `TypedGraphState`, or any other component. Matches architecture's "single-file fix" description exactly.
- **Interface contracts**: All interfaces used as specified in ARCHITECTURE.md's Integration Surface table:
  - `TypedGraphState::rebuild(&store) -> Result<Self, StoreError>` called at `layer.rs:188`
  - `ServiceLayer::typed_graph_handle() -> TypedGraphStateHandle` called at `layer.rs:390`
  - `TypedGraphStateHandle` write-lock swap pattern at `layer.rs:391–393`
  - `EvalServiceLayer::typed_graph_handle() -> TypedGraphStateHandle` exposed at `layer.rs:452`
- **ADR compliance**:
  - ADR-001 (post-construction write vs. parameter): write-after-construction implemented as specified
  - ADR-002 (degraded mode vs. abort): degraded mode implemented, tested
  - ADR-003 (test live search): three-layer assertion implemented
  - ADR-004 (pub(crate) visibility): confirmed at `layer.rs:452`
  - ADR-005 (distribution_change=false): confirmed in TOML
- **Step 13b flow**: Exactly matches ARCHITECTURE.md component interaction diagram — `if let Some(state) = rebuilt_state` guard, `inner.typed_graph_handle()`, write-lock swap, immediate `drop(guard)`.
- **No scope additions**: No changes beyond what architecture specifies. `ScenarioResult`, `ProfileResult`, runner/report types all unchanged.

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence** from `crt-045-agent-4-tester-report.md`:
- `## Knowledge Stewardship` section present.
- `Queried:` entries present: `mcp__unimatrix__context_briefing` returning entries #2758, #4085, #3806. All three entries were applied in the test execution.
- `Stored:` entry: "nothing novel to store — no new fixture patterns or harness techniques discovered beyond what entries #4096 and #4100 already capture." Reason is specific and sufficient.

---

### Check 6: Integration Smoke Gate

**Status**: PASS

**Evidence**:
- `pytest -m smoke` ran 22 tests, all passed, run time 191.48s.
- No `@pytest.mark.xfail` markers exist in the integration test suite (confirmed via code search).
- No integration tests were deleted or commented out.
- RISK-COVERAGE-REPORT.md includes integration test counts: 22 smoke tests passed, 259 total in full suite.
- The report correctly explains no additional suites were required: crt-045 affects only the eval CLI path, which is not observable through MCP JSON-RPC.

---

## Rework Required

None.

---

## Deferred Items (Not Failures)

These are correctly deferred per the spawn prompt and the specification:

| Item | Reason | Gate Impact |
|------|--------|-------------|
| AC-02 (manual eval run with live snapshot) | Cannot automate without populated snapshot; live server not available in CI | Not a gate blocker |
| AC-04 (baseline regression, manual) | Same as AC-02; pre-existing regression tests serve as automated proxy | Not a gate blocker |
| R-07 (rebuild timeout guard) | Explicitly deferred in SPECIFICATION.md; sqlx implicit guard accepted | Not a gate blocker |
| R-09 (mrr_floor drift check) | Manual pre-merge verification per RISK-TEST-STRATEGY.md; delivery agent responsibility | Not a gate blocker |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` is not needed for the validator role — the validator reads source documents and artifacts directly. No lookup was needed before this analysis.
- Stored: nothing novel to store — this gate validation found no systemic failure patterns. All checks passed on first run. The feature-specific results live in this gate report.
