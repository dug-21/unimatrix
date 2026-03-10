# Test Plan: C3 — Knowledge Curated Counter

**File:** `crates/unimatrix-observe/src/session_metrics.rs`
**Function:** `build_session_summary` (private, called by `compute_session_summaries`)
**Risks:** R-08 (MCP-prefix gap), R-10 (inconsistent normalization), R-11 (curate key)

## Unit Test Expectations

All tests in `session_metrics.rs::tests`.

### test_session_summaries_knowledge_in_out (UPDATE EXISTING -> RENAME)

Rename to `test_session_summaries_knowledge_served_stored` and update field assertions.

```
Arrange: records with bare context_search (x5), context_lookup (x2), context_get (x1),
         context_store (x3)
Act:     summaries = compute_session_summaries(&records)
Assert:  summaries[0].knowledge_served == 8
         summaries[0].knowledge_stored == 3
```

### test_session_summaries_mcp_prefixed_knowledge_flow (NEW)

MCP-prefixed tool names produce non-zero counters for all three knowledge metrics.

```
Arrange: records with:
  "mcp__unimatrix__context_search" (x2)
  "mcp__unimatrix__context_lookup" (x1)
  "mcp__unimatrix__context_get" (x1)
  "mcp__unimatrix__context_store" (x2)
  "mcp__unimatrix__context_correct" (x1)
  "mcp__unimatrix__context_deprecate" (x1)
  "mcp__unimatrix__context_quarantine" (x1)
Act:     summaries = compute_session_summaries(&records)
Assert:  summaries[0].knowledge_served == 4
         summaries[0].knowledge_stored == 2
         summaries[0].knowledge_curated == 3
```

### test_session_summaries_mixed_bare_and_prefixed (NEW)

Both bare and MCP-prefixed forms contribute to the same counter.

```
Arrange: records with:
  "context_search" (x1)
  "mcp__unimatrix__context_search" (x1)
  "context_store" (x1)
  "mcp__unimatrix__context_store" (x1)
  "context_correct" (x1)
  "mcp__unimatrix__context_correct" (x1)
Act:     summaries = compute_session_summaries(&records)
Assert:  summaries[0].knowledge_served == 2
         summaries[0].knowledge_stored == 2
         summaries[0].knowledge_curated == 2
```

### test_session_summaries_curate_in_tool_distribution (NEW)

The `tool_distribution` HashMap contains a "curate" key when curation tools are used.

```
Arrange: records with "mcp__unimatrix__context_correct" (x1),
         "mcp__unimatrix__context_deprecate" (x1)
Act:     summaries = compute_session_summaries(&records)
Assert:  summaries[0].tool_distribution.get("curate") == Some(&2)
```

### test_session_summaries_no_curate_without_curation_tools (NEW)

When no curation tools are used, "curate" key is absent from tool_distribution.

```
Arrange: records with "Read" (x1), "context_search" (x1)
Act:     summaries = compute_session_summaries(&records)
Assert:  summaries[0].tool_distribution.get("curate") == None
         summaries[0].knowledge_curated == 0
```

## Risk Coverage

- R-08: `test_session_summaries_mcp_prefixed_knowledge_flow` proves MCP-prefixed tools produce non-zero counters (the original #192 bug scenario).
- R-10: `test_session_summaries_mixed_bare_and_prefixed` proves all three counters apply normalization consistently.
- R-11: `test_session_summaries_curate_in_tool_distribution` and `test_session_summaries_no_curate_without_curation_tools` verify curate key presence/absence.
