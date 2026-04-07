# Risk-Based Test Strategy: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Triple-alias serde chain on `search_exposure_count` silently produces 0 if any alias is missing or ordered incorrectly — stored rows with `"delivery_count"` or `"tier1_reuse_count"` keys deserialize as zero with no error signal | High | Med | Critical |
| R-02 | `normalize_tool_name` omission in `extract_explicit_read_ids` causes all hook-sourced observations (`mcp__unimatrix__context_get`) to be silently excluded, producing `explicit_read_count = 0` for all production cycles | High | Low | High |
| R-03 | `total_served` semantics change (search exposures removed) produces a silent value shift for any stored record re-reviewed after the version bump — advisory message content may not communicate the semantic change clearly enough | High | Med | High |
| R-04 | `explicit_read_by_category` category join cap (500 IDs) is hit silently — `explicit_read_count` is accurate but `explicit_read_by_category` is partial; ASS-040 Group 10 downstream consumer receives an incomplete map with no in-band signal | Med | Low | Medium |
| R-05 | Early-return zero-delivery guard retains the old condition (only checking `search_exposure_count == 0`) and short-circuits a cycle that has non-zero explicit reads — `explicit_read_count` is never computed and returned | Med | Med | Medium |
| R-06 | `render_knowledge_reuse` section-order regression — new "Explicit reads" and "Search exposures" lines appear in the wrong order or with incorrect labels, or the legacy "Distinct entries served" label persists | Med | Med | Medium |
| R-07 | `compute_knowledge_reuse_for_sessions` call site in `context_cycle_review` step 13–14 is not updated to pass `&attributed` — existing passing compilation masks the missing thread-through because the old signature still compiles with an empty-equivalent argument | Med | Low | Medium |
| R-08 | `SUMMARY_SCHEMA_VERSION` is not bumped to 3 — stale stored records are returned from cache for re-reviews without any advisory, silently omitting `explicit_read_count` and using old `total_served` semantics | High | Low | High |
| R-09 | `explicit_read_by_category` field missing from `FeatureKnowledgeReuse` or using an incorrect type breaks the AC-13 Group 10 contract — downstream feature requires a re-design pass before it can proceed | High | Low | High |
| R-10 | Filter-based `context_lookup` calls (no `id` in input) are not excluded — any filter-path lookup observation is incorrectly counted as an explicit read, inflating `explicit_read_count` | Med | Low | Medium |
| R-11 | `batch_entry_meta_lookup` is called with individual per-ID queries (N+1 pattern) instead of batched IN-clauses — performance degrades for cycles with many explicit reads; violates NFR-03 and C-03 | Med | Low | Medium |
| R-12 | `total_served` deduplication across explicit reads and injections is not applied — an entry appearing in both sets is counted twice, inflating `total_served` | Med | Med | Medium |
| R-13 | Test fixture updates for `delivery_count` → `search_exposure_count` rename are incomplete — existing golden-output tests that contain the literal string `"delivery_count"` in serialized JSON still pass at compile time but assert the wrong canonical key | Med | Med | Medium |

---

## Risk-to-Scenario Mapping

### R-01: Triple-alias serde chain silent zero
**Severity**: High
**Likelihood**: Med
**Impact**: Pre-existing `cycle_review_index.summary_json` rows containing `"delivery_count"` or `"tier1_reuse_count"` keys deserialize `search_exposure_count` as `0` on re-review, producing permanently incorrect knowledge reuse metrics. No error is raised; the metric silently degrades. Evidence: lesson #885 documents gate failures on serde-heavy types when alias tests are omitted.

**Test Scenarios**:
1. Construct a `FeatureKnowledgeReuse` JSON payload with key `"search_exposure_count"` and value `42`; deserialize and assert field equals `42`.
2. Construct a JSON payload with key `"delivery_count"` and value `42`; deserialize and assert `search_exposure_count` equals `42`.
3. Construct a JSON payload with key `"tier1_reuse_count"` and value `42`; deserialize and assert `search_exposure_count` equals `42`.
4. Serialize a `FeatureKnowledgeReuse` struct with `search_exposure_count = 42`; assert the output JSON key is `"search_exposure_count"` (not either alias).
5. Round-trip test: serialize then deserialize with each alias form; assert value is preserved end-to-end.

**Coverage Requirement**: All three deserialization alias paths and the canonical serialization key must have dedicated assertions. No alias may be validated only implicitly. These are AC-02 [GATE] items per lesson #885.

---

