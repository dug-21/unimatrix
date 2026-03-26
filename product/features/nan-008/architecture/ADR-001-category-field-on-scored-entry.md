## ADR-001: Add `category: String` to `ScoredEntry` in Both Type Copies

### Context

Computing CC@k and ICD requires knowing the category of each result entry.
`ScoredEntry` in `runner/output.rs` currently holds `id`, `title`,
`final_score`, `similarity`, `confidence`, `status`, and `nli_rerank_delta`.
The category is available on `se.entry.category` during the mapping step in
`run_single_profile` but is currently discarded.

Two approaches exist:
1. Compute CC@k and ICD inline during the mapping loop, before `ScoredEntry`
   is constructed, without persisting the category.
2. Add `category: String` to `ScoredEntry` so it is preserved in the output
   JSON and available to the report module's own copy.

The report module (`report/mod.rs`) has its own independent `ScoredEntry`
copy for deserializing result JSON without a compile-time dependency on runner.
If category is not persisted in the JSON, the report module cannot access it
for future per-entry display or category-based rendering.

The added output size is approximately 132 KB across 1761 scenarios × 5
entries × ~15 chars average category name — negligible.

### Decision

Add `category: String` to `ScoredEntry` in both `runner/output.rs` and
`report/mod.rs`. In the runner, populate it from `se.entry.category` during
the mapping step in `run_single_profile`. In the report copy, add
`#[serde(default)]` so pre-nan-008 result files (which lack the field)
deserialize without error and default to an empty string.

CC@k and ICD are then computed from the already-assembled `entries` vec in
`run_single_profile`, after mapping.

### Consequences

- `ScoredEntry` in both copies gains a `category` field.
- Result JSON files grow by approximately 132 KB total across all scenarios.
- Pre-nan-008 result JSON files continue to deserialize in `eval report` due
  to `#[serde(default)]`.
- Future metrics or report features can use category information without
  further runner changes.
- Both type copies must be updated together (SR-01 risk; enforced by the
  round-trip integration test specified in ADR-003).
