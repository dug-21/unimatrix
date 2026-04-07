# Gate 3c Report: crt-049

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-07
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 13 risks mapped to passing tests in RISK-COVERAGE-REPORT.md |
| Test coverage completeness | PASS | All risk-to-scenario mappings exercised; R-04 boundary via code review (documented acceptable) |
| Specification compliance | PASS | All 17 ACs verified; all 7 GATE ACs pass |
| Architecture compliance | PASS | Components match architecture; all ADR decisions implemented |
| Knowledge stewardship | PASS | Tester agent report has Queried + Stored entries |

## Detailed Findings

### Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 13 risks from RISK-TEST-STRATEGY.md to specific named passing tests:

- R-01 (triple-alias serde chain): 5 dedicated round-trip tests — canonical key, delivery_count alias, tier1_reuse_count alias, canonical serialization output, full round-trip. All pass.
- R-02 (normalize_tool_name omission): `test_extract_explicit_read_ids_prefixed_context_get_matched`, `_prefixed_context_lookup_matched`, `_prefixed_tool_name_normalized`. All pass. Verified in code: `knowledge_reuse.rs` line 93 calls `normalize_tool_name(raw_tool)` before comparison.
- R-03 (total_served semantics change): `test_compute_knowledge_reuse_total_served_excludes_search_exposures`, `_union_of_reads_and_injections`. Advisory message at tools.rs line 2724–2726 contains: "schema_version 2 predates the explicit read signal and total_served redefinition (search exposures no longer contribute to total_served)" — specific semantic language confirmed.
- R-04 (cap at 500 silently partial): `test_explicit_read_meta_cap_constant_exists` asserts `EXPLICIT_READ_META_CAP == 500`; cap logic verified at tools.rs lines 3281–3291 with `tracing::warn!` on exceedance. Full runtime 501-ID test documented as impractical without mock store — accepted per test plan.
- R-05 (early-return guard): `test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle`, `test_render_knowledge_reuse_injection_only_cycle_not_suppressed`. Guard at retrospective.rs:998 confirmed: `total_served == 0 && search_exposure_count == 0`.
- R-06 (section-order regression): `test_render_knowledge_reuse_golden_output_all_sections`, `_no_legacy_distinct_entries_served_label`, `_explicit_read_categories_section`. All pass.
- R-07 (attributed slice not threaded): `test_compute_knowledge_reuse_for_sessions_explicit_read_count_from_attributed`. tools.rs call site at line 1949–1953 confirmed: passes `&attributed`.
- R-08 (SUMMARY_SCHEMA_VERSION): `test_summary_schema_version_is_three` (CRS-V24-U-01); cycle_review_index.rs line 35: `pub const SUMMARY_SCHEMA_VERSION: u32 = 3`.
- R-09 (explicit_read_by_category contract): Four tests covering populated map, empty map, serde round-trip, absent field defaults to empty map. All pass.
- R-10 (filter-based lookup included): Three tests covering no-id-field, null id, and valid single-ID forms. All pass.
- R-11 (N+1 pattern): Single `batch_entry_meta_lookup` call in `compute_knowledge_reuse_for_sessions` (tools.rs line 3300–3301). Confirmed via code review.
- R-12 (total_served deduplication): Three tests covering overlap, full overlap, disjoint sets. Confirmed via set union computation at knowledge_reuse.rs line 332.
- R-13 (fixture update completeness): Workspace build passes with zero errors; golden-output assertions contain no stale `"delivery_count"` JSON key assertions.

**Cargo test result** (confirmed live): All test crates pass, zero failures across the workspace.

---

### Test Coverage Completeness

**Status**: PASS

**Evidence**: The Risk-Based Test Strategy defined 13 risks with 37 test scenarios. The RISK-COVERAGE-REPORT maps every risk to at least one passing test. Coverage breakdown:
- R-01: Full (5 scenarios)
- R-02: Full (3 scenarios including the mandatory prefixed-name GATE test)
- R-03: Full (2 unit tests + advisory text assertion)
- R-04: Partial — boundary runtime test (501-ID path) covered by code review + constant assertion only; the test plan documents this as acceptable given the absence of a mock store. No objection.
- R-05 through R-13: Full

