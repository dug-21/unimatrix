# Scope Risk Assessment: crt-049

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `total_served` semantics change is a silent breaking change for any consumer that treats `total_served ≈ delivery_count`; no consumer inventory exists in scope | High | Med | Architect must identify all callers of `total_served` (rendering, retrospective, external JSON consumers) and confirm the alias chain on `search_exposure_count` covers stored rows without a schema migration |
| SR-02 | Triple-alias serde chain (`tier1_reuse_count` → `delivery_count` → `search_exposure_count`) on a single field is load-bearing; alias ordering and Rust's serde resolution rules are non-obvious | Med | Med | Spec must mandate backward-compat round-trip tests for all three alias names against stored `cycle_review_index.summary_json` rows (lesson #885: serde-heavy types cause gate failures when tests are omitted) |
| SR-03 | `batch_entry_meta_lookup` join on the extracted explicit-read ID set introduces a DB call inside `compute_knowledge_reuse` whose cost scales with read set size; no cardinality bound stated in scope | Med | Low | Architect should specify a cardinality guard or batching strategy; an unbounded query against `entries` at review time could be slow for high-volume cycles |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `render_knowledge_reuse` output label changes affect the `context_cycle_review` rendered report; scope defers section-order regression checking but adds two new labeled lines and renames one | Med | Med | Spec should require a section-order / golden-output assertion covering the full rendered report to catch label inversions (pattern #3426: formatter features consistently underestimate section-order regression risk) |
| SR-05 | `SUMMARY_SCHEMA_VERSION` bump to 3 forces stale-record advisory on all prior stored rows; scope says `explicit_read_count` will deserialize as `0` via `#[serde(default)]`, but `total_served` will also silently recalculate differently on re-review — this behavioral delta is not surfaced to callers | Low | Med | Spec should document the re-review behavior delta and confirm the advisory message wording communicates the semantic change, not merely a schema version mismatch |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | The `attributed` observations slice is in-scope as the source for explicit reads, but tool names in `observations.tool` carry the `mcp__unimatrix__` prefix on hook-sourced events; `normalize_tool_name` must be applied — scope confirms this, but the extraction helper is a new code path not covered by existing tests | High | Low | Spec must include a non-negotiable test case for prefixed tool names (AC-06/AC-12d already stated — verify these are marked non-negotiable in the test plan) |
| SR-07 | ASS-040 Group 10 (phase-conditioned category affinity) explicitly depends on `explicit_read_by_category` being shipped in this feature; any slip in the `explicit_read_by_category` contract (field name, category join semantics) will require rework in that downstream feature | Med | Low | Architect should define the `explicit_read_by_category` field contract precisely enough for Group 10 to stub against; consider adding a forward-compatibility note in the spec |

## Assumptions

- **SCOPE.md §Background / Observations Table Structure**: Assumes `ObservationRecord.input` is always parseable as `serde_json::Value` for `context_get` and `context_lookup` calls. If any path writes a non-JSON or truncated `input`, ID extraction silently produces zero results — no error, no signal.
- **SCOPE.md §Proposed Approach Step 3**: Assumes the `attributed` slice at step 13 of `context_cycle_review` is already fully loaded and contains all relevant sessions' observations. If the slice is filtered or truncated upstream, `explicit_read_count` will undercount without any diagnostic.
- **SCOPE.md §Non-Goals**: Assumes `cross_session_count` extension is cleanly deferrable — if the new `explicit_read_count` metric is immediately compared against `cross_session_count` by consumers, the asymmetry (search-exposures-only cross-session vs. observations-based explicit reads) may confuse report readers.

## Design Recommendations

- **SR-01 + SR-02**: Architect must enumerate `total_served` consumers before finalizing the signature; spec writer must mandate serde round-trip tests for all three alias names on `search_exposure_count` as non-negotiable test cases.
- **SR-03**: Architect should add a cardinality cap or paginated batch strategy for `batch_entry_meta_lookup` in `compute_knowledge_reuse`; document the expected upper bound per cycle.
- **SR-04**: Spec writer should include a golden-output section-order test covering all labeled lines in `render_knowledge_reuse` output to prevent label-inversion regressions.
- **SR-06**: Mark AC-06 and AC-12d as non-negotiable in the test plan; gate validators must verify prefixed-tool-name handling is tested, not assumed.
