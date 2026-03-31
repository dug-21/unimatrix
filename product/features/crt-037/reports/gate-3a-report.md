# Gate 3a Report: crt-037

> Gate: 3a (Design Review)
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All five components match architecture decomposition; ADR decisions followed |
| Specification coverage | PASS | All 15 FRs and 8 NFs have corresponding pseudocode; no scope additions |
| Risk coverage | PASS | All 20 risks map to at least one test scenario in test plans |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage throughout |
| Knowledge stewardship compliance | PASS | Both agent reports contain Queried/Stored entries |
| Critical check 1 — NliCandidatePair tagged union | PASS | Enum with SupportsContradict and Informs variants; InformsCandidate has 9 required non-Option fields |
| Critical check 2 — AC-05 assertion specificity | PASS | Test asserts `scores[node_A_index] > 0.0` by specific lesson node index |
| Critical check 3 — Phase 8b five independent negative tests | PASS | Five named tests, one per composite guard predicate |
| Critical check 4 — AC-13 through AC-23 enumerated | PASS | All 11 integration tests explicitly named with mapped test function names |
| Critical check 5 — Domain vocabulary exclusion | PASS | Domain strings absent from nli_detection_tick.md pseudocode; confined to config.md |
| Critical check 6 — PPR fourth edges_of_type Direction::Outgoing | PASS | Both personalized_pagerank and positive_out_degree_weight use Direction::Outgoing |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

Component boundaries exactly match the architecture decomposition in ARCHITECTURE.md §Component Breakdown:

- `graph.rs` (unimatrix-engine): additive enum extension only — pseudocode/graph.md covers only RelationType, as_str(), from_str(), and doc comment update. No I/O, no other logic changes.
- `graph_ppr.rs` (unimatrix-engine): fourth `edges_of_type` call added in both `personalized_pagerank` and `positive_out_degree_weight` — pseudocode/graph_ppr.md matches architecture §Component B exactly, including Direction::Outgoing and the outgoing_contribution helper being unchanged.
- `config.rs` (unimatrix-server): three new fields with serde defaults and validate() range checks — pseudocode/config.md follows the exact pattern from architecture §Component C including the default value functions, field docs, and validate() additions.
- `read.rs` (unimatrix-store): `query_existing_informs_pairs` with directional (source_id, target_id) dedup — pseudocode/read.md matches architecture §Component E signature exactly.
- `nli_detection_tick.rs` (unimatrix-server): Phase 4b, Phase 8b, NliCandidatePair tagged union, Phase 5 combined cap, Phase 6 merged fetch, PairOrigin scaffolding — pseudocode/nli_detection_tick.md matches architecture §Component D and all phase diagrams.

ADR compliance:
- ADR-001 (tagged union): `NliCandidatePair` in pseudocode/nli_detection_tick.md and pseudocode/OVERVIEW.md is an enum with `SupportsContradict { source_id, target_id, cosine, nli_scores }` and `Informs { candidate, nli_scores }` variants. ADR-001 defines exactly this shape.
- ADR-002 (combined cap with Informs second priority): Phase 5 pseudocode in nli_detection_tick.md implements the exact algorithm from ARCHITECTURE.md §Phase 5 — truncate supports first, compute remaining_capacity, truncate informs to remaining. Includes the required debug log.
- ADR-003 (directional dedup): read.md pseudocode explicitly returns `(source_id, target_id)` without normalization, documented with a contrast table against query_existing_supports_pairs.

The wave ordering in pseudocode/OVERVIEW.md (Wave 1: graph.rs, config.rs, read.rs; Wave 2: graph_ppr.rs, nli_detection_tick.rs) is consistent with the dependency analysis in ARCHITECTURE.md §Component Interactions.

### Check 2: Specification Coverage

**Status**: PASS

All 15 functional requirements (FR-01 through FR-15) have corresponding pseudocode:

