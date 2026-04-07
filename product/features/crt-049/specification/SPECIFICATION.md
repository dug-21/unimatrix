# SPECIFICATION: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

## Objective

The `context_cycle_review` knowledge reuse metric currently counts search exposures (entries appearing in query results) as a proxy for knowledge consumption. This is a weak signal. `context_get` and `context_lookup` (single-ID) calls are unambiguous signals of intentional knowledge consumption and are already recorded in the `observations` table. This feature adds `explicit_read_count` and `explicit_read_by_category` to `FeatureKnowledgeReuse`, renames `delivery_count` to `search_exposure_count` with full backward-compat alias chain, redefines `total_served` as the deduplicated union of explicit reads and injections (excluding search exposures), and bumps `SUMMARY_SCHEMA_VERSION` to 3.

---

## Functional Requirements

**FR-01** — `FeatureKnowledgeReuse` must expose a field `explicit_read_count: u64` with `#[serde(default)]` representing the count of distinct entry IDs explicitly read by agents during the cycle.

**FR-02** — `FeatureKnowledgeReuse` must expose a field `explicit_read_by_category: HashMap<String, u64>` with `#[serde(default)]` tallying explicit reads per `entries.category` string, populated via `batch_entry_meta_lookup` on the extracted ID set.

**FR-03** — The field currently named `delivery_count` must be renamed to `search_exposure_count`. The field must carry `#[serde(alias = "delivery_count", alias = "tier1_reuse_count")]` to preserve deserialization of all stored `cycle_review_index.summary_json` rows and any external consumer that wrote either historical name.

**FR-04** — `total_served` must be redefined as the count of distinct entry IDs in the deduplicated union of the explicit read ID set and the injection ID set. Search exposures (query result IDs) must NOT contribute to `total_served`.

**FR-05** — An extraction helper `extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>` must be implemented in `knowledge_reuse.rs`. It must apply the exact filter predicate defined in the Domain Models section.

**FR-06** — `compute_knowledge_reuse_for_sessions` must accept the `attributed: &[ObservationRecord]` slice and pass it through to `compute_knowledge_reuse`. The call site in `context_cycle_review` step 13-14 must supply `&attributed`.

**FR-07** — `compute_knowledge_reuse` must continue to load `query_log` and `injection_log` records and populate `search_exposure_count`, `cross_session_count`, `by_category`, `cross_feature_reuse`, `intra_cycle_reuse`, and `top_cross_feature_entries` unchanged.

**FR-08** — The early-return zero-delivery guard in `render_knowledge_reuse` must use the condition: `total_served == 0 && search_exposure_count == 0`. A cycle with zero search exposures but non-zero `total_served` (e.g., injection-only cycle with injections present) must NOT short-circuit rendering.

**FR-09** — `render_knowledge_reuse` must display separate labeled lines for search exposures and explicit reads when either is non-zero. The label previously reading "Distinct entries served" must be updated to "Entries served to agents (reads + injections)".

**FR-10** — `SUMMARY_SCHEMA_VERSION` (defined in `crates/unimatrix-store/src/cycle_review_index.rs`) must be bumped from `2` to `3`. Pre-existing stored rows deserializing with `explicit_read_count` absent will default to `0` via `#[serde(default)]`; `total_served` will differ on re-review due to the semantics change — the stale-record advisory message must communicate semantic change, not merely schema version mismatch.

**FR-11** — `render_knowledge_reuse` must display a per-category breakdown from `explicit_read_by_category` when the map is non-empty, labeled "Explicit read categories".

---

## Non-Functional Requirements

**NFR-01 — No schema migration.** The `observations` table already contains `input` (JSON) and `phase`. No new columns, tables, or migration steps are introduced.

**NFR-02 — No new crate dependencies.** `ObservationRecord` is in `unimatrix-core`, which `unimatrix-server` already depends on. `FeatureKnowledgeReuse` is in `unimatrix-observe`. No new inter-crate edges are introduced.

