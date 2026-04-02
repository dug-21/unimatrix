# Agent Report: crt-040-agent-3-risk

## Deliverable

`product/features/crt-040/RISK-TEST-STRATEGY.md` — written.

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 1 | R-01 |
| High | 3 | R-02, R-03, R-12 |
| Medium | 7 | R-04, R-05, R-06, R-07, R-10, R-11, R-13 |
| Low | 2 | R-08, R-09 |

Total: 13 risks, 34 test scenarios.

## Architecture Gaps Requiring Resolution Before Delivery

### CRITICAL: R-01 — Category Data Gap in candidate_pairs

`candidate_pairs` is `Vec<(u64, u64, f32)>` — it carries only `(source_id, target_id, cosine)`. It contains no category information. AC-03 requires filtering by `[source_category, target_category]` against `informs_category_pairs`. The architecture specifies using `all_active: Vec<EntryRecord>` (Phase 2 pre-fetch) as the source for category resolution, but does not specify how the lookup is performed.

The delivery agent must choose one of:
1. **HashMap pre-build (recommended)**: Build `HashMap<u64, &str>` (entry_id → category) from `all_active` once after Phase 2, before Path C. O(1) lookup per candidate pair.
2. **Per-pair DB lookup (rejected)**: O(candidates × DB round-trip) on the hot path.

The IMPLEMENTATION-BRIEF must mandate option 1 explicitly. Without this specification, delivery will likely implement a linear scan over `all_active` per pair (O(n × candidates)), which is functionally correct but degrades as corpus grows.

### HIGH: R-13 — Config Merge Function

The config merge function must be updated for `supports_cosine_threshold`. Spec FR-08 states this requirement. The delivery agent must locate the merge function call site — it is not in the AC list's verification steps and has historically been missed (lesson #4013: spec names only a subset of test sites).

## Key Risks for Human Attention

1. **R-01** (Critical): The category data gap is a delivery blocker. Path C cannot implement AC-03 without a specified category resolution mechanism. The IMPLEMENTATION-BRIEF must resolve this before any code is written.

2. **R-02** (High): The `write_nli_edge` → `write_graph_edge` delegation must be tested by inspecting the `source` column value in the database. Compiler success does not verify correctness — the wrong string literal compiles.

3. **R-07** (Medium): Path B + Path C collision on the same tick returns `false` from `write_graph_edge`. Delivery must NOT treat `false` as an error. The distinction between `false` (IGNORE'd by UNIQUE constraint, expected) and SQL error (warn, unexpected) must be explicit in the implementation.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection graph inference tick"` — found #3579, #3723, #3668; tick observability gap (#3723) directly elevated R-06 severity
- Queried: `/uni-knowledge-search` for `"risk pattern graph edge write source tagging"` — found #4025 (write_nli_edge pattern), directly informs R-02
- Queried: `/uni-knowledge-search` for `"InferenceConfig serde default impl Default dual site"` — found #3817, #4013, #4014; all directly inform R-03, R-04, R-13
- Queried: `/uni-knowledge-search` for `"category lookup candidate_pairs tick missing data"` — no prior art for the category gap pattern
- Queried: `/uni-knowledge-search` for `"tick infallible error propagation unwrap warn continue"` — found #3897, #1542; confirms error handling pattern
- Stored: nothing novel — R-01 category gap is feature-specific; will store as cross-feature pattern if it recurs in crt-041 or Group 4
