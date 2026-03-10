# col-020b Pseudocode Overview

## Components

| ID | Component | File(s) Modified | Purpose |
|----|-----------|-----------------|---------|
| C1 | tool-name-normalizer | `session_metrics.rs` | Strip `mcp__unimatrix__` prefix before classification/counting |
| C2 | tool-classification | `session_metrics.rs` | Add `curate` category; call normalizer before match |
| C3 | knowledge-curated-counter | `session_metrics.rs` | Count curation tool calls as `knowledge_curated` |
| C4 | type-renames | `types.rs` | Rename fields/types with serde(alias) backward compat |
| C5 | knowledge-reuse-semantics | `knowledge_reuse.rs` | Revise: delivery_count = all entries; cross_session_count = 2+ sessions |
| C6 | data-flow-debugging | `tools.rs` | Add tracing::debug! at data flow boundaries |
| C7 | re-export-update | `lib.rs`, `tools.rs` | Update KnowledgeReuse -> FeatureKnowledgeReuse across imports |

## Build Order

1. **C4 (type-renames)** -- all other components depend on the renamed types
2. **C7 (re-export-update)** -- must follow C4 to compile
3. **C1 (tool-name-normalizer)** -- standalone helper, no dependencies beyond C4
4. **C2 (tool-classification)** -- depends on C1
5. **C3 (knowledge-curated-counter)** -- depends on C1 and C4 (new field)
6. **C5 (knowledge-reuse-semantics)** -- depends on C4 (renamed type)
7. **C6 (data-flow-debugging)** -- depends on C5 and C7 (return type)

Practical recommendation: implement C4 + C7 together (type renames + import updates), then C1 + C2 + C3 together (session_metrics changes), then C5 + C6 together (knowledge_reuse + tools changes).

## Data Flow

```
ObservationRecord[]
    |
    v
session_metrics.rs::build_session_summary
    |-- normalize_tool_name(tool) -> bare_name           [C1]
    |-- classify_tool(tool) -> category (uses C1)        [C2]
    |-- knowledge_served counter (uses C1)               [C3]
    |-- knowledge_stored counter (uses C1)               [C3]
    |-- knowledge_curated counter (uses C1)              [C3]
    v
SessionSummary { knowledge_served, knowledge_stored, knowledge_curated, ... }  [C4]
    |
    v
tools.rs::compute_knowledge_reuse_for_sessions           [C6]
    |-- Store::scan_query_log_by_sessions -> QueryLogRecord[]
    |-- Store::scan_injection_log_by_sessions -> InjectionLogRecord[]
    |-- Store::count_active_entries_by_category -> HashMap
    |-- tracing::debug! at each boundary                 [C6]
    v
knowledge_reuse.rs::compute_knowledge_reuse              [C5]
    |-- delivery_count: ALL unique entry IDs
    |-- cross_session_count: entries in 2+ sessions
    |-- by_category: from ALL delivered entries
    |-- category_gaps: categories with 0 delivery
    v
FeatureKnowledgeReuse                                    [C4]
    |
    v
RetrospectiveReport.feature_knowledge_reuse              [C4]
```

## Shared Types (changes from C4)

### SessionSummary -- field renames + addition
- `knowledge_in` -> `knowledge_served` with `#[serde(alias = "knowledge_in")]`
- `knowledge_out` -> `knowledge_stored` with `#[serde(alias = "knowledge_out")]`
- new: `knowledge_curated: u64` with `#[serde(default)]`

### KnowledgeReuse -> FeatureKnowledgeReuse -- type rename + field changes
- Type renamed from `KnowledgeReuse` to `FeatureKnowledgeReuse`
- `tier1_reuse_count` -> `delivery_count` with `#[serde(alias = "tier1_reuse_count")]`
- new: `cross_session_count: u64` with `#[serde(default)]`
- `by_category` and `category_gaps` unchanged in type, changed in semantics (C5)

### RetrospectiveReport -- field rename
- `knowledge_reuse` -> `feature_knowledge_reuse` with `#[serde(alias = "knowledge_reuse")]`
- Type changes from `Option<KnowledgeReuse>` to `Option<FeatureKnowledgeReuse>`