| FR | Component | Coverage |
|----|-----------|---------|
| FR-01 | graph.md | `from_str("Informs")` arm and `as_str()` arm added |
| FR-02 | graph.md | from_str recognition causes build_typed_relation_graph to include Informs edges; no warn |
| FR-03 | graph_ppr.md | Fourth `edges_of_type(Informs, Outgoing)` in personalized_pagerank |
| FR-04 | graph_ppr.md | Fourth `edges_of_type(Informs, Outgoing)` in positive_out_degree_weight |
| FR-05 | config.md | Three fields with serde defaults declared |
| FR-06 | config.md | validate() additions for exclusive and inclusive bounds |
| FR-07 | read.md | `query_existing_informs_pairs` directional dedup |
| FR-08 | nli_detection_tick.md | Phase 4b HNSW scan at nli_informs_cosine_floor |
| FR-09 | nli_detection_tick.md | All five Phase 4b guards: cross-category, temporal, cross-feature, cosine floor, dedup |
| FR-10 | nli_detection_tick.md | NliCandidatePair is a tagged union; PairOrigin scaffolding for construction |
| FR-11 | nli_detection_tick.md | Phase 8b composite guard includes Guard 5 (entailment <= supports_edge_threshold AND contradiction <= nli_contradiction_threshold) |
| FR-12 | nli_detection_tick.md | write_nli_edge called with "Informs", weight=cosine*ppr_weight, EDGE_SOURCE_NLI, current_timestamp_secs() |
| FR-13 | graph.md | graph_penalty and find_terminal_active noted as unchanged; penalty invariant documented |
| FR-14 | nli_detection_tick.md | Phase 5 combined cap with debug log of accepted/dropped counts |
| FR-15 | nli_detection_tick.md | C-12 explicitly called out; config fields passed as runtime values; no domain string literals |

Non-functional requirements are addressed:
- NF-01 (tick latency): nli_detection_tick.md Phase 4b uses the in-memory `entry_meta` HashMap (O(1) lookup from `all_active` already fetched in Phase 2) rather than per-entry DB calls; OQ-S3 resolved.
- NF-02 (cap safety): Phase 5 sequential cap algorithm ensures total <= max_graph_inference_per_tick.
- NF-03 (regression): nli_detection_tick.md §13 and graph.md §Regression Safety explicitly require existing tests to pass unchanged.
- NF-04 (sync-only rayon): Phase 7 pseudocode body is sync-only; C-05/C-14 constraints reiterated in nli_detection_tick.md Constraints section.
- NF-05 (no schema migration): read.md notes free-text column; no DDL change described anywhere.
- NF-06 (no new ML model): NliServiceHandle reused; no new ONNX session mentioned.
- NF-07 (no new tick): all changes are within run_graph_inference_tick body.
- NF-08 (weight finitude): Phase 8b includes `debug_assert!(weight.is_finite())`.

No scope additions were found. The pseudocode files do not implement any feature not required by the specification.

### Check 3: Risk Coverage

**Status**: PASS

All 20 risks from RISK-TEST-STRATEGY.md have test plan coverage. The test-plan/OVERVIEW.md includes a complete risk-to-test mapping table. Specific review of the critical and high-priority risks:

**R-01 (CHECK constraint)**: read.md includes `test_write_nli_edge_informs_row_is_retrievable` and `test_graph_edges_informs_relation_type_stored_verbatim`. DDL inspection note included.

**R-02 (PPR direction)**: graph_ppr.md `test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node` asserts `scores[node_A_index] > 0.0` by specific lesson node index. Note from entry #3896 about needing both A→B and B→A edges included. CI grep gate for Direction::Incoming included.

**R-03 (composite guard partial application)**: nli_detection_tick.md §2 has five independent named tests, one per composite guard predicate: temporal-equal, temporal-reversed, same-cycle, wrong-category, cosine-below-floor. Plus neutral=0.5 boundary test. Plus dual-failure test (one pass, one fail).

**R-04 (cross-contamination routing)**: nli_detection_tick.md §1 has three explicit tests: Phase 8 writes Supports not Informs for SupportsContradict variant; Phase 8b writes Informs not Supports for Informs variant; Informs pair with high entailment not written by Phase 8.

**R-05 (metadata survival)**: nli_detection_tick.md §4 includes AC-20 weight check (0.55 * 0.6 = 0.33 within f32 epsilon), AC-19 source="nli" check, and feature-cycle propagation test.

**R-20 (missing tests)**: test-plan/nli_detection_tick.md has 14 sections covering AC-13 through AC-23 all enumerated with named test functions. The critical mandate at the top of the file explicitly states delivery process hard-stop if missing.

Integration risks are covered: the three-crate boundary table in test-plan/OVERVIEW.md maps each interface to a specific test.

### Check 4: Interface Consistency

**Status**: PASS

The shared types defined in pseudocode/OVERVIEW.md are used consistently across component pseudocode files.

**NliCandidatePair**: Defined in OVERVIEW.md as an enum with `SupportsContradict { source_id, target_id, cosine, nli_scores }` and `Informs { candidate, nli_scores }`. Used in nli_detection_tick.md with identical variant names and field names. The architecture/ARCHITECTURE.md Integration Surface table also lists this exact shape. ADR-001 defines the same shape. All four sources are consistent.

