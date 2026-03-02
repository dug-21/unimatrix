# Test Plan: compact-dispatch

## Component Scope

Changes to `crates/unimatrix-server/src/uds_listener.rs`: CompactPayload handler, format_compaction_payload, budget constants.

## Risk Coverage

| Risk | Test |
|------|------|
| R-02 (stale entries) | Quarantined excluded, deprecated included |
| R-03 (budget overflow) | Budget enforcement, multi-byte UTF-8 |
| R-04 (fallback empty) | Fallback path with/without entries |
| R-08 (latency) | Benchmark test |
| R-11 (fetch failures) | Entry fetch failure skipping |

## Unit Tests

### format_compaction_payload Tests

#### test_format_payload_decisions_first
```
Arrange: CompactionCategories with 2 decisions, 3 injections, 1 convention
Act: format_compaction_payload(...)
Assert: Decisions section appears before Key Context section, which appears before Conventions
```

#### test_format_payload_sorted_by_confidence
```
Arrange: decisions with confidence [0.6, 0.9, 0.3]
Act: format_compaction_payload(...)
Assert: 0.9 confidence entry appears first in Decisions section
```

#### test_format_payload_budget_enforcement
```
Arrange: entries totaling > 8000 bytes
Act: format_compaction_payload(..., max_bytes=8000)
Assert: output.len() <= 8000
```

#### test_format_payload_multibyte_utf8
```
Arrange: entry content with CJK characters at truncation boundary
Act: format_compaction_payload(..., max_bytes=500)
Assert: output is valid UTF-8, output.len() <= 500
```

#### test_format_payload_emoji_content
```
Arrange: entry content with 4-byte emoji characters
Act: format_compaction_payload(..., max_bytes=500)
Assert: output is valid UTF-8, no split emoji
```

#### test_format_payload_category_budget_rollover
```
Arrange: 0 decisions (unused budget rolls to injections), 10 injections
Act: format_compaction_payload(...)
Assert: injections section gets more than INJECTION_BUDGET_BYTES (rolls over from decisions)
```

#### test_format_payload_deprecated_entry_indicator
```
Arrange: entry with status == Deprecated
Act: format_compaction_payload(...)
Assert: output contains "[deprecated]" marker
```

#### test_format_payload_empty_categories
```
Arrange: all categories empty
Act: format_compaction_payload(...)
Assert: returns None
```

#### test_format_payload_header_present
```
Arrange: non-empty categories
Act: format_compaction_payload(...)
Assert: output starts with "--- Unimatrix Compaction Context ---"
```

#### test_format_payload_session_context_section
```
Arrange: role="developer", feature="col-008", compaction_count=2
Act: format_compaction_payload(...)
Assert: output contains "Role: developer", "Feature: col-008", "Compaction: #3"
```

#### test_format_payload_entry_metadata
```
Arrange: entry with id=42, title="ADR-001"
Act: format_compaction_payload(...)
Assert: output contains "<!-- id:42 -->"
```

#### test_format_payload_truncation_at_100_byte_threshold
```
Arrange: first entry fills category budget to within 50 bytes of cap
Act: format_compaction_payload(...)
Assert: second entry is omitted (remaining < 100 bytes)
```

### CompactPayload Dispatch Tests

#### test_dispatch_compact_payload_returns_briefing_content
```
Arrange: SessionRegistry with session "s1" (has injection history with known entry IDs), entry_store with matching entries
Act: dispatch_request(CompactPayload { session_id: "s1", ... })
Assert: response is BriefingContent with non-empty content
```

#### test_dispatch_compact_payload_unknown_session_fallback
```
Arrange: SessionRegistry (no sessions), entry_store with active decisions
Act: dispatch_request(CompactPayload { session_id: "unknown", ... })
Assert: response is BriefingContent (fallback path -- contains decisions from KB)
```

#### test_dispatch_compact_payload_empty_session_empty_kb
```
Arrange: SessionRegistry (no sessions), entry_store empty
Act: dispatch_request(CompactPayload { session_id: "s1", ... })
Assert: response is BriefingContent { content: "", token_count: 0 }
```

#### test_dispatch_compact_payload_increments_compaction_count
```
Arrange: SessionRegistry with session "s1"
Act: dispatch_request(CompactPayload { session_id: "s1", ... })
Assert: session_registry.get_state("s1").compaction_count == 1
```

#### test_dispatch_compact_payload_quarantined_excluded
```
Arrange: session with injection_history containing entry ID 5, entry 5 is Quarantined
Act: dispatch CompactPayload
Assert: BriefingContent does not contain entry 5's content
```

#### test_dispatch_compact_payload_token_limit_override
```
Arrange: session with entries, token_limit = Some(100) (= 400 bytes)
Act: dispatch CompactPayload
Assert: output.len() <= 400
```

### Fallback Path Tests

#### test_fallback_returns_decisions_and_conventions
```
Arrange: entry_store with 3 active decisions, 2 active conventions, no session
Act: fallback_path(None, entry_store)
Assert: decisions has 3 entries, conventions has 2 entries
```

#### test_fallback_filters_by_feature_tag
```
Arrange: entry_store with decisions tagged "col-008" and untagged decisions, feature="col-008"
Act: fallback_path(Some("col-008"), entry_store)
Assert: feature-tagged decisions appear first
```

#### test_fallback_excludes_non_active
```
Arrange: entry_store with Deprecated decisions and Active decisions
Act: fallback_path(None, entry_store)
Assert: only Active decisions in result
```

### Primary Path Tests

#### test_primary_deduplicates_by_highest_confidence
```
Arrange: injection_history with entry_id=1 at confidence 0.6 and entry_id=1 at confidence 0.9
Act: primary_path(session, entry_store)
Assert: entry 1 appears once with confidence 0.9
```

#### test_primary_skips_missing_entries
```
Arrange: injection_history with entry_id=999 (not in store)
Act: primary_path(session, entry_store)
Assert: entry 999 silently skipped, no error
```

## Benchmark Tests

#### bench_compact_payload_20_entries
```
Arrange: session with 20 injection records, all entries in store
Act: dispatch CompactPayload 10 times, measure p95
Assert: p95 < 15ms
```