### R-02: normalize_tool_name omission produces silent zero explicit_read_count
**Severity**: High
**Likelihood**: Low
**Impact**: In production, all `PreToolUse` events from hook-driven observation recording carry the `mcp__unimatrix__` prefix. If `extract_explicit_read_ids` uses bare string comparison instead of `normalize_tool_name`, every hook-sourced observation is excluded and `explicit_read_count` is always `0`. There is no error, log, or assertion to detect this. Evidence: pattern #4211 (normalize_tool_name gotcha); confirmed production prefix behavior in `session_metrics.rs`.

**Test Scenarios**:
1. Pass a synthetic `ObservationRecord` slice with `tool = "mcp__unimatrix__context_get"` and valid `input["id"]`; assert the returned set is non-empty (AC-12d — [GATE]).
2. Pass a slice with `tool = "mcp__unimatrix__context_lookup"` and valid `input["id"]`; assert the entry ID is in the returned set.
3. Confirm that bare `"context_get"` (no prefix) is also matched — both forms must succeed.
4. Pass a slice with `tool = "mcp__unimatrix__context_search"` (not a read tool); assert it is excluded even after normalization.

**Coverage Requirement**: AC-06 [GATE] — a test with a prefixed tool name is non-negotiable. Bare-name tests alone do not cover the production case.

---

### R-03: total_served semantics change silent on stale records
**Severity**: High
**Likelihood**: Med
**Impact**: Any cycle re-reviewed after the version bump will compute `total_served` from `explicit_reads ∪ injections` rather than from search exposures. If the advisory message says only "schema version mismatch" (not specifically that `total_served` no longer includes search exposures), callers will not know to interpret the new metric. Stored records with `schema_version = 2` silently return `total_served` computed under old semantics until force-recomputed.

**Test Scenarios**:
1. Construct a scenario with explicit reads `{1, 2}` and injections `{2, 3}` and search exposures `{4, 5, 6}`; assert `total_served = 3` (not 5 or 6) — AC-15 [GATE].
2. Construct a scenario with `explicit_read_count = 0`, `injection_count = 0`, search exposures `{1, 2, 3}`; assert `total_served = 0` — AC-15 [GATE].
3. Verify the `SUMMARY_SCHEMA_VERSION = 2` advisory message text contains a description of the semantic change (not merely "schema version mismatch").
4. Trigger a re-review with a stored `schema_version = 2` record; assert advisory is returned; assert no full computation is run without `force = true`.

**Coverage Requirement**: AC-14 and AC-15 are [GATE] items. Deduplication behavior and search-exposure exclusion must each have an independent assertion. Advisory message wording must be verified as a string assertion.

---

### R-04: explicit_read_by_category silently partial at cap
**Severity**: Med
**Likelihood**: Low
**Impact**: For cycles with >500 distinct explicit reads (pathological but possible in large swarms), `explicit_read_by_category` tallies only the first 500 IDs. `explicit_read_count` is accurate but the category map is silently incomplete. ASS-040 Group 10 consumes this map as its primary input; partial data produces biased affinity scores with no diagnostic in the rendered output.

**Test Scenarios**:
1. Construct an explicit read ID set with exactly 500 IDs; assert `batch_entry_meta_lookup` is called once (no cap warning) and the map has expected entries.
2. Construct an explicit read ID set with 501 IDs; assert a `tracing::warn` is emitted and `explicit_read_by_category` contains at most 500 category tallies.
3. Assert `explicit_read_count` equals 501 (full set) even when the category map is capped at 500.
4. Confirm `EXPLICIT_READ_META_CAP = 500` constant exists in `tools.rs` near `compute_knowledge_reuse_for_sessions`.

**Coverage Requirement**: Boundary test at cap (500), at cap+1 (501), and a small set well below cap. The warning emission at the cap boundary is required behavior, not optional.

---

### R-05: Early-return guard retains old condition, short-circuits explicit-read-only cycles
**Severity**: Med
**Likelihood**: Med
**Impact**: A cycle with zero search exposures but non-zero explicit reads (e.g., an agent that called `context_get` but no `context_search`) hits the early-return path and returns an empty `FeatureKnowledgeReuse`. `explicit_read_count` is never populated. This is the scenario AC-09 is designed to catch.

