# Test Plan: C3 -- knowledge_reuse

Location: Inline in `crates/unimatrix-server/src/mcp/tools.rs` (context_retrospective handler)

## Integration Tests

Knowledge reuse is computed server-side per ADR-001. Tests require Store setup with seeded data across multiple tables (entries, query_log, injection_log). These are Rust integration tests using real Store instances.

### Core reuse computation

#### test_knowledge_reuse_cross_session_query_log
- **Setup**: Store with entry E1 (category="convention") stored in session "s1". query_log row for session "s2" with result_entry_ids="[E1_id]".
- **Assert**: tier1_reuse_count = 1, by_category = {"convention": 1}
- **Risks**: R-04
- **AC**: AC-06

#### test_knowledge_reuse_cross_session_injection_log
- **Setup**: Entry E1 stored in session "s1". injection_log row for session "s2" referencing E1.
- **Assert**: tier1_reuse_count = 1
- **Risks**: R-04
- **AC**: AC-06

#### test_knowledge_reuse_same_session_excluded
- **Setup**: Entry E1 stored in session "s1". query_log for session "s1" with result_entry_ids="[E1_id]".
- **Assert**: tier1_reuse_count = 0 (same-session retrieval is NOT cross-session reuse)
- **Risks**: R-04 (edge case from Risk Strategy)
- **AC**: AC-06

#### test_knowledge_reuse_deduplication_across_sources
- **Setup**: Entry E1 stored in session "s1". Both query_log AND injection_log for session "s2" reference E1.
- **Assert**: tier1_reuse_count = 1 (not 2)
- **Risks**: R-12
- **AC**: AC-06

#### test_knowledge_reuse_deduplication_across_sessions
- **Setup**: Entry E1 stored in session "s1". query_log for session "s2" references E1. injection_log for session "s3" references E1.
- **Assert**: tier1_reuse_count = 1 (distinct entries, not retrieval events)
- **Risks**: R-12

### by_category breakdown

#### test_knowledge_reuse_by_category
- **Setup**: 2 convention entries and 1 pattern entry reused cross-session.
- **Assert**: by_category = {"convention": 2, "pattern": 1}
- **AC**: AC-07

### category_gaps

#### test_knowledge_reuse_category_gaps
- **Setup**: Active entries in categories "convention", "pattern", "procedure". Only "convention" reused cross-session.
- **Assert**: category_gaps contains "pattern" and "procedure"
- **AC**: AC-08

#### test_knowledge_reuse_no_gaps_all_reused
- **Setup**: Active entries in 2 categories, both reused.
- **Assert**: category_gaps is empty

### JSON parsing robustness

#### test_knowledge_reuse_malformed_result_entry_ids
- **Setup**: query_log row with result_entry_ids = "not json"
- **Assert**: Row contributes 0 entries to reuse, no panic, computation completes
- **Risks**: R-01

#### test_knowledge_reuse_empty_result_entry_ids
- **Setup**: query_log row with result_entry_ids = ""
- **Assert**: Row contributes 0 entries
- **Risks**: R-01

#### test_knowledge_reuse_null_result_entry_ids
- **Setup**: query_log row with result_entry_ids = "null"
- **Assert**: Row contributes 0 entries
- **Risks**: R-01

#### test_knowledge_reuse_duplicate_ids_in_result
- **Setup**: query_log row with result_entry_ids = "[1,1,1,2]"
- **Assert**: Deduplicated to {1, 2}
- **Risks**: R-01 (edge case from Risk Strategy)

### Data gap handling

#### test_knowledge_reuse_no_query_log_data
- **Setup**: Topic with sessions but empty query_log.
- **Assert**: tier1_reuse_count computed from injection_log only (or 0 if also empty). No error.
- **Risks**: R-02

#### test_knowledge_reuse_no_injection_log_data
- **Setup**: Topic with sessions, query_log has data, injection_log empty.
- **Assert**: tier1_reuse_count computed from query_log only.
- **Risks**: R-02

#### test_knowledge_reuse_both_sources_empty
- **Setup**: Topic with sessions but no query_log and no injection_log.
- **Assert**: tier1_reuse_count = 0, by_category empty, category_gaps lists all active categories.
- **Risks**: R-02, R-10

### Empty input

#### test_knowledge_reuse_zero_sessions
- **Setup**: No sessions for the topic.
- **Assert**: KnowledgeReuse not computed (None on report). No error.
- **Risks**: R-10
