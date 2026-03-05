# Test Plan: migration (Wave 0)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-01 (Migration Data Fidelity) | RT-01 to RT-10 |
| RISK-07 (Enum-to-Integer Mapping) | RT-44, RT-45 |
| RISK-16 (Migration Txn Size) | RT-69, RT-70 |
| RISK-20 (Schema Version) | RT-74 |

## Integration Tests

### IT-mig-01: Round-trip every table (RT-01)
```
Setup: Create v5 database with known data in all 7 tables:
  - entries (3 entries with diverse fields)
  - co_access (2 pairs)
  - sessions (2 sessions with different statuses)
  - injection_log (3 records across 2 sessions)
  - signal_queue (2 signals with entry_ids)
  - agent_registry (2 agents with capabilities)
  - audit_log (3 events with target_ids)
Action: Run migrate_v5_to_v6
Assert: Read back every record from v6 tables, assert field-by-field equality
```

### IT-mig-02: Historical schema entries migrate (RT-02)
```
Setup: Create v5 database with entries that have fewer fields (simulating v0/v1/v2 era entries that went through previous migrations)
Action: Run migration
Assert: Default fields populated correctly (helpful_count=0, confidence=0.5, etc.)
```

### IT-mig-03: All 24 fields non-default survive (RT-03)
```
Setup: Create v5 entry with EVERY field set to a distinct non-default value:
  - id=42, title="test", content="body", topic="t1", category="c1"
  - source="s1", status=Active, confidence=0.85, created_at=1000, updated_at=2000
  - last_accessed_at=3000, access_count=7, supersedes=Some(10), superseded_by=Some(50)
  - correction_count=2, embedding_dim=Some(384), created_by="agent1", modified_by="agent2"
  - content_hash="hash123", previous_hash=Some("prev"), version=3
  - feature_cycle="nxs-001", trust_source="system"
  - helpful_count=5, unhelpful_count=2
  - tags=["tag1", "tag2", "tag3"]
Action: Migrate, read back
Assert: Every single field matches
```

### IT-mig-04: Option fields NULL mapping (RT-04)
```
Setup: Entry with supersedes=Some(10), superseded_by=None
Action: Migrate
Assert: supersedes column = 10, superseded_by column = NULL
```

### IT-mig-05: Empty database migrates cleanly (RT-05)
```
Setup: Create v5 database with 0 rows in all tables
Action: Run migrate_v5_to_v6
Assert: No errors, schema_version = 6, all v6 tables exist with correct DDL
```

### IT-mig-06: 200-entry database (RT-06)
```
Setup: Create v5 database with 200 entries, 50 co_access pairs, 20 sessions, 100 injection logs, 15 signals, 5 agents, 30 audit events
Action: Migrate
Assert: Row counts match, spot-check 10 random entries for field equality
```

### IT-mig-07: Backup file exists (RT-07)
```
Setup: Create v5 database at known path
Action: Run migration
Assert: File {db_path}.v5-backup exists and is non-empty
```

### IT-mig-08: Migration atomicity (RT-08)
```
Setup: Create v5 database
Action: Verify migration runs within BEGIN IMMEDIATE...COMMIT
Assert: If any step fails, no _v6 tables remain, original tables intact
(Implementation: verify via transaction wrapper in migrate_v5_to_v6)
```

### IT-mig-09: Tags extracted to entry_tags (RT-10)
```
Setup: v5 entry with tags=["alpha", "beta", "gamma"]
Action: Migrate
Assert:
  - entry_tags table has 3 rows for this entry_id
  - SELECT tag FROM entry_tags WHERE entry_id=? returns ["alpha", "beta", "gamma"]
```

### IT-mig-10: Session enum mapping (RT-44)
```
Setup: v5 sessions with each SessionLifecycleStatus value (Active=0, Completed=1, TimedOut=2, Abandoned=3)
Action: Migrate
Assert: status column contains correct integer for each
```

### IT-mig-11: Signal enum mapping (RT-45)
```
Setup: v5 signals with each SignalType and SignalSource combination
Action: Migrate
Assert: signal_type and signal_source columns contain correct integers
```

### IT-mig-12: Schema version updated (RT-74)
```
Setup: v5 database
Action: Migrate
Assert: SELECT value FROM counters WHERE name='schema_version' returns "6"
```

### IT-mig-13: 500-row performance (RT-69)
```
Setup: v5 database with 500 entries + proportional operational data
Action: Time the migration
Assert: Completes within 5 seconds
```

### IT-mig-14: Empty database no-error (RT-70)
```
Same as IT-mig-05 but verify no panics, no warnings in stderr
```

### IT-mig-15: migrate_if_needed receives db_path
```
Setup: Verify function signature includes db_path parameter
Assert: Backup creation uses provided path
```

## Synthetic v5 Database Helper

Tests need a helper function to create v5 databases with known data:
```rust
fn create_synthetic_v5_db(path: &Path) -> Connection {
    // Create tables with v5 DDL (id INTEGER PK, data BLOB pattern)
    // Insert known data using bincode serialization
    // Set schema_version = 5
}
```

This helper lives in a test utility module shared across migration tests.
