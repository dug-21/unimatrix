# Agent Report: crt-040-agent-2-spec

## Output

`product/features/crt-040/specification/SPECIFICATION.md`

## Status

Complete.

## Key Requirements Added Beyond SCOPE.md ACs

**AC-16 (impl Default trap)**: Added explicit acceptance criterion requiring that
`default_supports_cosine_threshold()` and `impl Default for InferenceConfig` both encode `0.65`
and are tested independently. This mirrors crt-041's AC-17 pattern (pattern #3817). SCOPE.md
called this out in the narrative but did not assign an AC-ID.

**AC-17 (nli_post_store_k removal completeness)**: Added explicit AC listing all six removal
sites: struct field, serde default fn, `impl Default` struct literal, `validate()` block, test
assertions, config merge branch. SCOPE.md specified the removal but did not enumerate all sites.
The grep-based verification method (`grep -n "nli_post_store_k"` returns zero) is the delivery
gate.

**AC-18 (serde forward compatibility)**: Added explicit AC requiring a unit test that
deserializes TOML containing the stale `nli_post_store_k = 5` field and asserts the result is
`Ok(_)`. Confirms `deny_unknown_fields` is not active. SCOPE.md mentioned this in Resolved
Decision 6 but did not assign an AC.

## Constraints Discovered from Codebase Inspection

1. **UNIQUE constraint confirmed**: `UNIQUE(source_id, target_id, relation_type)` — does NOT
   include `source`. Confirmed from `db.rs` DDL. Path B + Path C collision on same pair in same
   tick resolved silently by `INSERT OR IGNORE`. This is correct behavior; delivery must not
   treat it as a bug. SCOPE.md §Architecture Note flagged this as unverified; it is now confirmed.

2. **nli_post_store_k removal scope is larger than expected**: Six removal sites, including the
   config merge function (lines 2222-2227 in config.rs). The merge function uses an integer
   equality comparison (not f32 epsilon), so the removal pattern differs slightly from f32
   fields.

3. **write_graph_edge signature**: `write_nli_edge` in `nli_detection.rs` uses parameters
   `(store, source_id, target_id, relation_type, weight, created_at, metadata)`. The new
   `write_graph_edge` adds `source: &str` as an additional parameter. Exact position is TBD
   for architect — recommended: append as last parameter to minimize diff noise.

4. **candidate_pairs type**: `Vec<(u64, u64, f32)>` — tuple of `(source_id, target_id, cosine)`.
   Path C has direct access to cosine without any additional lookup. Source/target category
   values are NOT present in `candidate_pairs` — they are in `InformsMetadata` (Path A's
   structure). Path C needs category lookup. The SCOPE.md §candidate_pairs note says "source_category,
   target_category" are available — but this is in `InformsMetadata`, not in `candidate_pairs` tuples.
   The architect must specify how Path C retrieves category data for the filter. Options:
   (a) join `entry_meta` map already present in scope, (b) separate DB lookup, (c) build a
   parallel map. This is an open architectural question, not a specification ambiguity — the
   requirement (FR-01) stands; the mechanism is for the architect.

5. **serde deny_unknown_fields**: NOT active on `InferenceConfig` — confirmed by absence of
   the attribute in the struct. AC-18 verification is safe to write.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 17 entries returned; #4025, #3817, #3713,
  #3591 were directly actionable for spec requirements.
- Queried: `context_search("InferenceConfig serde default field validation range")` — confirmed
  dual-site change requirement.
- Queried: `context_search("graph cohesion metrics supports edge count eval gate")` — confirmed
  `supports_edge_count` source-agnostic; `inferred_edge_count` NLI-only.