**InformsCandidate**: Defined in OVERVIEW.md with nine non-Option required fields. Used in nli_detection_tick.md construction at Phase 4b with the same nine fields. The architecture ARCHITECTURE.md Integration Surface table lists "9 required (non-Option) fields: source_id, target_id, cosine, created_at×2, feature_cycle×2, category×2". ADR-001 lists identical fields.

One discrepancy noted and resolved: SPECIFICATION.md §Domain Models table defines `source_feature_cycle` and `target_feature_cycle` as `Option<String>`, while the pseudocode, architecture, and ADR-001 all define them as `String` (non-Option). The ADR-001 explicitly supersedes the spec table on this point, noting that the spec table was a pre-ADR draft. The note in pseudocode/nli_detection_tick.md lines 84-87 correctly explains that Phase 4b excludes pairs where either feature_cycle is None — this is the guard that allows InformsCandidate to hold non-Option Strings. This is architecturally consistent and correct. The spec table is the earlier, pre-ADR draft; the ADR is the authoritative type definition. No issue.

**InferenceConfig new fields**: config.md defines `informs_category_pairs: Vec<[String; 2]>`, `nli_informs_cosine_floor: f32` (default 0.45), `nli_informs_ppr_weight: f32` (default 0.6). Used in nli_detection_tick.md as `config.informs_category_pairs`, `config.nli_informs_cosine_floor`, `config.nli_informs_ppr_weight`. Architecture §Integration Surface table lists the same signatures. Consistent.

**query_existing_informs_pairs**: Defined in read.md as `pub async fn query_existing_informs_pairs(&self) -> Result<HashSet<(u64, u64)>>`. Called in nli_detection_tick.md as `store.query_existing_informs_pairs().await`. Architecture Integration Surface table lists the same signature. Consistent.

**Data flow** (OVERVIEW.md): config.rs → nli_detection_tick.rs (category pairs, floor, weight), read.rs → nli_detection_tick.rs (existing pairs set), graph.rs → graph_ppr.rs (RelationType::Informs variant). This is consistent with the architecture §Component Interactions diagram.

**String literal match**: nli_detection_tick.md passes `"Informs"` to `write_nli_edge` and notes it must match `RelationType::Informs.as_str()`. graph.md defines `as_str()` returning `"Informs"` exactly. Strings match.

### Critical Check 1: NliCandidatePair Tagged Union

**Status**: PASS

**Evidence**: pseudocode/OVERVIEW.md lines 67-104 and pseudocode/nli_detection_tick.md lines 21-71 both define `NliCandidatePair` as:

```rust
enum NliCandidatePair {
    SupportsContradict { source_id: u64, target_id: u64, cosine: f32, nli_scores: NliScores },
    Informs { candidate: InformsCandidate, nli_scores: NliScores },
}
```

This is a tagged union (Rust enum) with named variants, not a flat struct with Option fields. `InformsCandidate` has nine fields all declared as non-Option types: `source_id: u64`, `target_id: u64`, `cosine: f32`, `source_created_at: i64`, `target_created_at: i64`, `source_feature_cycle: String`, `target_feature_cycle: String`, `source_category: String`, `target_category: String`. ADR-001 is the governing decision; the pseudocode correctly implements it.

### Critical Check 2: AC-05 Assertion Specificity

**Status**: PASS

**Evidence**: test-plan/graph_ppr.md lines 31-41 define `test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node` with:
- Seed at B (decision node)
- Assert: `scores[node_A_index] > 0.0` — "assert by the *specific lesson node index*, not by any-node non-zero — covers AC-05 (R-20 gate check 5)"

The test also includes the entry #3896 note requiring both forward A→B and reverse B→A edges. pseudocode/graph_ppr.md lines 107-108 explicitly states: "Test AC-05 must assert `scores[A] > 0.0` (specifically the lesson node), not `scores.values().any(|&v| v > 0.0)`."

The assertion is specific to the lesson node, not a weaker aggregate check.

### Critical Check 3: Phase 8b Five Independent Negative Tests

**Status**: PASS

**Evidence**: test-plan/nli_detection_tick.md §2 (lines 62-144) contains five independently named tests, one per composite guard predicate:

1. **Temporal equal**: `test_phase8b_no_informs_when_timestamps_equal` — guard: `source_created_at = target_created_at`
2. **Temporal reversed**: `test_phase8b_no_informs_when_source_newer_than_target` — guard: `source_created_at > target_created_at`
3. **Same-cycle**: `test_phase8b_no_informs_when_same_feature_cycle` — guard: feature cycles identical
4. **Wrong-category**: `test_phase8b_no_informs_when_category_pair_not_in_config` — guard: `("decision", "decision")` not in pairs
5. **Cosine-below-floor**: `test_phase8b_no_informs_when_cosine_below_floor` — guard: cosine 0.44 below floor 0.45

