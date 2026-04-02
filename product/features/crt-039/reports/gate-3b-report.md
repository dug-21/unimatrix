# Gate 3b Report: crt-039

> Gate: 3b (Code Review)
> Date: 2026-04-02
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All pseudocode constructs implemented exactly |
| Architecture compliance | PASS | ADR-001/002/003 all satisfied |
| Interface implementation | PASS | Signatures match; enum variants removed correctly |
| Test case alignment | PASS | TC-01 through TC-07 present; TR-01/02/03 absent |
| Code quality | WARN | Modified production files exceed 500 lines — pre-existing, documented in OVERVIEW.md NFR-06 |
| Security | PASS | No secrets, no path traversal, no unwrap() in non-test code |
| Knowledge stewardship | PASS | All agent reports have Queried + Stored entries |

---

## Detailed Findings

### 1. Pseudocode Fidelity

**Status**: PASS

**Evidence**:

- Phase 1 removal (get_provider() moved to Path B entry): confirmed at `nli_detection_tick.rs:137` — `// Phase 1 removed (crt-039 ADR-001)`.
- Path A unconditional write loop (lines 457–507): iterates `informs_metadata`, calls `apply_informs_composite_guard(candidate)`, computes weight, calls `format_informs_metadata`, writes via `write_nli_edge`. Matches pseudocode skeleton exactly.
- Path B entry gate (lines 509–530): `if candidate_pairs.is_empty() { return; }` followed by `get_provider()` match with Err-return. Exactly as specified in pseudocode.
- Phase 4b Supports-set subtraction (lines 402–410): `supports_candidate_set` built from `candidate_pairs` (both directions via `flat_map`), then `informs_metadata.retain(...)`. Matches pseudocode FR-06/AC-13 requirement.
- Observability log (lines 501–507): all four fields (`informs_candidates_found`, `informs_candidates_after_dedup`, `informs_candidates_after_cap`, `informs_edges_written`) in a single `tracing::debug!` call after the Path A write loop. Correct placement.
- `format_informs_metadata` (lines 818–825): emits `{"cosine", "source_category", "target_category"}`. No NLI score fields. Matches pseudocode.
- `apply_informs_composite_guard` (lines 806–811): exactly 1 parameter (`candidate: &InformsCandidate`), exactly 2 guards (temporal + cross-feature). Matches ADR-002.

**R-01 / ADR-001 critical invariant**: `get_provider()` is called at line 522, which is AFTER the Phase A write loop completes (lines 463–497) and after the observability log (lines 501–507). No code path from `get_provider()` Err to a `write_nli_edge("Supports")` call exists — the Err arm at line 524 is `return`. PASS.

**AC-13 / FR-06 explicit subtraction**: Present as `informs_metadata.retain(...)` (line 407–410) after the `supports_candidate_set` construction (lines 402–405). This is NOT reliance on threshold arithmetic alone. PASS.

**AC-17 observability log placement**: Log emitted at lines 501–507, AFTER the Phase 8b write loop (which ends at line 497). All four values known. Emitted even when `informs_edges_written = 0` (no guard). PASS.

**ADR-002 guard count**: `apply_informs_composite_guard` at line 806: signature `(candidate: &InformsCandidate) -> bool`, body checks `candidate.source_created_at < candidate.target_created_at` AND the feature-cycle cross-feature check. Exactly 2 guards, no `nli_scores`, no `config`. PASS.

### 2. Architecture Compliance

**Status**: PASS

**Evidence**:

- **ADR-001 (Control flow split)**: `background.rs` calls `run_graph_inference_tick` unconditionally (lines 773–780). No `if inference_config.nli_enabled` wrapper present. Grep confirms: only occurrence of `nli_enabled` near the call is a comment at line 772 saying the gate was removed.
- **ADR-002 (Guard simplification)**: `apply_informs_composite_guard` has 1 parameter and 2 guards. All call sites (lines 467, 1596, 1668, 1685, 1706, 1782, 2067, 2082, 2095, 2103, 2111, 2120) pass a single `&candidate` argument. No stale 3-argument calls.
- **ADR-003 (Cosine floor)**: `default_nli_informs_cosine_floor()` returns `0.5` (config.rs:784). `InferenceConfig::default()` sets `nli_informs_cosine_floor: default_nli_informs_cosine_floor()` (config.rs:628). Both locations confirmed.
- **Tick ordering invariant (FR-11)**: Comment block at `background.rs:661–667` shows `contradiction_scan BEFORE structural_graph_tick` — the invariant comment lists contradiction_scan before extraction_tick before structural_graph_tick. Code order matches comment (contradiction scan at line 675, extraction tick at line 735, graph tick at line 773). PASS.
- **R-04 (enum variants removed)**: `NliCandidatePair::Informs` and `PairOrigin::Informs` variants are absent. Grep of production code returns only comment references ("Informs variant removed") — no match arm, no variant construction, no wildcard. Phase 8 match at lines 636–646 uses only `PairOrigin::SupportsContradict`. PASS.