**NFR-03 — No new DB round-trip for explicit reads.** Explicit read extraction operates entirely on the `attributed` in-memory slice. The only new DB call is the `batch_entry_meta_lookup` for `explicit_read_by_category`, which must use a single batched IN-clause (not N individual reads). Per ADR-003 (col-026): chunk at 100 IDs, target ≤ 50ms for ≤ 100 entries on a warm DB.

**NFR-04 — No external API break.** The `compute_knowledge_reuse` signature change (adding `attributed`) affects only the server crate's internal call site. No MCP tool signature changes.

**NFR-05 — Backward-compatible deserialization.** JSON stored in `cycle_review_index.summary_json` under either `"delivery_count"` or `"tier1_reuse_count"` must round-trip correctly into `search_exposure_count` without data loss or error.

---

## Acceptance Criteria

All AC-IDs flow from SCOPE.md. Items marked **[GATE]** are non-negotiable — a failing gate item blocks delivery merge.

**AC-01** — `FeatureKnowledgeReuse` has a field `explicit_read_count: u64` with `#[serde(default)]`.
Verification: struct definition inspection + serialization round-trip test with absent field deserializing to `0`.

**AC-02 [GATE] — Triple-alias serde chain** — `search_exposure_count` carries `#[serde(alias = "delivery_count", alias = "tier1_reuse_count")]`. All three names (`"search_exposure_count"`, `"delivery_count"`, `"tier1_reuse_count"`) must deserialize into the same field without error or data loss.
Verification: dedicated round-trip test constructing JSON with each of the three key names and asserting the deserialized value is identical. This test is non-negotiable (SR-02: serde-heavy types cause gate failures when alias tests are omitted — lesson #885).
Failure mode: if either alias is missing, stored `cycle_review_index.summary_json` rows containing the historical name silently deserialize `search_exposure_count` as `0`, producing incorrect metrics on re-review without any error signal.

**AC-03** — `extract_explicit_read_ids` returns the set of distinct entry IDs from `PreToolUse` observations where the normalized tool name is `context_get` or `context_lookup` and `input["id"]` is a non-null integer.
Verification: unit test with synthetic `ObservationRecord` slice.

**AC-04** — Filter-based `context_lookup` calls (no `id` field in `input`, or `input["id"]` is null) are excluded from `explicit_read_count`.
Verification: unit test with a filter-path `context_lookup` observation — result set must be empty.

**AC-05** — `compute_knowledge_reuse_for_sessions` accepts and uses the `attributed` observation slice to populate `explicit_read_count`; the field is non-zero when agents made explicit reads during the cycle.
Verification: integration test with synthetic observations containing valid explicit reads.

**AC-06 [GATE] — `normalize_tool_name` mandatory** — Tool names with the `mcp__unimatrix__` prefix (e.g., `mcp__unimatrix__context_get`) are correctly stripped before matching. The extraction logic must call `normalize_tool_name` — bare string comparison is not acceptable.
Verification: unit test AC-12(d) with a prefixed tool name must pass; this test is non-negotiable (SR-06: prefix normalization is a new code path not covered by existing tests).
Failure mode: if `normalize_tool_name` is not applied, all production hook-sourced observations (which carry the `mcp__unimatrix__` prefix) will be silently excluded from `explicit_read_count`, producing `0` for all hook-driven cycles with no diagnostic.

**AC-07** — `render_knowledge_reuse` outputs separate labeled lines for "Search exposures" and "Explicit reads" when either is non-zero.
Verification: golden-output assertion covering the full rendered section including label text and ordering (SR-04: section-order regression risk).

**AC-08** — `SUMMARY_SCHEMA_VERSION` is `3`; the constant assertion test `CRS-V24-U-01` is updated to assert `3`.
Verification: test execution passes with updated assertion.

**AC-09** — A cycle with zero search exposures but non-zero explicit reads does not hit the early-return zero-delivery path and returns a populated `FeatureKnowledgeReuse`.
Verification: unit test constructing a scenario with `search_exposure_count == 0`, `explicit_read_count > 0`.

**AC-10** — Existing serialization round-trip tests for `FeatureKnowledgeReuse` pass with the renamed field.
Verification: CI test suite green — no existing test modified to weaken alias coverage.

**AC-11** — `compute_knowledge_reuse_for_sessions` still loads `query_log` and `injection_log` and populates `search_exposure_count`, `cross_session_count`, `by_category`, `cross_feature_reuse`, `intra_cycle_reuse`, and `top_cross_feature_entries` as before.
Verification: existing knowledge reuse tests continue to pass without modification.

**AC-12** — New unit tests cover:
- (a) explicit reads extracted from `context_get` observations
- (b) filter `context_lookup` (no `id` field) excluded
- (c) single-ID `context_lookup` (with `id` field) included
- (d) prefixed tool name `mcp__unimatrix__context_get` correctly matched after normalization **[GATE — see AC-06]**
- (e) empty observations produces `explicit_read_count = 0`

**AC-13 [GATE] — `explicit_read_by_category` field** — `FeatureKnowledgeReuse` has `explicit_read_by_category: HashMap<String, u64>` with `#[serde(default)]`. It is populated by calling `batch_entry_meta_lookup` once on the full extracted explicit read ID set and tallying counts per category string. This field is a cycle-level category breakdown (no phase dimension) used as a human-facing reporting and correctness signal. It is NOT the training input for Group 10 — Group 10 requires phase-stratified `(phase, category)` aggregates from `observations` directly (out of scope per C-08). The field name, type, and join source are frozen as a [GATE] contract.
Verification: unit test with synthetic ID set producing a known category distribution; assert map contents match expected tallies.
Failure mode: if this field is absent, has a different type, or uses a different category source, Group 10 must perform an additional re-design pass before it can proceed.

**AC-14 [GATE] — `total_served` semantics** — `total_served` is the count of distinct entry IDs in `explicit_reads ∪ injections` (deduplicated set union). Search exposure IDs must NOT be included in this union. The display label must read "Entries served to agents (reads + injections)".
Verification: unit test asserting `total_served` equals `|explicit_read_ids ∪ injection_ids|`, not `|query_result_ids|` or any combination including query results.
Failure mode: if search exposures are included, `total_served` inflates by up to an order of magnitude relative to actual consumption, making it meaningless as a served-knowledge metric.

**AC-15 [GATE] — `total_served` unit test** — A unit test verifies `total_served` is the count of distinct entry IDs appearing in either explicit reads or injections, and that an entry appearing only in search results does NOT increase `total_served`.
Verification: synthetic scenario where an entry ID appears only in `query_result_ids` but not in explicit reads or injections; assert `total_served` is unchanged.
Failure mode: same as AC-14.

**AC-16 [GATE] — String-form ID handling** — `extract_explicit_read_ids` correctly handles string-form IDs (`{"id": "42"}`) in addition to integer-form (`{"id": 42}`). Both forms must extract the same `u64` value. A unit test covers both forms explicitly.
Verification: unit test with two synthetic `ObservationRecord` entries — one with `input["id"]` as a JSON number and one as a JSON string — asserting both are present in the returned `HashSet<u64>`.
Failure mode: if only `as_u64()` is applied without the string-parse fallback, `GetParams`-compatible string-form IDs are silently dropped from `explicit_read_count`, producing systematic undercounting with no diagnostic.

**AC-17 [GATE] — Injection-only cycle render guard** — An injection-only cycle (injections present, zero search exposures, zero explicit reads) does NOT trigger the early-return render guard. `render_knowledge_reuse` produces output showing `total_served > 0`. The render guard condition is `total_served == 0 && search_exposure_count == 0`, not `search_exposure_count == 0 && explicit_read_count == 0`.
Verification: unit test constructing a scenario with `injection_ids` non-empty, `search_exposure_count == 0`, `explicit_read_count == 0`; assert rendered output is non-empty and includes `total_served > 0`.
Failure mode: injection knowledge served to agents is silently suppressed in the review report — the most reliable knowledge delivery path produces zero signal in the metrics output.

---

## Domain Models

### FeatureKnowledgeReuse (after crt-049)

The central struct in `unimatrix-observe/src/types.rs` representing knowledge reuse metrics for one feature cycle.

| Field | Type | Serde | Semantics |
|---|---|---|---|
| `search_exposure_count` | `u64` | `alias = "delivery_count"`, `alias = "tier1_reuse_count"` | Count of distinct entry IDs returned in query result sets during the cycle. Does NOT imply the agent read the entry. |
| `explicit_read_count` | `u64` | `default` | Count of distinct entry IDs explicitly retrieved by agents via `context_get` or single-ID `context_lookup`. Unambiguous consumption signal. |
| `explicit_read_by_category` | `HashMap<String, u64>` | `default` | Per-category tally of explicit read IDs, joined via `batch_entry_meta_lookup`. Cycle-level breakdown (no phase dimension); used as a human-facing reporting and correctness signal. NOT the primary Group 10 training input — Group 10 requires phase-stratified `(phase, category)` aggregates from `observations` directly (C-08). Category strings match `entries.category` values. |
| `cross_session_count` | `u64` | — | Distinct entry IDs delivered (via search exposure) across more than one session in the cycle. Unchanged by this feature. |
| `by_category` | `HashMap<String, u64>` | — | Per-category tally of search exposures. Sourced from query_log. Unchanged by this feature. Relabeled "Search exposure categories" in rendering. |
| `category_gaps` | `Vec<String>` | — | Categories with zero search exposures. Unchanged. |
| `total_served` | `u64` | — | **Redefined**: count of distinct entry IDs in `explicit_read_ids ∪ injection_ids`. Search exposures excluded. |
| `total_stored` | `u64` | — | Total active entries in the store at review time. Unchanged. |
| `cross_feature_reuse` | `...` | — | Unchanged. |
| `intra_cycle_reuse` | `...` | — | Unchanged. |
| `top_cross_feature_entries` | `Vec<...>` | — | Unchanged. |

### Served Entry vs. Search Exposure — Central Distinction

This distinction is the semantic core of this feature:

**Search Exposure** (`search_exposure_count`): An entry ID that appeared in the result set of a `context_search` or UDS search-path call. The agent received a list that included this entry. There is no guarantee the agent read, used, or was even aware of that specific entry. This is a delivery-side metric.

**Served Entry** (`total_served`): An entry that was actively consumed by an agent — either explicitly retrieved by ID (`context_get` / single-ID `context_lookup`), or injected directly into the agent's context by the hook system (`injection_log`). These are confirmed consumption events. This is a consumption-side metric. Note: `ObservationRecord.input` for hook-sourced events arrives as `Some(Value::String(raw_json))`; direct MCP calls produce `Some(Value::Object(_))`. Extraction must handle both forms (see Explicit Read Filter Predicate).

An entry can be a search exposure without ever being served. An entry can be served (via injection) without ever appearing as a search exposure. An entry explicitly read was almost certainly first found via search — but `total_served` does not deduplicate against search exposures; it deduplicates only across the two served sources (reads and injections).

### Explicit Read Filter Predicate

An `ObservationRecord` from the `attributed` slice qualifies as an explicit read if and only if all of the following hold:

1. `record.event_type == EventType::PreToolUse`
2. `normalize_tool_name(&record.tool)` is one of `{"context_get", "context_lookup"}`
3. `record.input` is `Some(Value::String(_))` OR `Some(Value::Object(_))`
4. After parsing — `Value::String(s)` branch via `serde_json::from_str(s)`, `Value::Object(_)` branch used as-is — the resulting object has a field `id`
5. The `id` value parses to a valid `u64`: try `as_u64()` first, then `as_str().and_then(|s| s.parse().ok())` to handle string-form IDs (e.g., `{"id": "42"}`)

Hook-listener sourced `ObservationRecord.input` arrives as `Some(Value::String(raw_json))` (the listener stores the raw JSON string without parsing — confirmed at `listener.rs:1911`). Direct MCP calls produce `Some(Value::Object(_))`. Both forms must be handled. Condition 5 is also the natural exclusion predicate for filter-based `context_lookup` calls: when `params.id` is absent, the parsed object has no `id` field, which fails both `as_u64()` and the string parse without special casing.

### `normalize_tool_name`

Defined in `unimatrix_observe`. Strips the `mcp__unimatrix__` prefix from tool names. Must be applied before any string comparison against `"context_get"` or `"context_lookup"`. Hook-sourced `PreToolUse` events carry the prefix; direct MCP call events do not. Both forms must match.

### SUMMARY_SCHEMA_VERSION

Integer constant in `crates/unimatrix-store/src/cycle_review_index.rs`. Bump policy (ADR-002 crt-033): bump when any field is added, removed, or renamed on `RetrospectiveReport` or any nested type affecting JSON round-trip fidelity. Adding `explicit_read_count` and `explicit_read_by_category` and redefining `total_served` all qualify. Value: `3` (was `2` after crt-047).

---

## User Workflows

### Workflow 1: Agent calls context_cycle_review after a feature session

1. Agent calls `context_cycle_review` with `feature_id`.
2. Server loads attributed observations for the cycle's sessions (already occurs before step 13).
3. `compute_knowledge_reuse_for_sessions` is called with `session_records` AND `&attributed`.
4. `extract_explicit_read_ids` filters the attributed slice using the predicate above.
5. `batch_entry_meta_lookup` is called once on the extracted ID set to get categories.
6. `total_served` is computed as `|explicit_read_ids ∪ injection_ids|`.
7. `render_knowledge_reuse` emits labeled lines for both "Search exposures (distinct)" and "Explicit reads (distinct)".
8. Report is returned to agent with the new fields visible.

### Workflow 2: Agent re-reviews a cycle with a stored summary (version mismatch)

1. Agent calls `context_cycle_review` for a cycle whose stored `summary_json` has `schema_version = 2`.
2. `SUMMARY_SCHEMA_VERSION` is now `3`; mismatch detected.
3. Stale-record advisory is returned: message must indicate the semantic change (not merely "schema version mismatch") — e.g., "schema_version 2 predates explicit read signal and total_served redefinition; use force=true to recompute".
4. On `force=true`, the full computation runs. `explicit_read_count` will be populated from observations; `total_served` will reflect reads+injections only.

### Workflow 3: ASS-040 Group 10 consumes explicit_read_by_category

1. Group 10 calls `context_cycle_review` or accesses `FeatureKnowledgeReuse` from stored data.
2. `explicit_read_by_category` is present as a `HashMap<String, u64>`.
3. Group 10 uses this as the per-category read signal to compute phase-conditioned category affinity.
4. The category strings in this map are canonical `entries.category` values (same domain as existing `by_category` field).

---

## Constraints

**C-01** — Serde alias chain on `search_exposure_count` must carry both `"delivery_count"` AND `"tier1_reuse_count"`. Dropping either alias silently corrupts metrics for pre-existing stored rows.

**C-02** — `SUMMARY_SCHEMA_VERSION` bump to `3` is mandatory. Skipping it causes the server to return stale cached records without `explicit_read_count` and with old `total_served` semantics, with no advisory signal to the caller.

**C-03** — `batch_entry_meta_lookup` for `explicit_read_by_category` must be a single batched IN-clause call, chunked at 100 IDs per ADR-003 (col-026). An N+1 per-ID query is not acceptable.

**C-04** — The `attributed` slice passed into `compute_knowledge_reuse_for_sessions` must be the same unfiltered slice loaded at step 13 of `context_cycle_review`. Any upstream truncation or session-level filtering would silently undercount `explicit_read_count`.

**C-05** — `FeatureKnowledgeReuse` is defined in `unimatrix-observe`. The extraction helper `extract_explicit_read_ids` lives in the server crate (`knowledge_reuse.rs`) and receives `&[ObservationRecord]` from `unimatrix-core`. No new inter-crate dependencies are introduced.

**C-06** — `total_served` must NOT include search exposure IDs. Adding them would conflate consumption with delivery, making the served-knowledge metric meaningless.

**C-07** — `cross_session_count` extension to cover explicit reads is explicitly out of scope. Do not implement.

**C-08** — Phase-stratified breakdowns of explicit reads are explicitly out of scope. `phase` is available on `ObservationRecord` but must not be used in this feature. Group 10 owns phase aggregation.

---

## Dependencies

| Dependency | Kind | Note |
|---|---|---|
| `unimatrix-observe/src/types.rs` | Internal | `FeatureKnowledgeReuse` struct definition — modified |
| `unimatrix-server/src/mcp/knowledge_reuse.rs` | Internal | `compute_knowledge_reuse`, `extract_explicit_read_ids` — modified/added |
| `unimatrix-server/src/tools.rs` | Internal | Call site in `context_cycle_review` step 13-14 — modified |
| `unimatrix-server/src/mcp/retrospective.rs` | Internal | `render_knowledge_reuse` — rendering labels modified |
| `unimatrix-store/src/cycle_review_index.rs` | Internal | `SUMMARY_SCHEMA_VERSION` — bumped to 3 |
| `unimatrix-core/src/observation.rs` | Internal | `ObservationRecord` type — read-only dependency, no changes |
| `unimatrix_observe::normalize_tool_name` | Internal | Must be called before tool name comparison — no changes to the function |
| `batch_entry_meta_lookup` | Internal | DB batch query for `(id, category)` — called once per review for explicit read ID set |

No new external crates, no new DB tables, no MCP protocol changes.

---

## NOT in Scope

- Removing `search_exposure_count`, `query_log` loading, or `injection_log` loading. Both remain.
- Adding `context_get` or `context_lookup` writes to `query_log`. Their observation recording is already correct and unchanged.
- Deduplicating explicit reads against search exposures. The two metrics are independently meaningful.
- Extending `cross_session_count` to cover explicit reads.
- Phase-stratified explicit read breakdowns (Group 10 scope).
- Adding `phase` breakdowns to any field in `FeatureKnowledgeReuse`.
- Extending `PhaseFreqTable` or the `query_log`-based phase frequency pipeline.
- New DB tables, columns, or schema migrations.
- Any changes to how `context_get` or `context_lookup` write observations.
- Phase-conditioned category affinity computation (ASS-040 Group 10 — depends on this feature, not part of it).

---

## Open Questions

None. All open questions from SCOPE.md (OQ-01 through OQ-03) are resolved. See SCOPE.md for resolution details.

Architect attention required on:
- **SR-03** (from risk assessment): `batch_entry_meta_lookup` cardinality bound. The spec requires chunking at 100 per ADR-003 but the architect should confirm whether an explicit upper cap per cycle is warranted (e.g., warn if explicit read set exceeds N entries) or whether the 100-chunk batching is sufficient.
- **SR-05**: Advisory message wording for schema version mismatch. The message should communicate the `total_served` semantics change specifically, not just the version number.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found pattern #4213 (explicit read extraction from attributed slice, normalize_tool_name requirement), ADR #3794 (SUMMARY_SCHEMA_VERSION unified bump policy), ADR #3423 (batch IN-clause for entry meta lookup, chunking at 100). All three directly inform this specification.
