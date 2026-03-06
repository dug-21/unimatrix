# C4: UDS Path Hardening — Test Plan

## Location
`crates/unimatrix-server/src/uds/listener.rs` (integration tests)

## Tests

### T-UDS-01: handle_context_search uses Strict mode (FR-4.1, AC-01)
- Verify ServiceSearchParams constructed with `retrieval_mode: RetrievalMode::Strict`
- Can be tested via SearchService mock or by observing that deprecated entries are excluded

### T-UDS-02: UDS search returns zero deprecated entries (AC-01)
- Insert: Active + Deprecated entries
- Call handle_context_search
- Expected: HookResponse::Entries contains only Active entries

### T-UDS-03: UDS empty results — no fallback (AC-10)
- Insert: only Deprecated entries for query topic
- Call handle_context_search
- Expected: `HookResponse::Entries { items: vec![], total_tokens: 0 }`

### T-UDS-04: BriefingService injection history excludes deprecated (AC-11, R-10)
- Setup: injection history with mix of Active and Deprecated entry IDs
- BriefingService assembles briefing
- Expected: Deprecated entries absent from briefing payload

### T-UDS-05: BriefingService handles empty injection history after filtering (R-10)
- Setup: injection history with all Deprecated entries
- Expected: empty injection sections, no error

## Risk Coverage

| Risk | Scenarios | Tests |
|------|-----------|-------|
| R-03 (strict empty) | UDS returns empty | T-UDS-03 |
| R-10 (briefing over-filtering) | All deprecated history | T-UDS-04, T-UDS-05 |
| AC-01 | Zero deprecated in UDS | T-UDS-02 |
| AC-10 | Empty set, no fallback | T-UDS-03 |
| AC-11 | Briefing excludes deprecated | T-UDS-04 |