Integration test counts per RISK-COVERAGE-REPORT:
- Smoke: 23 passed / 0 failed
- Lifecycle: 48 passed / 0 failed / 5 xfail (pre-existing, GH#291/GH#406) / 2 xpass (pre-existing markers now passing, not caused by crt-049)
- Tools: 117 passed / 0 failed / 2 xfail (pre-existing)

Smoke gate (pytest -m smoke): 23/23 PASS.

All xfail markers verified to reference GH Issues (GH#291, GH#406) or documented CI constraints (no ONNX model). No xfail markers were added for crt-049. No integration tests were deleted or commented out. No crt-049-caused xfail failures.

The 2 xpassed lifecycle tests (`test_search_multihop_injects_terminal_active`, `test_inferred_edge_count_unchanged_by_cosine_supports`) are pre-existing xfail markers that started passing due to other feature work. This is not a failure — xpass is not a test failure under pytest's default mode. The tester agent recommends closing the corresponding GH issues, which is a cleanup follow-up, not a blocking issue.

---

### Specification Compliance

**Status**: PASS

**Evidence**: All 17 acceptance criteria verified against the SPECIFICATION.md functional and non-functional requirements:

Functional requirements FR-01 through FR-11:
- FR-01 (explicit_read_count field): types.rs line 296 — `pub explicit_read_count: u64` with `#[serde(default)]`. AC-01 PASS.
- FR-02 (explicit_read_by_category field): types.rs line 301 — `pub explicit_read_by_category: HashMap<String, u64>` with `#[serde(default)]`. AC-13 [GATE] PASS.
- FR-03 (rename delivery_count → search_exposure_count with aliases): types.rs lines 289–291 — stacked `#[serde(alias = "delivery_count")]`, `#[serde(alias = "tier1_reuse_count")]`. AC-02 [GATE] PASS.
- FR-04 (total_served excludes search exposures): knowledge_reuse.rs line 332 — `explicit_read_ids.union(&all_injection_ids).count()`. AC-14/AC-15 [GATE] PASS.
- FR-05 (extract_explicit_read_ids helper): knowledge_reuse.rs line 82 — implemented with full predicate. AC-03/AC-06/AC-12 PASS.
- FR-06 (attributed slice parameter): tools.rs function signature line 3216 — `attributed: &[unimatrix_observe::ObservationRecord]`. Call site line 1953 passes `&attributed`. AC-05/AC-07 PASS.
- FR-07 (existing fields unchanged): All legacy fields populate correctly; existing tests pass without modification. AC-11 PASS.
- FR-08 (zero-delivery guard): retrospective.rs line 998 — `if reuse.total_served == 0 && reuse.search_exposure_count == 0`. AC-17 [GATE] PASS.
- FR-09 (separate labeled lines): retrospective.rs lines 1011–1019 — "Search exposures (distinct)" and "Explicit reads (distinct)" lines present. AC-07 PASS.
- FR-10 (SUMMARY_SCHEMA_VERSION): cycle_review_index.rs line 35 — value 3. Advisory message at tools.rs 2724–2726 names the semantic change. AC-08 PASS.
- FR-11 (explicit_read_by_category breakdown): retrospective.rs lines 1056–1071 — "Explicit read categories" section rendered when non-empty. AC-07/FR-11 PASS.

Non-functional requirements:
- NFR-01 (no schema migration): Confirmed — no migration files in crt-049.
- NFR-02 (no new crate dependencies): Confirmed — no Cargo.toml changes.
- NFR-03 (no N+1 DB call): Single `batch_entry_meta_lookup` call, chunked at 100 per pattern #883.
- NFR-04 (no external API break): MCP tool signatures unchanged.
- NFR-05 (backward-compatible deserialization): Triple alias chain confirmed; all three key names deserialize correctly.

---

### Architecture Compliance

**Status**: PASS

**Evidence**: All five architecture components implemented as specified:

1. `unimatrix-observe/types.rs` (Component 1): `FeatureKnowledgeReuse` updated with all required fields and serde annotations. Matches architecture spec exactly.

2. `unimatrix-server/mcp/knowledge_reuse.rs` (Component 2): `extract_explicit_read_ids` implemented as standalone helper. `compute_knowledge_reuse` extended with `explicit_read_ids: &HashSet<u64>` and `explicit_read_meta: &HashMap<u64, EntryMeta>` parameters. total_served computed as set union. Matches architecture spec.

3. `unimatrix-server/mcp/tools.rs` (Component 3): `compute_knowledge_reuse_for_sessions` accepts `attributed: &[ObservationRecord]`. Calls `extract_explicit_read_ids`, applies ADR-004 cap (500), calls `batch_entry_meta_lookup` once for explicit reads. Call site at step 13–14 passes `&attributed`. Architecture data flow matches implementation.

4. `unimatrix-server/mcp/response/retrospective.rs` (Component 4): `render_knowledge_reuse` uses updated guard, displays "Entries served to agents (reads + injections)", "Search exposures (distinct)", "Explicit reads (distinct)", "Explicit read categories". Architecture render spec matches.

5. `unimatrix-store/cycle_review_index.rs` (Component 5): `SUMMARY_SCHEMA_VERSION = 3`. Architecture bump from 2 to 3 confirmed.

ADR compliance:
- ADR-001 (extract_explicit_read_ids as standalone helper in knowledge_reuse.rs): Confirmed.
- ADR-002 (triple-alias serde chain): Confirmed — stacked aliases on search_exposure_count.
- ADR-003 (total_served redefinition as explicit reads ∪ injections): Confirmed at knowledge_reuse.rs line 332.
- ADR-004 (cardinality cap 500 for explicit read batch lookup): Confirmed — `EXPLICIT_READ_META_CAP = 500` at tools.rs line 3204; applied before lookup at lines 3281–3293.

No architectural drift detected.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: Tester agent report (`crt-049-agent-4-tester-report.md`) contains `## Knowledge Stewardship` section with:
- `Queried: mcp__unimatrix__context_briefing — found entries #3806, #238, #4218, #748, #747`
- `Stored: nothing novel to store — all patterns applied (serde alias round-trip, normalize_tool_name coverage, golden-output assertions) already captured in #885, #4211, #3426`

Both `Queried:` and `Stored:` entries present with reasons. Compliant.

---

## Rework Required

None.

## Scope Concerns

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this feature had a clean gate 3c pass with complete coverage; no new recurring failure patterns to add (existing entries #885 triple-alias gate failures, #4211 normalize_tool_name silent zero are the relevant lessons and were already applied correctly).
