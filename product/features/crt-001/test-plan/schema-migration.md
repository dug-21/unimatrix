# Test Plan: C2 Schema Migration

## Risk Coverage: R-01 (Schema migration corruption)

### T-C2-01: Migration preserves entries (R-01 scenario 1)
- Create v1 database with 10 entries (various statuses)
- Open with crt-001 code (triggers migration)
- Verify all 10 entries readable with original field values preserved
- Verify helpful_count=0 and unhelpful_count=0 on all entries
- Verifies: AC-03

### T-C2-02: Migration preserves non-zero fields (R-01 scenario 2)
- Create v1 database with entries containing non-zero access_count, supersedes, correction_count, security fields
- Verify all are preserved through migration

### T-C2-03: Migration preserves Unicode content (R-01 scenario 3)
- Create v1 database with Unicode content in title/content
- Verify content and content_hash survive migration

### T-C2-04: Migration handles empty strings (R-01 scenario 4)
- Create v1 database with empty strings in all string fields
- Verify migration handles edge cases

### T-C2-05: v0->v1->v2 migration chain (R-01 scenario 5)
- Create v0 database (pre-nxs-004, 17-field entries)
- Open with crt-001 code
- Verify both migrations run sequentially
- Verify result has all security fields AND helpful/unhelpful=0

### T-C2-06: Idempotency (R-01 scenario 6, R-12)
- Open an already-migrated v2 database
- Verify migration is a no-op (schema_version already 2)

### T-C2-07: schema_version counter set to 2 (R-01 scenario 7)
- After migration, verify COUNTERS["schema_version"] = 2

### T-C2-08: Counters preserved (R-01 scenario 8)
- Create v1 database with known counter values
- Verify total_active, total_deprecated, total_proposed, next_entry_id preserved

## Test Infrastructure

### create_v1_database helper
- Similar to existing create_legacy_database
- Creates database with 24-field V1EntryRecord format
- Includes AGENT_REGISTRY and AUDIT_LOG tables (vnc-001)
- Sets schema_version=1 in COUNTERS
