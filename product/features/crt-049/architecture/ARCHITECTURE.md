# Architecture: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

## System Overview

`context_cycle_review` assembles a `RetrospectiveReport` from multiple sub-pipelines.
The knowledge reuse sub-pipeline (steps 13–14) currently measures knowledge consumption
using `query_log` (search exposures) and `injection_log` (hook-injected entries). Both
are weak proxies: appearing in results or being injected does not confirm agent consumption.

crt-049 introduces a third signal — explicit reads — sourced from the `observations` table
already loaded at step 12. `context_get` and single-ID `context_lookup` calls are
unambiguous consumption signals. This feature threads the in-memory `attributed` slice
into `compute_knowledge_reuse_for_sessions`, extracts explicit read IDs from it, and
enriches `FeatureKnowledgeReuse` with two new fields (`explicit_read_count`,
`explicit_read_by_category`) while redefining `total_served` to exclude search exposures.

No new tables, migrations, or crate dependencies are required.

---

## Component Breakdown

### 1. `unimatrix-observe` — `types.rs` / `FeatureKnowledgeReuse`

**Responsibility**: Define the shared report type that carries knowledge reuse metrics.

Changes:
- Rename field `delivery_count` → `search_exposure_count` with serde aliases
  `"delivery_count"` and `"tier1_reuse_count"` (both must be present for backward compat)
- Add `explicit_read_count: u64` with `#[serde(default)]`
- Add `explicit_read_by_category: HashMap<String, u64>` with `#[serde(default)]`
- Redefine semantic role of `total_served`: now `|explicit_reads ∪ injections|`
  (computation happens in `knowledge_reuse.rs`; the field is populated there)

### 2. `unimatrix-server` — `mcp/knowledge_reuse.rs`

**Responsibility**: Pure computation of knowledge reuse metrics from pre-loaded slices.

Changes:
- Add `fn extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>`
  — filters PreToolUse events for `context_get`/`context_lookup`, extracts `input["id"]`
- Extend `compute_knowledge_reuse` signature to accept `explicit_read_ids: &HashSet<u64>`
  and `explicit_read_meta: &HashMap<u64, EntryMeta>` (pre-fetched by caller in `tools.rs`)
- Compute `explicit_read_count = explicit_read_ids.len() as u64`
- Compute `explicit_read_by_category` from `explicit_read_meta` (category tally)
- Recompute `total_served = |(explicit_read_ids ∪ injection_entry_ids)|` (deduplicated)
- Update early-return guards: zero check is now
  `total_served == 0 && search_exposure_count == 0`

### 3. `unimatrix-server` — `mcp/tools.rs` / `compute_knowledge_reuse_for_sessions`

**Responsibility**: Orchestrate data loading and delegate to `knowledge_reuse.rs`.

Changes:
- Add `attributed: &[ObservationRecord]` parameter to function signature
- Call `crate::mcp::knowledge_reuse::extract_explicit_read_ids(attributed)` to get the
  explicit read ID set (in-memory, no DB call)