**Test Scenarios**:
1. Call `compute_knowledge_reuse` (or `compute_knowledge_reuse_for_sessions`) with `search_exposure_count = 0`, `injection_count = 0`, `explicit_read_ids = {5}` (one ID); assert the returned struct has `explicit_read_count = 1` and is not an empty/default struct — AC-09.
2. Inspect the guard expression in code review: must read `total_served == 0 && search_exposure_count == 0` (AC-17 [GATE] condition).
3. Verify the old two-condition guard `search_exposure_count == 0 && injection_count == 0` has been removed.

**Coverage Requirement**: AC-09 is a required test. The guard condition must include `explicit_read_count` — a test that only validates non-zero-exposure cycles does not cover this risk.

---

### R-06: render_knowledge_reuse section-order regression
**Severity**: Med
**Likelihood**: Med
**Impact**: The rendered knowledge reuse section gains two new labeled lines and has one renamed label. Label inversion (e.g., "Search exposures" displayed where "Explicit reads" belongs) produces a misleading report. The legacy label "Distinct entries served" appearing in output means the `render_knowledge_reuse` function was not fully updated. Evidence: pattern #3426 documents formatter features consistently underestimating section-order regression risk.

**Test Scenarios**:
1. Construct a `FeatureKnowledgeReuse` with `search_exposure_count = 10`, `explicit_read_count = 3`, `total_served = 5`, `explicit_read_by_category = {"decision": 2, "pattern": 1}`; render via `render_knowledge_reuse` and assert: (a) "Entries served to agents (reads + injections)" appears before "Search exposures (distinct)" and "Explicit reads (distinct)"; (b) the value `5` appears on the "Entries served" line; (c) the value `10` appears on the "Search exposures" line; (d) the value `3` appears on the "Explicit reads" line; (e) "Explicit read categories" section is present with correct entries — AC-07.
2. Assert the string "Distinct entries served" does NOT appear in rendered output (legacy label guard).
3. Verify the zero-delivery guard uses `total_served == 0 && search_exposure_count == 0` (AC-17 [GATE]) — a struct with only `explicit_read_count > 0` produces non-empty output.

**Coverage Requirement**: Golden-output assertion covering full section label text and ordering is required per SR-04 and pattern #3426.

---

### R-07: attributed slice not threaded through to compute_knowledge_reuse_for_sessions
**Severity**: Med
**Likelihood**: Low
**Impact**: If the call site in `context_cycle_review` is not updated to pass `&attributed`, the compiler error catches the missing argument only if the signature change is implemented — if the parameter is given a default or the wrong overload is called silently, `explicit_read_count` remains `0` in all integration paths despite passing unit tests.

**Test Scenarios**:
1. Integration test: run a `context_cycle_review` call in a cycle that includes at least one `context_get` observation; assert `explicit_read_count > 0` in the returned report — AC-05.
2. Confirm (via code review) that the existing test `test_compute_knowledge_reuse_for_sessions_no_block_on_panic` passes `&[]` for the new `attributed` parameter (not left as the old signature).

**Coverage Requirement**: At least one integration-level test must verify end-to-end that observations in-memory produce a non-zero `explicit_read_count` in the final report. Unit tests on `extract_explicit_read_ids` alone are insufficient.

---

### R-08: SUMMARY_SCHEMA_VERSION not bumped
**Severity**: High
**Likelihood**: Low
**Impact**: If `SUMMARY_SCHEMA_VERSION` remains `2`, the cycle review cache serves stale records with `explicit_read_count = 0` and old `total_served` semantics for all existing stored cycles. No advisory is emitted. The metric is invisibly degraded for all historical reviews.

**Test Scenarios**:
1. Assert `SUMMARY_SCHEMA_VERSION == 3` in `cycle_review_index.rs` — update the existing `CRS-V24-U-01` forced-value assertion test — AC-08.
2. Simulate a stored record with `schema_version = 2`; assert `context_cycle_review` returns a stale-record advisory without computing.

**Coverage Requirement**: AC-08 is required. The version constant and its forced-value assertion test must both be updated — one without the other is incomplete.

---

### R-09: explicit_read_by_category field contract break for Group 10
**Severity**: High
**Likelihood**: Low
**Impact**: ASS-040 Group 10 (phase-conditioned category affinity) declares a hard dependency on `explicit_read_by_category: HashMap<String, u64>` populated via `batch_entry_meta_lookup`. Any deviation in field name, type, or category source requires a re-design pass of Group 10 before it can proceed. This is the AC-13 [GATE] contract.