### 3. Interface Implementation

**Status**: PASS

**Evidence**:

- `run_graph_inference_tick` public signature unchanged: `(store: &Store, nli_handle: &NliServiceHandle, vector_index: &VectorIndex, rayon_pool: &RayonPool, config: &InferenceConfig)`. Returns `()`. PASS.
- `apply_informs_composite_guard` private signature correctly simplified to `(candidate: &InformsCandidate) -> bool`. PASS.
- `phase4b_candidate_passes_guards` signature unchanged: 8 parameters, returns `bool`. PASS.
- `format_informs_metadata` new function: `(cosine: f32, source_category: &str, target_category: &str) -> String`. `format_nli_metadata_informs` absent from production code (only in a comment on line 817 as a replacement note). PASS.
- `InformsCandidate` struct unchanged: 9 fields, all non-Option. PASS.
- `NliCandidatePair` enum: single `SupportsContradict` variant. `PairOrigin` enum: single `SupportsContradict` variant. Both consistent. PASS.
- `nli_informs_cosine_floor` default 0.5 in both locations (ADR-003, pattern #4011): `default_nli_informs_cosine_floor()` returns `0.5` (line 784) AND `InferenceConfig::default()` sets field via this function (line 628). PASS.

### 4. Test Case Alignment

**Status**: PASS

**Evidence**:

**TR-01/TR-02/TR-03 removal**: Grep for deleted test names returns only comment markers (lines 1269, 2048, 2049) — not function bodies. The function bodies are gone. PASS.

**TC-01** (`test_phase4b_writes_informs_when_nli_not_ready`, line 1278): Real Store, two entries with embeddings, `NliServiceHandle::new()` (Loading state). Asserts `informs_count >= 1` AND `supports_count == 0`. Separate test from TC-02. PASS.

**TC-02** (`test_phase8_no_supports_when_nli_not_ready`, line 1383): Separate test. Real Store, entries with cosine > supports_candidate_threshold (identical embeddings, cosine ~1.0). Asserts `supports.len() == 0`. PASS.

**TC-07** (`test_phase4b_explicit_supports_set_subtraction`, line 2157): Validates the explicit subtraction. Builds `supports_candidate_set` from `candidate_pairs`, populates `informs_metadata` with pair (1,2) at cosine 0.68 and pair (3,4) at cosine 0.55, applies `retain(...)`, asserts pair (1,2) absent and pair (3,4) present. Also tests boundary variant at cosine 0.50. PASS.

**TC-03** (`test_apply_informs_composite_guard_temporal_guard`, line 2055): Two assertions — source newer fails, source older passes. Single `&candidate` argument. PASS.

**TC-04** (`test_apply_informs_composite_guard_cross_feature_guard`, line 2089): Four assertions covering same-cycle (fail), source-empty (pass), target-empty (pass), different-cycles (pass). PASS.

**TC-05/TC-06** (`test_phase4b_cosine_floor_boundary`, line 2128): Combined test. 0.500 passes (inclusive >=), 0.499 excluded. PASS.

**Config tests updated**: `test_inference_config_default_nli_informs_cosine_floor` (config.rs:6853) asserts `default_nli_informs_cosine_floor() == 0.5_f32` (TC-06a) and `config.nli_informs_cosine_floor == 0.5_f32` (TC-06b). `test_validate_nli_informs_cosine_floor_valid_value_is_ok` uses `0.5` as nominal value. No stale 0.45 assertions in test section. PASS.

**`test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`** (line 1837): Updated to use `cosine_at_floor = 0.50_f32`, verifies `phase4b_candidate_passes_guards` returns true at exactly 0.50. Includes sanity assert that floor is 0.5 after crt-039. PASS.

**All `apply_informs_composite_guard` call sites in tests**: Every occurrence passes exactly one argument (`&candidate`). No stale 3-argument calls found. PASS.

**`informs_passing_scores()` helper**: Removed (no remaining callers after NliScores parameter removed). PASS.

### 5. Code Quality

**Status**: WARN (pre-existing, documented)

**Evidence**:

- **Build**: `cargo build --workspace` — zero errors. 17 pre-existing warnings in unimatrix-server (not in modified files). PASS.
- **Tests**: `cargo test -p unimatrix-server` — 2572 passed, 0 failed. PASS.
- **No stubs**: Grep for `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in modified files (`nli_detection_tick.rs`, `background.rs`, `infra/config.rs`) — the one TODO at `background.rs:970` is in the pre-existing lifecycle guard stub (crt-031, GH#409), NOT introduced by crt-039. crt-039 touched lines ~660–780 of background.rs only. WARN (pre-existing, not introduced by this feature).
- **No `.unwrap()` in non-test code**: All `.unwrap()` occurrences in `nli_detection_tick.rs` are after line 898 (the `#[cfg(test)]` boundary). PASS.
- **File size**: `nli_detection_tick.rs` production code ends at line 897. `background.rs` is 3898 lines total. `config.rs` is 7052 lines total. All three exceed the 500-line workspace guideline. However, the OVERVIEW.md explicitly documents this as a pre-existing condition for `nli_detection_tick.rs` (NFR-06/OQ-04: "net-removal, no extraction needed"). `background.rs` and `config.rs` are large multi-concern files that predate this feature. crt-039 is net-negative in all three files. WARN (pre-existing, no new boundary crossed, documented).

**Clippy**: Clippy errors in workspace are entirely in pre-existing crates (`unimatrix-engine`, `unimatrix-observe`, `anndists`). No clippy errors in `unimatrix-server`. PASS.

**`cargo audit`**: Tool not installed in this environment (`cargo audit` not available). Cannot verify. WARN (tooling gap — not attributable to crt-039).

### 6. Security

**Status**: PASS

**Evidence**:

- No hardcoded secrets, API keys, or credentials in modified files. PASS.
- Input validation at boundaries: `weight.is_finite()` guard at line 472 prevents NaN/Inf from being written as edge weight. PASS.
- No path traversal or shell invocation in modified code. PASS.
- Serialization (`format_informs_metadata`) uses `serde_json::json!` macro — cannot panic on well-typed inputs. PASS.
- No new unsafe code introduced. PASS.

### 7. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

- `crt-039-agent-3-nli-tick-report.md`: Has `## Knowledge Stewardship` section. Has `Queried:` entry (context_briefing, found ADRs #4017/4018/4019 and patterns). Has `Stored:` entry — entry #4020 "Observability counters in tick pipeline loops must be incremented before dedup-check continue statements". PASS.
- `crt-039-agent-3-background-report.md`: Has `## Knowledge Stewardship` section. Has `Queried:` entry (context_search, ADRs confirmed). Has "nothing novel to store" with reason (pattern of unconditional tick calls with internal gating already captured). PASS.
- `crt-039-agent-3-config-report.md`: Has `## Knowledge Stewardship` section. Has `Queried:` entry (context_search, found #4013 directly relevant). Has `Stored:` note — attempted `context_correct` on #4013 but agent lacked Write capability; proposed content documented in report for admin follow-up. WARN (good faith attempt documented; tooling limitation, not agent failure).

---

## Rework Required

None.

---

## Verification of Spawn Prompt Key Items

| Item | Result | Evidence |
|------|--------|---------|
| R-01/ADR-001: get_provider() called ONLY after Path A completes | PASS | get_provider() at line 522; Path A loop ends at line 497; no write_nli_edge("Supports") reachable from Err path |
| AC-13/FR-06: explicit Supports-set subtraction present | PASS | Lines 402–410: supports_candidate_set built, informs_metadata.retain(...) called |
| AC-17: tracing::debug! with all 4 fields after Phase 8b | PASS | Lines 501–507: all four fields in single debug! call after loop |
| ADR-002: apply_informs_composite_guard has exactly 1 param, 2 guards | PASS | Line 806: single param; body has temporal && cross-feature only |
| R-04: NliCandidatePair::Informs and PairOrigin::Informs removed | PASS | Grep returns only comment references; no active variant or match arm |
| TR-01/TR-02/TR-03: three tests absent | PASS | Function bodies absent; only comment markers (lines 1269, 2048, 2049) |
| TC-01 and TC-02: two separate tests | PASS | Lines 1278 and 1383; separate #[tokio::test] functions |
| TC-07: test_phase4b_explicit_supports_set_subtraction exists | PASS | Line 2157; validates explicit subtraction, not threshold arithmetic |
| Ordering invariant comment shows contradiction_scan BEFORE structural_graph_tick | PASS | Lines 661–664: comment shows `contradiction_scan -> extraction_tick -> structural_graph_tick` |
| nli_informs_cosine_floor default 0.5 in BOTH locations | PASS | config.rs:784 (backing fn) and config.rs:628 (InferenceConfig::default) |
| cargo audit | WARN | Tool not installed; cannot run |
| No stubs in modified files | PASS | No todo!(), unimplemented!(), FIXME in crt-039 code; one pre-existing TODO(#409) in background.rs outside crt-039 scope |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — not invoked (direct file review sufficient for validation gate). No novel pattern found requiring storage beyond what implementors already stored.
- Stored: nothing novel to store — gate-3b validation found all checks passing with no systemic failure patterns. The pre-existing file size concern is documented in crt-039 OVERVIEW.md already. Entry #4020 was stored by the implementor covering the counter-placement pattern.