- Call `batch_entry_meta_lookup(store, explicit_ids_vec)` for explicit read IDs — the
  existing `batch_entry_meta_lookup` function in `tools.rs` is already available in scope
  and handles chunking at 100 IDs per query (pattern #883, ADR-003)
- Pass the resulting `HashSet<u64>` and `HashMap<u64, EntryMeta>` into `compute_knowledge_reuse`
- Update the call site at step 13–14 in `context_cycle_review` to pass `&attributed`

### 4. `unimatrix-server` — `mcp/response/retrospective.rs` / `render_knowledge_reuse`

**Responsibility**: Render `FeatureKnowledgeReuse` as markdown for the cycle review report.

Changes:
- Zero-delivery guard: `reuse.total_served == 0 && reuse.search_exposure_count == 0`
  (injection-only cycles have `total_served > 0` and must not be suppressed)
- Summary line: replace "Distinct entries served" with "Entries served to agents
  (reads + injections)" backed by `reuse.total_served`
- Add "Search exposures (distinct)" line using `reuse.search_exposure_count`
- Add "Explicit reads (distinct)" line using `reuse.explicit_read_count`
- Add "Explicit read categories" breakdown from `reuse.explicit_read_by_category`
  (format matches existing `by_category` rendering)

### 5. `unimatrix-store` — `cycle_review_index.rs`

**Responsibility**: Define `SUMMARY_SCHEMA_VERSION` and `CycleReviewRecord`.

Changes:
- Bump `SUMMARY_SCHEMA_VERSION` from `2` to `3`
- Update the forced-value assertion test (`CRS-V24-U-01`) from `2` to `3`

---

## Component Interactions

```
context_cycle_review handler (tools.rs)
    │
    ├─ Step 12: attributed = load_attributed_observations() → Vec<ObservationRecord>
    │
    └─ Step 13-14: compute_knowledge_reuse_for_sessions(&store, &sessions, &feature_cycle, &attributed)
                        │
                        ├─ scan_query_log_by_sessions()          [DB: query_log]
                        ├─ scan_injection_log_by_sessions()      [DB: injection_log]
                        ├─ count_active_entries_by_category()    [DB: entries]
                        │
                        ├─ extract_explicit_read_ids(&attributed)   [in-memory, no DB]
                        │       filters PreToolUse + context_get/context_lookup
                        │       parses input as Value::String (hook path) or Value::Object (direct MCP)
                        │       extracts id as u64 or parseable string; returns HashSet<u64>
                        │
                        ├─ batch_entry_meta_lookup(&store, query_log_ids ∪ injection_ids)  [existing]
                        ├─ batch_entry_meta_lookup(&store, explicit_read_ids)              [new call]
                        │       chunked at 100 IDs, returns HashMap<u64, EntryMeta>
                        │
                        └─ compute_knowledge_reuse(
                               query_logs, injection_logs,
                               active_cats, feature_cycle,
                               entry_category_lookup,      [closure over query/inj meta_map]
                               entry_meta_lookup,          [closure over query/inj meta_map]
                               explicit_read_ids,          [new param]
                               explicit_read_meta,         [new param]
                           ) → FeatureKnowledgeReuse
                                │
                                ├─ search_exposure_count  (formerly delivery_count)
                                ├─ explicit_read_count    (new)
                                ├─ explicit_read_by_category (new)
                                ├─ total_served = |explicit_reads ∪ injections|  (redefined)
                                └─ ... (all existing fields unchanged)
```

---

## Technology Decisions

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | `extract_explicit_read_ids` as standalone helper in `knowledge_reuse.rs` | #4214 |
| ADR-002 | Triple-alias serde chain on `search_exposure_count` | #4215 |
| ADR-003 | `total_served` redefinition as explicit reads ∪ injections | #4216 |
| ADR-004 | Cardinality cap (500) for explicit read `batch_entry_meta_lookup` | #4217 |

---

## Integration Points

### `batch_entry_meta_lookup` availability (SR-01 from SCOPE-RISK-ASSESSMENT)

`batch_entry_meta_lookup` is defined at line 3143 of `tools.rs` as a private `async fn`
in the same module as `compute_knowledge_reuse_for_sessions` (line 3198). It is directly
callable from `compute_knowledge_reuse_for_sessions`. No visibility change is needed.

The call pattern established by col-026: pre-fetch all IDs from query_log + injection_log
before calling `compute_knowledge_reuse`. The explicit read IDs require a second call
to `batch_entry_meta_lookup` within `compute_knowledge_reuse_for_sessions`:

```rust
// Existing call (col-026)
let meta_map_owned = batch_entry_meta_lookup(store, &ids_vec).await;

// New call (crt-049)
let explicit_ids: HashSet<u64> =
    crate::mcp::knowledge_reuse::extract_explicit_read_ids(attributed);
let explicit_ids_vec: Vec<u64> = explicit_ids.iter().copied().collect();
let explicit_meta_map = batch_entry_meta_lookup(store, &explicit_ids_vec).await;
```

### Callers of `compute_knowledge_reuse_for_sessions` (SR-01 blast radius)

Grep result (tools.rs line 1949): **exactly one call site** in the `context_cycle_review`
handler at step 13–14. There are no other callers outside of tests. The signature change
adds one parameter (`attributed: &[ObservationRecord]`) at the end. The call site at
line 1949 already has `attributed` in scope (it is used at step 12, line 1945).

The single test caller (`test_compute_knowledge_reuse_for_sessions_no_block_on_panic`,
line 4753) passes an empty slice (`&[]`) — it must be updated to pass `&[]` for the new
`attributed` parameter as well.

### Callers of `render_knowledge_reuse` (blast radius)

Grep result (retrospective.rs line 128): **exactly one call site** in
`render_retrospective_report`. All other appearances in the file are test fixtures that
construct `FeatureKnowledgeReuse` directly. These test fixtures use `delivery_count` as
the field name — they must be updated to use `search_exposure_count` after the rename.

### `total_served` consumers

All uses of `total_served` found in the codebase:
- `types.rs` lines 296, 693, 831, 1382, 1425 — field definition and test fixtures
- `retrospective.rs` lines 1525, 1604, 2197, 2219, 3374 — test fixtures (set to `0`)
- `retrospective.rs` line 1036 — read in `render_knowledge_reuse` (not currently displayed;
  render uses `delivery_count` for the summary line)

No external JSON consumer reads `total_served` from a stored `cycle_review_index` row
through any path other than `render_knowledge_reuse`. The field is currently set to `0`
in all production test fixtures, meaning its existing value is already semantically
unused. The redefinition to `explicit_read_count + injection_count` is a safe change.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `extract_explicit_read_ids` | `fn(&[ObservationRecord]) -> HashSet<u64>` | `knowledge_reuse.rs` (new) |
| `compute_knowledge_reuse` | adds `explicit_read_ids: &HashSet<u64>`, `explicit_read_meta: &HashMap<u64, EntryMeta>` params | `knowledge_reuse.rs` (extended) |
| `compute_knowledge_reuse_for_sessions` | adds `attributed: &[ObservationRecord]` param | `tools.rs` (extended) |
| `FeatureKnowledgeReuse::search_exposure_count` | `u64`, aliases: `"delivery_count"`, `"tier1_reuse_count"` | `types.rs` (renamed) |
| `FeatureKnowledgeReuse::explicit_read_count` | `u64`, `#[serde(default)]` | `types.rs` (new) |
| `FeatureKnowledgeReuse::explicit_read_by_category` | `HashMap<String, u64>`, `#[serde(default)]` | `types.rs` (new) |
| `FeatureKnowledgeReuse::total_served` | `u64` — redefined as `\|explicit_reads ∪ injections\|` | `types.rs` (semantic change) |
| `SUMMARY_SCHEMA_VERSION` | `u32 = 3` (was `2`) | `cycle_review_index.rs` (bumped) |
| `batch_entry_meta_lookup` | existing `async fn(&Arc<SqlxStore>, &[u64]) -> HashMap<u64, EntryMeta>` | `tools.rs` line 3143 |
| `normalize_tool_name` | `fn(&str) -> &str` — strips `mcp__unimatrix__` prefix | `unimatrix_observe::session_metrics` |
| `ObservationRecord` | fields: `event_type: String`, `tool: Option<String>`, `input: Option<serde_json::Value>`, `session_id: String` | `unimatrix-core` |

---

## Data Flow: Explicit Read Extraction

```
attributed: Vec<ObservationRecord>
    │
    ▼  filter: event_type == "PreToolUse"
    │          normalize_tool_name(tool) ∈ {"context_get", "context_lookup"}
    │
    ▼  parse input (two-branch — hook listener delivers Value::String, direct MCP Value::Object):
    │      Some(Value::Object(_)) → clone as-is
    │      Some(Value::String(s)) → serde_json::from_str(s).ok()
    │      _ → None
    │
    ▼  extract ID (integer or string form, matching GetParams deserializer):
    │      obj["id"].as_u64()
    │        .or_else(|| obj["id"].as_str().and_then(|s| s.parse().ok()))
    │
    ▼  deduplicate: HashSet<u64>   ← explicit_read_ids
    │
    ▼  batch_entry_meta_lookup(store, &explicit_ids_vec)
    │       chunked at 100, returns HashMap<u64, EntryMeta>
    │
    ▼  tally: for id in explicit_read_ids, lookup EntryMeta.category
    │
    ▼  explicit_read_by_category: HashMap<String, u64>
    │  explicit_read_count: explicit_read_ids.len() as u64
    │
    ▼  total_served: (explicit_read_ids ∪ injection_entry_ids).len() as u64
```

---

## Open Questions

None. All scope open questions (OQ-01, OQ-02, OQ-03) are resolved in SCOPE.md.
All risk items (SR-01 through SR-07) are addressed in ADRs or explicitly documented above.