**Test Scenarios**:
1. Construct a cycle with explicit reads against entries in categories `"decision"` (×2) and `"pattern"` (×1); assert `explicit_read_by_category = {"decision": 2, "pattern": 1}` — AC-13 [GATE].
2. Construct a cycle with no explicit reads; assert `explicit_read_by_category` is an empty map (not absent/null).
3. Assert `#[serde(default)]` is present — a `FeatureKnowledgeReuse` without the field deserializes to an empty map, not an error.
4. Verify category strings in the map match `entries.category` values (canonical strings, same domain as `by_category`).

**Coverage Requirement**: AC-13 is [GATE]. The field name and type must be verified by compilation and by a round-trip serde assertion. The category join semantics must be verified by an integration test that reads actual `entries.category` values.

---

### R-10: Filter-based context_lookup included in explicit reads
**Severity**: Med
**Likelihood**: Low
**Impact**: A filter-path `context_lookup` call (where `params.id IS NULL`) writes an observation with no `id` in the input JSON. If the extraction predicate does not enforce `input["id"].as_u64()` returning `Some(n)`, the observation is incorrectly counted as an explicit read.

**Test Scenarios**:
1. Pass a synthetic observation with `tool = "context_lookup"` and `input = {"query": "some text"}` (no `id` field); assert the returned ID set is empty — AC-04.
2. Pass a synthetic observation with `tool = "context_lookup"` and `input = {"id": null}`; assert excluded (null does not satisfy `as_u64()`).
3. Pass a synthetic observation with `tool = "context_lookup"` and `input = {"id": 42}`; assert ID `42` is in the returned set — single-ID path included — AC-12(c).

**Coverage Requirement**: All three input shapes for `context_lookup` must have explicit test coverage (AC-04 and AC-12 items).

---

### R-11: N+1 query pattern in batch_entry_meta_lookup for explicit reads
**Severity**: Med
**Likelihood**: Low
**Impact**: If `batch_entry_meta_lookup` is called once per explicit read ID instead of batching, a cycle with 50 explicit reads makes 50 DB round-trips instead of 1. This violates NFR-03 and C-03 and degrades review latency from sub-50ms to seconds.

**Test Scenarios**:
1. Confirm (code review / instrumentation) that `batch_entry_meta_lookup` is called exactly once for the explicit read ID set in `compute_knowledge_reuse_for_sessions`, not in a loop.
2. Confirm the existing 100-ID chunking logic is reused — no per-ID individual queries introduced.

**Coverage Requirement**: Structural code review check. If a query-count integration test is feasible (mock store counting calls), add it.

---

### R-12: total_served deduplication not applied
**Severity**: Med
**Likelihood**: Med
**Impact**: An entry appearing in both `explicit_read_ids` and `injection_entry_ids` is counted once in `total_served`. If the set union is computed incorrectly (e.g., as sum of counts instead of union of sets), `total_served` inflates. The display label "Entries served to agents (reads + injections)" implies deduplication — violation silently misrepresents how many distinct entries were consumed.

**Test Scenarios**:
1. Construct `explicit_read_ids = {1, 2}` and `injection_ids = {2, 3}`; assert `total_served = 3` (not 4) — overlapping ID `2` counted once — AC-15 [GATE].
2. Construct `explicit_read_ids = {1}` and `injection_ids = {1}`; assert `total_served = 1`.
3. Construct disjoint sets `explicit_read_ids = {1, 2}` and `injection_ids = {3}`; assert `total_served = 3`.

**Coverage Requirement**: AC-15 [GATE] — deduplication behavior must have an explicit test. The set union is the semantic core of the redefined metric.

---

### R-13: Incomplete fixture updates for delivery_count rename
**Severity**: Med
**Likelihood**: Med
**Impact**: Test fixtures in `retrospective.rs` and `types.rs` that construct `FeatureKnowledgeReuse` using the Rust field name `delivery_count` will fail to compile after the rename — this is caught at build time. However, test fixtures that contain the literal JSON string `"delivery_count"` in golden-output assertions will continue to compile while asserting the wrong canonical key name, silently weakening coverage.

**Test Scenarios**:
1. Search for literal strings `"delivery_count"` in test assertions in `retrospective.rs` and `types.rs`; each occurrence must be replaced with `"search_exposure_count"` or removed (golden-output tests must reflect the new canonical key).
2. Run full test suite — zero compilation errors from `delivery_count` field references.
3. Confirm no test fixture that serializes `FeatureKnowledgeReuse` asserts the literal key `"delivery_count"` in the output (output should be `"search_exposure_count"`).

