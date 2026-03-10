# Test Plan: C1 -- session_metrics

Module: `crates/unimatrix-observe/src/session_metrics.rs`

## Unit Tests

All tests are pure computation on `ObservationRecord` arrays. No Store, no async.

### compute_session_summaries

#### test_session_summaries_groups_by_session_id
- **Input**: 6 ObservationRecords: 3 with session_id "s1", 3 with session_id "s2"
- **Assert**: Returns 2 SessionSummary entries, one per session_id
- **Risks**: R-10 (data grouping)

#### test_session_summaries_empty_input
- **Input**: Empty `&[]`
- **Assert**: Returns empty Vec, no panic
- **Risks**: R-10

#### test_session_summaries_single_record
- **Input**: 1 ObservationRecord
- **Assert**: Returns 1 SessionSummary with duration_secs = 0
- **Risks**: R-10 (edge case: single observation)

#### test_session_summaries_ordered_by_started_at
- **Input**: Records for 3 sessions with started_at values 300, 100, 200
- **Assert**: Summaries returned in order [100, 200, 300]
- **Risks**: R-07
- **AC**: AC-16

#### test_session_summaries_tiebreak_by_session_id
- **Input**: Records for sessions "beta" and "alpha" both with identical earliest timestamp
- **Assert**: "alpha" before "beta" (lexicographic tiebreaker)
- **Risks**: R-07

#### test_session_summaries_tool_distribution_categories
- **Input**: Records with tools: Read, Edit, Bash, context_search, context_store, SubagentStart, UnknownTool (all PreToolUse)
- **Assert**: tool_distribution = {"read": 1, "write": 1, "execute": 1, "search": 1, "store": 1, "spawn": 1, "other": 1}
- **Risks**: R-06
- **AC**: AC-02

#### test_session_summaries_filters_pretooluse_only
- **Input**: Records with tool="Read": 2 PreToolUse + 1 PostToolUse
- **Assert**: tool_distribution["read"] == 2 (PostToolUse excluded per FR-01.2)
- **Risks**: Integration risk C1<->C6 (PreToolUse filtering)
- **AC**: AC-02

#### test_session_summaries_knowledge_in_out
- **Input**: 5 context_search + 2 context_lookup + 1 context_get + 3 context_store (all PreToolUse)
- **Assert**: knowledge_in = 8, knowledge_out = 3
- **AC**: AC-05

#### test_session_summaries_agents_spawned
- **Input**: 3 SubagentStart records with tool names "agent-a", "agent-b", "agent-c"
- **Assert**: agents_spawned = ["agent-a", "agent-b", "agent-c"]
- **AC**: AC-04

#### test_session_summaries_top_file_zones_max_5
- **Input**: Records with file paths spanning 7 distinct directory zones
- **Assert**: top_file_zones has exactly 5 entries, in descending frequency order
- **AC**: AC-03

#### test_session_summaries_started_at_and_duration
- **Input**: Session with records at timestamps 1000, 2000, 5000 (epoch millis)
- **Assert**: started_at = 1000, duration_secs = 4 (5000 - 1000 = 4000ms = 4s)
- **AC**: AC-01 (partially)

### extract_file_path (internal helper)

#### test_extract_file_path_read
- **Input**: tool="Read", input=`{"file_path": "/foo/bar.rs"}`
- **Assert**: Returns Some("/foo/bar.rs")
- **Risks**: R-06

#### test_extract_file_path_edit
- **Input**: tool="Edit", input=`{"file_path": "/foo/bar.rs", "old_string": "x"}`
- **Assert**: Returns Some("/foo/bar.rs")

#### test_extract_file_path_write
- **Input**: tool="Write", input=`{"file_path": "/foo/bar.rs", "content": "x"}`
- **Assert**: Returns Some("/foo/bar.rs")

#### test_extract_file_path_glob
- **Input**: tool="Glob", input=`{"path": "/foo"}`
- **Assert**: Returns Some("/foo")

#### test_extract_file_path_grep
- **Input**: tool="Grep", input=`{"path": "/foo", "pattern": "test"}`
- **Assert**: Returns Some("/foo")
- **Risks**: R-06 (Grep inclusion per ADR-004)

#### test_extract_file_path_unknown_tool
- **Input**: tool="NewTool", input=`{"file_path": "/foo"}`
- **Assert**: Returns None (silent skip)
- **Risks**: R-06

#### test_extract_file_path_missing_field
- **Input**: tool="Read", input=`{"other_field": "value"}`
- **Assert**: Returns None, no panic
- **Risks**: R-06

#### test_extract_file_path_non_string_value
- **Input**: tool="Read", input=`{"file_path": 42}`
- **Assert**: Returns None, no panic

### classify_tool (internal helper)

#### test_classify_tool_all_categories
- **Assert**: Read->"read", Edit->"write", Bash->"execute", context_search->"search", context_store->"store", SubagentStart->"spawn", anything_else->"other"

### extract_directory_zone (internal helper)

#### test_extract_directory_zone_absolute_path
- **Input**: "/workspaces/unimatrix/crates/store/src/lib.rs"
- **Assert**: Returns "crates/store/src" (first 3 components from workspace root)
- **Risks**: R-15

#### test_extract_directory_zone_relative_path
- **Input**: "crates/store/src/lib.rs"
- **Assert**: Returns "crates/store/src"
- **Risks**: R-15

#### test_extract_directory_zone_short_path
- **Input**: "src/lib.rs"
- **Assert**: Returns "src" (fewer than 3 components available)

#### test_extract_directory_zone_trailing_slash
- **Input**: "/workspaces/unimatrix/crates/store/src/"
- **Assert**: Same zone as files in that directory
- **Risks**: R-15

### compute_context_reload_pct

#### test_reload_pct_basic
- **Input**: Session 1 reads files A, B, C. Session 2 reads B, C, D.
- **Assert**: Returns 2/3 (~0.667) -- B and C are reloaded out of 3 files in session 2
- **AC**: AC-10

#### test_reload_pct_single_session
- **Input**: 1 session
- **Assert**: Returns 0.0
- **Risks**: R-13
- **AC**: AC-13

#### test_reload_pct_no_files_in_later_sessions
- **Input**: Session 1 reads files A, B. Session 2 has no file reads.
- **Assert**: Returns 0.0, not NaN
- **Risks**: R-13

#### test_reload_pct_full_overlap
- **Input**: Session 1 reads A, B. Session 2 reads A, B.
- **Assert**: Returns 1.0 exactly
- **Risks**: Edge case from Risk Strategy

#### test_reload_pct_no_overlap
- **Input**: Session 1 reads A, B. Session 2 reads C, D.
- **Assert**: Returns 0.0

#### test_reload_pct_range
- **Assert**: Return value is always in [0.0, 1.0]
- **AC**: AC-13
