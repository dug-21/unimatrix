# OVERVIEW: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

## Purpose

Replace the search-exposure proxy in `context_cycle_review`'s knowledge reuse metric
with an explicit read signal derived from `context_get` and single-ID `context_lookup`
observations already loaded in memory. Six components are modified.

---

## Components Involved

| Component | File | Change Kind |
|-----------|------|-------------|
| `FeatureKnowledgeReuse` | `unimatrix-observe/src/types.rs` | Rename + add fields + semantic change |
| `extract_explicit_read_ids` | `unimatrix-server/src/mcp/knowledge_reuse.rs` | New pure helper |
| `compute_knowledge_reuse` | `unimatrix-server/src/mcp/knowledge_reuse.rs` | Extended signature + new computations |
| `compute_knowledge_reuse_for_sessions` | `unimatrix-server/src/mcp/tools.rs` | New param + orchestration steps |
| `render_knowledge_reuse` | `unimatrix-server/src/mcp/response/retrospective.rs` | Guard fix + new labeled lines |
| `SUMMARY_SCHEMA_VERSION` | `unimatrix-store/src/cycle_review_index.rs` | Constant bump 2 → 3 |

---

## Data Flow

```
context_cycle_review handler (tools.rs)
    |
    +-- Step 12: attributed = load_attributed_observations()
    |       Vec<ObservationRecord> — hook-sourced input arrives as Value::String(raw_json)
    |
    +-- Step 13-14: compute_knowledge_reuse_for_sessions(
    |       store, session_records, feature_cycle, &attributed   [NEW param]
    |   )
    |       |
    |       +-- DB: scan_query_log_by_sessions()     -> Vec<QueryLogRecord>
    |       +-- DB: scan_injection_log_by_sessions() -> Vec<InjectionLogRecord>
    |       +-- DB: count_active_entries_by_category() -> HashMap<String, u64>
    |       |
    |       +-- extract_explicit_read_ids(&attributed) [in-memory, no DB]
    |       |       filters PreToolUse + normalize_tool_name -> context_get/context_lookup
    |       |       two-branch parse: Value::String(s) -> from_str(s).ok()
    |       |                         Value::Object(_) -> use as-is
    |       |       id extraction: as_u64() then as_str().parse().ok()
    |       |       returns: HashSet<u64>
    |       |
    |       +-- cardinality cap (EXPLICIT_READ_META_CAP = 500)
    |       |       warn + truncate lookup_ids if len > 500
    |       |       explicit_read_count computed from full uncapped set
    |       |
    |       +-- DB: batch_entry_meta_lookup(store, &lookup_ids)  [NEW call]
    |       |       chunked at 100 IDs per IN-clause (pattern #883)
    |       |       returns: HashMap<u64, EntryMeta>
    |       |
    |       +-- DB: batch_entry_meta_lookup(store, &ql_inj_ids)  [existing call]
    |       |
    |       +-- compute_knowledge_reuse(
    |               query_logs, injection_logs, active_cats, feature_cycle,
    |               entry_category_lookup, entry_meta_lookup,
    |               &explicit_read_ids,    [NEW]
    |               &explicit_meta_map     [NEW]
    |           )
    |               |
    |               +-- search_exposure_count (formerly delivery_count)
    |               +-- explicit_read_count = explicit_read_ids.len() as u64  [NEW]
    |               +-- explicit_read_by_category (tally from explicit_meta_map) [NEW]
    |               +-- total_served = |explicit_read_ids u injection_ids| [REDEFINED]
    |               +-- cross_session_count, by_category, cross_feature_reuse,
    |                   intra_cycle_reuse, top_cross_feature_entries [UNCHANGED]
    |               returns: FeatureKnowledgeReuse
    |
    +-- render_knowledge_reuse(&reuse, feature_cycle)
            guard: total_served == 0 && search_exposure_count == 0
            "Entries served to agents (reads + injections)": total_served
            "Search exposures (distinct)": search_exposure_count
            "Explicit reads (distinct)": explicit_read_count
            "Explicit read categories": explicit_read_by_category breakdown
```

---

## Shared Types (New / Modified Fields)

### `FeatureKnowledgeReuse` in `unimatrix-observe/src/types.rs`

Fields changed:
- `delivery_count: u64`  →  `search_exposure_count: u64`
  - Serde: stacked `#[serde(alias = "delivery_count")]` + `#[serde(alias = "tier1_reuse_count")]`
  - Canonical serialization key: `"search_exposure_count"`
- `explicit_read_count: u64`  — NEW, `#[serde(default)]`
- `explicit_read_by_category: HashMap<String, u64>`  — NEW, `#[serde(default)]`
- `total_served: u64`  — SEMANTIC CHANGE: now `|explicit_reads u injections|`

Fields unchanged: `cross_session_count`, `by_category`, `category_gaps`, `total_stored`,
`cross_feature_reuse`, `intra_cycle_reuse`, `top_cross_feature_entries`.

### `EntryMeta` in `unimatrix-server/src/mcp/knowledge_reuse.rs`

Unchanged. Used for both the existing query_log/injection_log meta lookup and the new
explicit read meta lookup. Fields: `title: String`, `feature_cycle: Option<String>`,
`category: String`.

### Constant `EXPLICIT_READ_META_CAP: usize = 500` in `tools.rs`

Placed near `compute_knowledge_reuse_for_sessions`. Limits the ID slice passed to
`batch_entry_meta_lookup` for explicit read category join. Does NOT limit `explicit_read_count`.

---

## Sequencing Constraints (Build Order)

Wave 1 (no dependencies on other changed files):
- `unimatrix-observe/src/types.rs` — `FeatureKnowledgeReuse` struct definition
- `unimatrix-store/src/cycle_review_index.rs` — constant bump only

Wave 2 (depends on Wave 1 type changes):
- `unimatrix-server/src/mcp/knowledge_reuse.rs` — new helper + extended signature

Wave 3 (depends on Wave 2):
- `unimatrix-server/src/mcp/tools.rs` — orchestration extension
- `unimatrix-server/src/mcp/response/retrospective.rs` — render changes

The implementation agent must compile after Wave 1 before proceeding to Wave 2,
and after Wave 2 before proceeding to Wave 3.

---

## Key Constraints (Summary)

1. `ObservationRecord.input` is `Value::String` from hook path — two-branch parse required (ADR-001 Correction)
2. Integer-form `{"id": 42}` and string-form `{"id": "42"}` both handled (AC-16 GATE)
3. Render guard: `total_served == 0 && search_exposure_count == 0` — NOT three conditions (AC-17 GATE)
4. Serde alias chain: `search_exposure_count` carries BOTH `"delivery_count"` AND `"tier1_reuse_count"` (AC-02 GATE)
5. `batch_entry_meta_lookup` capped at 500 IDs for category join; `explicit_read_count` uses full uncapped set (ADR-004)
6. `total_served = |explicit_read_ids u injection_ids|` — search exposures excluded (AC-14/AC-15 GATE)
7. `normalize_tool_name` mandatory before any tool name comparison (AC-06 GATE)

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found ADR #4218 (extract_explicit_read_ids helper),
  ADR #4216 (total_served redefinition), ADR #4215 (triple-alias serde chain). All directly
  used. Pattern #921 (col-020b compute/IO separation) confirms the design.
- Deviations from established patterns: none. Two-branch Value parse follows the same
  pattern as `extract_topic_signal` at `listener.rs:1911`.