**Coverage Requirement**: Fixture update completeness is verified by compilation (for struct field names) and by golden-output string review (for JSON key assertions).

---

## Integration Risks

**I-01 — `compute_knowledge_reuse` signature extension**: Adding `explicit_read_ids: &HashSet<u64>` and `explicit_read_meta: &HashMap<u64, EntryMeta>` parameters to `compute_knowledge_reuse` in `knowledge_reuse.rs` requires all existing direct test callers to be updated. The architecture documents one test caller (`test_compute_knowledge_reuse_for_sessions_no_block_on_panic`) that passes an empty `&[]`. Missing this update causes a compile error, but test files are often updated after the impl — a build-passing state with stale test signatures is possible during development.

**I-02 — Two `batch_entry_meta_lookup` calls in one function**: `compute_knowledge_reuse_for_sessions` now makes two calls to `batch_entry_meta_lookup` — the existing call for query_log+injection IDs and the new call for explicit read IDs. Both use the same store connection. If the store connection is held across both calls without await, or if the pool is saturated, the second call can deadlock. Verify the connection is released between awaits.

**I-03 — `attributed` slice ordering**: The extraction helper processes observations in slice order; deduplication via `HashSet` removes order dependency for the ID set. However, if upstream (step 12 of `context_cycle_review`) ever filters `attributed` by session subset rather than the full feature cycle, `explicit_read_count` will silently undercount. The constraint C-04 requires the unfiltered slice.

**I-04 — `ObservationRecord.tool` field optionality**: `ObservationRecord.tool` is `Option<String>`. The extraction predicate must handle `None` tool (records from non-tool observations) without panicking — `tool.as_deref().unwrap_or("")` is the correct pattern per ADR-001.

---

## Edge Cases

**E-01 — Empty attributed slice**: `extract_explicit_read_ids(&[])` must return an empty set, not an error. `explicit_read_count = 0`, `explicit_read_by_category = {}`. Covered by AC-12(e).

**E-02 — Duplicate explicit reads for the same ID**: An agent calling `context_get` for entry `42` ten times in one cycle; `explicit_read_count` must be `1` (distinct ID set), not `10`.

**E-03 — Input JSON with `id` as a float**: `input = {"id": 42.0}` — `as_u64()` may return `Some(42)` for `.0` floats in `serde_json`. Define expected behavior: if `as_u64()` succeeds, accept it; if it returns `None` (e.g., for `42.5`), exclude. Test with `id: 42.0` and `id: 42.5`.

**E-04 — Single-entry set for category join**: `explicit_read_ids = {7}` — `batch_entry_meta_lookup` must still return a populated map for a single ID. No accidental empty-slice skip guard.

**E-05 — All explicit reads for deleted/inactive entries**: `batch_entry_meta_lookup` returns an empty map if all IDs are absent from `entries`. `explicit_read_by_category` is empty; `explicit_read_count` is still accurate (sourced from ID set length, not meta lookup size).

**E-06 — Cycle with only injection signal (no explicit reads, no search)**: `explicit_read_count = 0`, `search_exposure_count = 0`, `injection_count > 0`; the early-return guard must NOT fire. `total_served = injection_count` (deduplicated over injection set).

---

## Security Risks

**S-01 — `input` JSON from untrusted hook events**: `ObservationRecord.input` is deserialized from a JSON blob written by the hook observation path. The `id` field is extracted as `Value::Number` via `as_u64()`. The risk is: a maliciously crafted hook event could inject an `id` of a non-existent or sensitive entry, inflating `explicit_read_count`. Blast radius is limited to the knowledge reuse metric — no write path is triggered, no ACL is bypassed. The `as_u64()` extraction cannot produce negative IDs or cause out-of-bounds access. Risk: Low.

**S-02 — `batch_entry_meta_lookup` SQL injection**: The SQL query uses parameterized IN-clauses with `?` placeholders (sqlx pattern). The explicit read ID vector is `Vec<u64>` — no string interpolation, no injection surface.

**S-03 — Category string from `entries.category`**: Category strings in `explicit_read_by_category` originate from the `entries` table (trusted store data), not from hook input. No injection or traversal risk from this field.

---

## Failure Modes

**F-01 — `batch_entry_meta_lookup` returns empty (store error or all IDs missing)**: `explicit_read_by_category` is empty. `explicit_read_count` is unaffected (computed from ID set length). The review report still shows the count; only the category breakdown is absent. No error propagated to caller — the empty map is valid per `#[serde(default)]`.