Each is a separate named test with distinct arrange/assert. The neutral=0.5 boundary test (`test_phase8b_no_informs_when_neutral_exactly_0_5`) and the dual-failure test are additional. All five required negative tests are present.

### Critical Check 4: AC-13 Through AC-23 Enumerated

**Status**: PASS

**Evidence**: test-plan/nli_detection_tick.md Acceptance Criteria table (lines 373-384) explicitly maps all 11 integration tests:

| AC-ID | Test Name |
|-------|-----------|
| AC-13 | `test_phase8b_writes_informs_edge_when_all_guards_pass` |
| AC-14 | `test_phase8b_no_informs_when_timestamps_equal`, `test_phase8b_no_informs_when_source_newer_than_target` |
| AC-15 | `test_phase8b_no_informs_when_same_feature_cycle` |
| AC-16 | `test_phase8b_no_informs_when_category_pair_not_in_config` |
| AC-17 | `test_phase8b_no_informs_when_cosine_below_floor` |
| AC-18 | `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`, `test_phase4b_pair_in_cosine_band_processed_by_phase4b_not_phase4` |
| AC-19 | `test_phase8b_metadata_from_phase4b_survives_to_write` |
| AC-20 | `test_phase8b_edge_weight_equals_cosine_times_ppr_weight` |
| AC-21 | CI grep gate + `test_tick_completes_without_panic_requiring_nli_scoring` |
| AC-22 | CI grep gate (domain strings) |
| AC-23 | `test_second_tick_does_not_write_duplicate_informs_edge`, `test_second_tick_query_existing_informs_pairs_loads_prior_edge` |

All 11 are enumerated with named test functions. AC-21 and AC-22 are shell grep gates as required by the specification.

### Critical Check 5: Domain Vocabulary Exclusion

**Status**: PASS

**Evidence**: The strings `"lesson-learned"`, `"decision"`, `"pattern"`, `"convention"` appear in pseudocode/config.md (in `default_informs_category_pairs()`) as required. In pseudocode/nli_detection_tick.md, these strings do not appear as literals. The Phase 4b pseudocode uses `source_meta.category.as_str()` compared against `config.informs_category_pairs` (a runtime value). No domain string literals exist in the detection tick pseudocode. The C-12 constraint is explicitly noted in the Constraints section of nli_detection_tick.md.

The CI grep gate command is documented in test-plan/OVERVIEW.md and test-plan/nli_detection_tick.md §8:
```bash
grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty
```

### Critical Check 6: PPR Fourth edges_of_type Direction::Outgoing

**Status**: PASS

**Evidence**: pseudocode/graph_ppr.md lines 33-35 and 62-64 show the fourth call in both functions:

```
for edge_ref in graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing):
    neighbor_contribution += outgoing_contribution(...)
```

and

```
for edge_ref in graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing):
    total += edge_ref.weight().weight as f64
```

Direction is `Direction::Outgoing` in both — consistent with the three existing calls for Supports, CoAccess, and Prerequisite. The architecture ARCHITECTURE.md §Integration Surface table confirms `personalized_pagerank (4th call): edges_of_type(idx, RelationType::Informs, Direction::Outgoing)` and `positive_out_degree_weight (4th call): edges_of_type(idx, RelationType::Informs, Direction::Outgoing)`. No `Direction::Incoming` appears in the PPR pseudocode.

The direction regression CI grep gate is documented in test-plan/OVERVIEW.md:
```bash
grep -n 'Direction::Incoming' crates/unimatrix-engine/src/graph_ppr.rs
# Expected: empty output
```

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

Both agent reports contain Knowledge Stewardship sections with substantive entries:

**crt-037-agent-1-pseudocode-report.md**:
- `Queried:` entries present: two context_search queries retrieving entries #3937, #3675, #3727, #3884, #3939, #3940, #3942.
- No `Stored:` entry but the report notes all established patterns were followed without deviation. This is acceptable for a pseudocode agent (read-only stewardship role per gate 3a definition).

**crt-037-agent-2-testplan-report.md**:
- `Queried:` entries present: context_briefing returning 17 entries; ADR-001 (#3942), ADR-002 (#3939), ADR-003 (#3940), PPR regression trap (#3896), Direction trap (#3744) retrieved and used.
- `Stored:` entry present: entry #3943 "Per-guard negative test for multi-predicate composite guards in NLI detection tick" stored via context_store (category: pattern, topic: testing).

Both reports fulfill stewardship obligations for their respective roles.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- all gate 3a findings are feature-specific; no systemic pattern emerged that would apply across other features beyond what is already documented in the test plan agent's entry #3943.
