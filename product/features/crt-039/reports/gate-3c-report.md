# Gate 3c Report: crt-039

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-02
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | 11/12 risks fully covered; R-05 partially covered (pre-existing eval gap) |
| Test coverage completeness | PASS | All risk-to-scenario mappings exercised; integration smoke gate passes |
| Specification compliance | PASS | All 18 ACs verified; AC-11 partial (pre-existing gap, accepted per spawn prompt) |
| Architecture compliance | PASS | Control flow, ordering invariant, and component boundaries match ARCHITECTURE.md |
| Knowledge stewardship | PASS | Queried and Stored entries present in tester agent report |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof

**Status**: PASS (with documented partial on R-05)

**Evidence**:

| Risk ID | Coverage | Evidence |
|---------|----------|----------|
| R-01 (Supports written without NLI) | Full | TC-02 (`test_phase8_no_supports_when_nli_not_ready`) PASS; `get_provider()` Err returns before Phase 8 entry at line 522; structurally no path from Err to `write_nli_edge("Supports",...)` |
| R-02 (test coverage gap at Path A/B boundary) | Full | TC-01 + TC-02 separate tests present (lines 1278, 1383); TR-01 grep confirms removal of `test_run_graph_inference_tick_nli_not_ready_no_op` |
| R-03 (mutual-exclusion gap at cosine 0.50 boundary) | Full | Explicit Supports-set subtraction at lines 402–410; TC-07 (`test_phase4b_explicit_supports_set_subtraction` line 2157) PASS |
| R-04 (dead-code enum variants) | Full | `NliCandidatePair` has only `SupportsContradict` variant (line 72); `PairOrigin` has only `SupportsContradict` variant (line 113); grep for `NliCandidatePair::Informs\|PairOrigin::Informs` returns comment lines only |
| R-05 (cosine floor raise eliminates candidate pool) | Partial | `run_eval.py` absent (pre-existing gap — spawn prompt notes this as accepted); scenarios.jsonl present; AC-17 observability log (`informs_candidates_found`) operational from tick 1; ADR-003 implementor corpus scan requirement documented |
| R-06 (stale nli_scores at call sites) | Full | `apply_informs_composite_guard` signature is single-arg `(candidate: &InformsCandidate)` at line 806; `cargo test --workspace` passes with 0 failures |
| R-07 (Phase 8b skipped when candidate_pairs empty) | Full | Path A write loop (lines 463–497) executes before Path B entry gate (lines 509–517); TC-01 uses zero-Supports-candidates corpus and confirms Informs edges written |
| R-08 (format_nli_metadata_informs dead code) | Full | `format_informs_metadata` at line 818 replaces it; grep returns doc-comment reference only; `cargo clippy -p unimatrix-server` passes |
| R-09 (contradiction scan behavioral change) | Full | `background.rs` diff shows comment additions only; contradiction scan condition unchanged at lines 675–732; existing tests pass |
| R-10 (log emitted at wrong pipeline point) | Full | `informs_candidates_found` incremented at line 367 (before dedup at line 370); log emitted at lines 501–507 (after Phase A writes); four log fields ordered: found → after_dedup → after_cap → written |
| R-11 (tick ordering disturbed) | Full | Ordering invariant comment at lines 661–664 matches FR-11 spec exactly; `run_graph_inference_tick` called at lines 773–780, after contradiction scan (lines 675–732) and extraction_tick (lines 734–767) |
| R-12 (cosine floor boundary semantics inverted) | Full | `phase4b_candidate_passes_guards` uses `similarity < config.nli_informs_cosine_floor` (strict less, meaning `>=` is the pass condition) at line 769; `test_phase4b_cosine_floor_boundary` (line 2128) covers 0.500 included and 0.499 excluded |

**R-05 Partial — Accepted**: The spawn prompt explicitly acknowledges `run_eval.py` was never implemented (pre-existing gap, not caused by crt-039). Mitigation via: (1) FR-14 observability log operational from tick 1, (2) ADR-003 pre-condition corpus scan requirement, (3) conservative floor direction (higher floor = more selective, not permissive). Recommendation: file a follow-up GH Issue to implement `run_eval.py` before Group 3 graph enrichment ships.

---

### Check 2: Test Coverage Completeness

**Status**: PASS

All risk-to-scenario mappings from the Risk-Based Test Strategy are exercised.

**Unit tests** (confirmed from `cargo test --workspace` output):
- `unimatrix-server` (lib): 2572 passed, 0 failed
- `unimatrix-core`: 423 passed, 0 failed
- `unimatrix-store`: 346 passed, 0 failed, 1 ignored (pre-existing)
- All other crates: clean

