# Test Plan Overview: crt-001 Usage Tracking

## Risk-to-Test Mapping

| Risk | Scenarios | Component Test Plan |
|------|-----------|-------------------|
| R-01 (Schema migration corruption) | 8 | schema-migration.md |
| R-02 (Counter update atomicity) | 6 | store-usage.md |
| R-03 (Dedup bypass) | 8 | usage-dedup.md |
| R-04 (FEATURE_ENTRIES orphan writes) | 5 | store-usage.md |
| R-05 (Trait object safety) | 3 | trait-extension.md |
| R-06 (bincode positional encoding) | 4 | schema-extension.md |
| R-07 (last_accessed_at staleness) | 4 | store-usage.md, server-integration.md |
| R-08 (write_count_since correctness) | 6 | audit-query.md |
| R-09 (Fire-and-forget masking) | 3 | server-integration.md |
| R-10 (briefing double-counting) | 3 | server-integration.md |
| R-11 (Backward compatibility) | 5 | server-integration.md |
| R-12 (Migration idempotency) | Covered by R-01 scenario 6 | schema-migration.md |
| R-14 (record_usage partial batch) | 3 | store-usage.md |
| R-16 (Vote correction atomicity) | 5 | usage-dedup.md, store-usage.md |
| R-17 (FEATURE_ENTRIES trust bypass) | 4 | server-integration.md |

## Test Strategy

1. **Unit tests** in each modified/new module (schema, migration, write, traits, adapters, async_wrappers, usage_dedup, audit)
2. **Integration tests** that exercise the full recording flow through the server layer
3. **Regression tests** ensuring existing behavior unchanged (R-11)
4. **Compile-time checks** for trait object safety (R-05)

## Test Infrastructure

Build on existing patterns:
- `TestDb` helper from unimatrix-store test_helpers
- `make_server()` helper from server::tests
- `make_event()` helper from audit::tests
- Standard tempfile::TempDir for database isolation

## Coverage Targets

- 66 scenarios from RISK-TEST-STRATEGY.md
- All 18 acceptance criteria verified
- All 7 edge cases (EC-01 through EC-07) covered
