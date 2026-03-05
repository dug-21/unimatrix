# Test Plan: compat-removal (Wave 4)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-05 (Compat Layer Removal) | RT-35, RT-36, RT-37 |

## Build Verification

### BV-compat-01: Workspace builds after deletions (RT-35)
```
Action: Delete handles.rs, dispatch.rs, tables.rs
Action: Remove mod declarations from lib.rs
Action: Remove SqliteReadTransaction, begin_read()
Action: cargo build --workspace
Assert: Zero compilation errors
```

### BV-compat-02: All tests pass after removal (RT-37)
```
Action: cargo test --workspace
Assert: All tests pass, no regressions in test count
```

## Static Analysis

### SA-compat-01: Zero compat references (RT-36)
```
Action: grep -r "open_table\|open_multimap\|begin_read\|TableU64Blob\|TableStrU64\|MultimapSpec\|TableSpec\|SqliteReadTransaction" crates/ --include="*.rs"
Assert: 0 hits (excluding comments/documentation)
```

### SA-compat-02: No runtime bincode for normalized tables (AC-15)
```
Action: grep -r 'bincode' crates/ --include="*.rs"
Assert: Hits only in:
  - migration_compat.rs (deserializers for v5 blobs)
  - OBSERVATION_METRICS paths (not normalized in this feature)
  - Cargo.toml dependency declaration
  - Test code creating synthetic v5 data
```

### SA-compat-03: No serialize_entry/deserialize_entry in runtime
```
Action: grep -r "serialize_entry\|deserialize_entry" crates/ --include="*.rs"
Assert: Only in migration_compat.rs and test code
```

### SA-compat-04: Index table names gone (AC-03)
```
Action: grep -r "topic_index\|category_index\|tag_index\|time_index\|status_index" crates/ --include="*.rs"
Assert: Zero hits in runtime code (may appear in migration_compat for old DDL)
```

## Files Verified Deleted

| File | Lines Removed |
|------|--------------|
| `crates/unimatrix-store/src/handles.rs` | ~428 |
| `crates/unimatrix-store/src/dispatch.rs` | ~134 |
| `crates/unimatrix-store/src/tables.rs` | ~182 |

## Files Verified Simplified

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/txn.rs` | ~89 lines -> ~35 lines (SqliteWriteTransaction only) |
| `crates/unimatrix-store/src/lib.rs` | Remove compat module declarations and re-exports |
| `crates/unimatrix-store/src/db.rs` | Remove begin_read() method |

## Clippy Verification

### CL-compat-01: Clean clippy
```
Action: cargo clippy --workspace -- -D warnings
Assert: Zero warnings
```