**F-02 — `extract_explicit_read_ids` encounters malformed input JSON**: Records where `input` is `None` or where the JSON object has no `id` field are silently skipped (excluded from the set). No panic, no error log. This is the correct behavior and is the exclusion predicate for filter-path lookups.

**F-03 — `normalize_tool_name` unavailable at call site**: This is a compile-time failure if the import is missing. The function is already used in `tools.rs` and re-exported from `unimatrix_observe`. No runtime failure mode.

**F-04 — Schema version advisory not triggered**: If `SUMMARY_SCHEMA_VERSION` is correct but `check_stored_review` logic is not updated to check version `3`, stale records are served silently. This is a separate code path from the version constant bump and requires its own verification.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — `total_served` semantics change silent breaking change | R-03, R-12 | ADR-003: consumer inventory confirmed zero external consumers; `total_served` redefined as `\|explicit_reads ∪ injections\|`; AC-14/AC-15 [GATE] tests required; SUMMARY_SCHEMA_VERSION bump surfaces to callers |
| SR-02 — Triple-alias serde chain load-bearing | R-01, R-13 | ADR-002: stacked `#[serde(alias)]` attributes mandated; three round-trip tests required (AC-02 [GATE]); fixture updates must replace literal `"delivery_count"` JSON strings |
| SR-03 — `batch_entry_meta_lookup` cardinality unbounded | R-04, R-11 | ADR-004: 500-ID cap applied before lookup; warning emitted at cap; `explicit_read_count` remains accurate; cap boundary test required |
| SR-04 — `render_knowledge_reuse` section-order regression risk | R-06 | Architecture enumerates exactly two new labeled lines and one renamed line; golden-output assertion covering full section required per pattern #3426 |
| SR-05 — `SUMMARY_SCHEMA_VERSION` re-review behavioral delta not surfaced | R-08 | Advisory message wording must name the `total_served` semantic change specifically (SPECIFICATION FR-10); test for advisory text content required |
| SR-06 — `normalize_tool_name` omission in new extraction path | R-02 | ADR-001: extraction helper in `knowledge_reuse.rs` mandated to call `normalize_tool_name`; AC-06 [GATE] prefixed-tool-name test is non-negotiable per pattern #4211 |
| SR-07 — `explicit_read_by_category` field contract for Group 10 | R-09 | AC-13 [GATE]: field name, type, and category join semantics locked; contract documented in SPECIFICATION domain model; any future change requires coordinated Group 10 update |

---

## Coverage Summary

| Priority | Risk Count | Required Test Scenarios |
|----------|-----------|------------------------|
| Critical | 1 (R-01) | AC-02 [GATE]: 5 serde round-trip scenarios (3 alias deserialization + 1 canonical serialization + 1 full round-trip) |
| High | 4 (R-02, R-03, R-08, R-09) | AC-06 [GATE] prefixed name (R-02); AC-14/AC-15 [GATE] total_served deduplication + exclusion (R-03); AC-08 version constant (R-08); AC-13 [GATE] category map contract (R-09) |
| Medium | 8 (R-04–R-07, R-10–R-13) | AC-09 early-return guard (R-05); AC-07 golden render output (R-06); AC-04/AC-12 filter-lookup exclusion (R-10); deduplication unit test (R-12); cap boundary at 500/501 (R-04); fixture update completeness (R-13); integration thread-through (R-07); batch query structure (R-11) |

Gate items (delivery merge blocked if failing): AC-02, AC-06, AC-13, AC-14, AC-15, AC-16, AC-17.

- AC-16 [GATE]: string-form ID handling (`{"id": "42"}`) — failure mode: systematic undercounting with no diagnostic
- AC-17 [GATE]: injection-only render guard (`total_served == 0 && search_exposure_count == 0`) — failure mode: injection knowledge silently suppressed in review report

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned gate failures — found #885 (serde-heavy types gate failure, alias tests omitted), #2758 (Gate 3c non-negotiable test grep), #3806 (handler integration tests absent); all directly inform R-01 severity rating and gate designation
- Queried: `/uni-knowledge-search` for risk patterns — found #4211 (normalize_tool_name bare-name tests mask production failure), #3426 (formatter section-order regression), #3442 (chunked batch IN-clause), #4213 (extract explicit reads from attributed slice); all incorporated as evidence
- Stored: nothing novel to store — R-02 (prefix normalization silent zero) is fully captured in pattern #4211 which already exists; all other risk patterns identified here are feature-specific to crt-049
