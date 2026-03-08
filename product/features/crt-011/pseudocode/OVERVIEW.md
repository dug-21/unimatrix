# crt-011: Pseudocode Overview — Confidence Signal Integrity

## Components

| Component | Scope | Files Modified |
|-----------|-------|----------------|
| consumer-dedup | Bug fixes in run_confidence_consumer and run_retrospective_consumer | crates/unimatrix-server/src/uds/listener.rs |
| integration-tests | Handler-level integration tests for confidence path | crates/unimatrix-server/src/uds/listener.rs (unit tests), crates/unimatrix-server/src/services/usage.rs, crates/unimatrix-server/src/server.rs |

## Data Flow (After Fix)

```
Session Close
  -> write_signals_to_queue (1 SignalRecord per session)
  -> SIGNAL_QUEUE (SQLite)
  -> drain_signals (batch of SignalRecords)
  -> run_confidence_consumer:
      Step 2: HashSet<u64> for helpful_count dedup (existing, correct)
      Step 3: record_usage_with_confidence (existing, correct)
      Step 4: HashSet<(String, u64)> for success_session_count dedup (NEW)
        - Pass 1 (under lock): check/insert HashSet before incrementing
        - Fetch (outside lock): fetch metadata for unknown entry_ids
        - Pass 3 (under lock): check HashSet again before incrementing
  -> run_retrospective_consumer:
      HashSet<(String, u64)> for rework_session_count dedup (NEW)
      rework_flag_count: no dedup (intentional, ADR-002)
  -> PendingEntriesAnalysis
```

## Shared Types

No new types introduced. The fix uses `HashSet<(String, u64)>` as a local variable in each consumer function.

## Component Interaction

The two components are independent:
- **consumer-dedup** modifies the consumer functions (production code)
- **integration-tests** adds tests that exercise the existing handler-to-service-to-store chain and the fixed consumer functions

Both components share the same test helpers already present in listener.rs, usage.rs, and server.rs test modules.

## Integration Harness

No integration test infrastructure from product/test/infra-001/ applies to this feature. All tests are Rust unit/integration tests within the unimatrix-server crate using existing helpers (make_store, make_server, make_usage_service, insert_test_entry).

## Patterns Used

- **HashSet dedup** pattern: already used in run_confidence_consumer Step 2 for helpful_count (HashSet<u64>). Extending to (String, u64) tuples for session-aware dedup.
- **Test helper reuse**: extending existing make_store(), make_server(), make_usage_service() helpers.
