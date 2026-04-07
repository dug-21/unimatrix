# Agent Report: crt-049-agent-1-pseudocode

## Summary

Produced seven pseudocode files for crt-049 (OVERVIEW + six per-component files).
All files are in `product/features/crt-049/pseudocode/`.

## Files Produced

| File | Component |
|------|-----------|
| `OVERVIEW.md` | Data flow, shared types, wave ordering, key constraints |
| `feature-knowledge-reuse.md` | Component 1: FeatureKnowledgeReuse struct changes |
| `extract-explicit-read-ids.md` | Component 2: New pure extraction helper |
| `compute-knowledge-reuse.md` | Component 3: Extended computation function |
| `compute-knowledge-reuse-for-sessions.md` | Component 4: Orchestration + two batch lookups |
| `render-knowledge-reuse.md` | Component 5: Render guard + new labeled lines |
| `schema-version-bump.md` | Component 6: Constant bump 2 → 3 + advisory message |

## Components Covered

1. `FeatureKnowledgeReuse` — `unimatrix-observe/src/types.rs`
2. `extract_explicit_read_ids` — `unimatrix-server/src/mcp/knowledge_reuse.rs`
3. `compute_knowledge_reuse` — `unimatrix-server/src/mcp/knowledge_reuse.rs`
4. `compute_knowledge_reuse_for_sessions` — `unimatrix-server/src/mcp/tools.rs`
5. `render_knowledge_reuse` — `unimatrix-server/src/mcp/response/retrospective.rs`
6. `SUMMARY_SCHEMA_VERSION` — `unimatrix-store/src/cycle_review_index.rs`

## Open Questions

None. All open questions from SCOPE.md (OQ-01 through OQ-03) are resolved per the
architecture and ADRs. The advisory message path (Change 2 in schema-version-bump.md)
requires the implementation agent to grep for the exact location before editing —
the message may live in `tools.rs` or `cycle_review_index.rs`; the pseudocode notes this.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found ADR #4218 (extract_explicit_read_ids
  placement), ADR #4216 (total_served redefinition), ADR #4215 (triple-alias serde chain),
  pattern #921 (col-020b compute/IO separation). All incorporated.
- Also found: lesson #885 (serde-heavy types gate failures when alias tests omitted),
  pattern #4211 (normalize_tool_name bare-name tests mask production failure). Both
  inform the GATE designation on AC-02 and AC-06.
- Deviations from established patterns: none. Two-branch Value parse follows the same
  pattern as `extract_topic_signal` at `listener.rs:1911`. Batch IN-clause at 100-ID
  chunks follows col-026 ADR-003 / pattern #883.