**Required new tests confirmed present and passing**:
- TC-01: `test_phase4b_writes_informs_when_nli_not_ready` — line 1278 (PASS, positive Informs assertion)
- TC-02: `test_phase8_no_supports_when_nli_not_ready` — line 1383 (PASS, zero Supports assertion)
- TC-05/TC-06: `test_phase4b_cosine_floor_boundary` — line 2128 (PASS, 0.500 included, 0.499 excluded)
- TC-07: `test_phase4b_explicit_supports_set_subtraction` — line 2157 (PASS, explicit set subtraction)

**Removed tests confirmed absent** (TR-01/TR-02/TR-03):
- `fn test_run_graph_inference_tick_nli_not_ready_no_op` — not present (comment at line 1269 records removal)
- `fn test_phase8b_no_informs_when_neutral_exactly_0_5` — not present (comment at line 2048)
- `fn test_phase8b_writes_informs_when_neutral_just_above_0_5` — not present (comment at line 2049)

**Integration smoke tests** (`pytest -m smoke`): 22/22 PASS.

**Lifecycle suite**: 41/41 non-xfail PASS. 2 xfailed (pre-existing, GH#291). 1 xpassed (`test_search_multihop_injects_terminal_active`, GH#406): tests multi-hop supersession traversal in `search.rs` — entirely unrelated to crt-039's tick decomposition changes. xpassed because another unrelated change fixed the traversal; xfail marker should be removed in a follow-up PR.

**No integration tests deleted or commented out** by crt-039.

**Integration test counts in RISK-COVERAGE-REPORT**: 63 integration tests run (22 smoke + 41 lifecycle), 63 passed — documented in the report.

---

### Check 3: Specification Compliance

**Status**: PASS (AC-11 partial, accepted)

Verification of all 18 acceptance criteria:

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `run_graph_inference_tick` called unconditionally at background.rs:773; comment at line 772 explains the removed guard |
| AC-02 | PASS | TC-01 and TC-02 both integration tests with real Store, both passing |
| AC-03 | PASS | `apply_informs_composite_guard(candidate: &InformsCandidate)` at line 806 — single argument, no `nli_scores` |
| AC-04 | PASS | `default_nli_informs_cosine_floor()` returns `0.5` at config.rs:784 |
| AC-05 | PASS | `test_phase4b_cosine_floor_boundary` at line 2128 |
| AC-06 | PASS | Contradiction scan condition unchanged; all existing contradiction scan tests pass |
| AC-07 | PASS | Ordering invariant comment at background.rs:661–664 matches FR-11 exactly |
| AC-08 | PASS | config.rs test section uses `0.5`; no `0.45` floor assertion |
| AC-09 | PASS | Both neutral guard tests absent; grep returns empty for function definitions |
| AC-10 | PASS | `cargo test --workspace` exits 0, 0 failures across all crates |
| AC-11 | PARTIAL | `run_eval.py` absent (pre-existing). scenarios.jsonl present. MRR quantitative gate cannot execute. Accepted per spawn prompt. |
| AC-12 | PASS | `query_existing_informs_pairs` at line 174 (Phase 2); `truncate(MAX_INFORMS_PER_TICK)` at line 448 (Phase 5) before write loop at line 463 |
| AC-13 | PASS | Explicit `supports_candidate_set` subtraction at lines 402–410; TC-07 validates |
| AC-14 | PASS | TC-02 confirms zero Supports edges when NLI Loading state |
| AC-15 | PASS | TC-01 confirms at least one Informs edge written when NLI Loading state |
| AC-16 | PASS | TC-01 (line 1278) and TC-02 (line 1383) are separate functions; old test name absent |
| AC-17 | PASS | Four log fields: `informs_candidates_found` (line 290 declare, 367 increment), `informs_candidates_after_dedup` (line 442), `informs_candidates_after_cap` (line 449), `informs_edges_written` (line 494); log at line 501 |
| AC-18 | PASS | `format_informs_metadata` at line 818; `format_nli_metadata_informs` absent from production code; clippy clean for `unimatrix-server` |

**NFR compliance**:
- NFR-01 (no rayon pool in Phase 4b): Phase A write loop (lines 463–497) uses only `write_nli_edge` (async, no rayon). PASS.
- NFR-02 (`MAX_INFORMS_PER_TICK = 25`): constant at line 51; `truncate` at line 448. PASS.
- NFR-03 (no `score_batch` when `nli_enabled=false`): `score_batch` only reached via Path B after `get_provider()` Ok. PASS.
- NFR-04 (config compatibility): only `nli_informs_cosine_floor` default changed. PASS.
- NFR-05 (eval gate MRR >= 0.2913): partial, accepted per spawn prompt.
- NFR-06 (file size): production code section is ~897 lines (pre-existing). crt-039 is net-negative per OVERVIEW.md assessment. WARN (pre-existing, not introduced by crt-039; documented in gate-3b).
- NFR-07 (zero behavioral change to contradiction scan): background.rs diff shows comment additions only. PASS.

---

### Check 4: Architecture Compliance

**Status**: PASS

**Control flow split (ADR-001)**: The restructured `run_graph_inference_tick` implements the exact sequence from ARCHITECTURE.md:
- Phase 4b HNSW expansion for Informs candidates (lines 273–395)
- Explicit Supports-set subtraction (lines 402–410)
- Phase 5 independent caps (lines 412–455)
- Path A write loop — unconditional (lines 457–507)
- Path B entry gate — `candidate_pairs.is_empty()` early return (lines 509–517)
- `get_provider()` error gate for Supports (lines 519–530)
- Phase 6/7/8 NLI Supports path (lines 532+)

**Ordering invariant (background.rs)**: Confirmed sequence:
1. compaction, promotion, graph-rebuild (before line 661)
2. contradiction_scan (lines 669–732, gated on `nli_enabled` interval)
3. extraction_tick (lines 734–767)
4. `run_graph_inference_tick` unconditional (lines 769–780)

This matches ARCHITECTURE.md Component 1 specification exactly.

**`apply_informs_composite_guard` (ADR-002)**: Two guards only — temporal (line 807) and cross-feature (lines 808–810). Guards 1, 4, 5 removed. No `nli_scores` parameter. Matches ADR-002.

**`nli_informs_cosine_floor` default (ADR-003)**: `default_nli_informs_cosine_floor()` returns `0.5` at config.rs:784. Matches ADR-003.

**`NliCandidatePair` and `PairOrigin` enum cleanup (ADR-001)**: `Informs` variant removed from both enums. Compiler-enforced exhaustive match coverage. No dead variants retained with `#[allow(dead_code)]`.

**Dedup pre-filter placement (SR-01)**: `query_existing_informs_pairs` loaded in Phase 2 before Phase 4b loop; applied at line 370 inside Phase 4b before candidate is pushed to `informs_metadata`. Matches ARCHITECTURE.md dedup pre-filter specification.

**`format_informs_metadata` replacement (ARCHITECTURE.md)**: `format_nli_metadata_informs` fully replaced by `format_informs_metadata(cosine: f32, source_category: &str, target_category: &str)` at line 818. No NLI score fields in metadata JSON.

**Contradiction scan block labeling (ARCHITECTURE.md)**: Named comment block at lines 669–673 makes independence explicit. Condition unchanged. Zero-diff behavioral constraint (SR-07/NFR-07) satisfied.

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

The tester agent report (`product/features/crt-039/agents/crt-039-agent-3-nli-tick-report.md`) does not appear to contain an explicit Knowledge Stewardship section, but the RISK-COVERAGE-REPORT itself (the tester deliverable) includes a `## Knowledge Stewardship` section with:
- Queried: `mcp__unimatrix__context_briefing` returned entries #3806, #3656, #3949, #2970, #3946, #4019, #4018; entry #2758 explicitly applied.
- Stored: "nothing novel to store — the test execution followed established patterns..." with specific reason.

This satisfies the stewardship block requirement: Queried entries documented, Stored entry has a substantive reason rather than a bare "nothing novel."

---

## xfail / xpassed Audit

| Test | Marker | GH Issue | Genuine Pre-existing? |
|------|--------|----------|----------------------|
| `test_auto_quarantine_after_consecutive_bad_ticks` | xfail | GH#291 | Yes — requires drivable tick interval |
| `test_dead_knowledge_entries_deprecated_by_tick` | xfail | GH#291 | Yes — same root cause |
| `test_search_multihop_injects_terminal_active` | xpassed | GH#406 | Yes — tests multi-hop supersession in search.rs, not crt-039 tick decomposition |

The xpassed test (`test_search_multihop_injects_terminal_active`) tests `search.rs` multi-hop traversal behavior, entirely outside crt-039's scope (which modifies `background.rs`, `nli_detection_tick.rs`, and `infra/config.rs`). The unexpected pass is not masking any crt-039 feature bug. The xfail marker should be removed in a follow-up PR per RISK-COVERAGE-REPORT recommendation.

---

## Follow-up Recommendations

1. **File a GH Issue for `run_eval.py`** (R-05, AC-11, NFR-05): The eval harness runner must be implemented before Group 3 graph enrichment ships. Group 3 depends on a live Informs graph and requires quantitative regression coverage.

2. **Remove xfail marker from `test_search_multihop_injects_terminal_active`**: The test now passes. The marker (GH#406) may be resolved or the fix landed incidentally.

3. **NFR-06 technical debt**: Production code in `nli_detection_tick.rs` is ~897 lines (pre-existing, above 500-line guideline). Not introduced by crt-039. A dedicated refactor ticket should track the submodule extraction.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the gate-3c validation followed established patterns (xfail GH-issue verification, eval harness gap acceptance with documented mitigation, cargo test output verification). A pre-existing pattern: partial eval coverage from absent runner scripts is a recurring risk for features that depend on research harnesses — but this is the first observation; a second occurrence would warrant storing as a lesson-learned.
