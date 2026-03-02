# Test Plan: session-registry

## Component Scope

New module `crates/unimatrix-server/src/session.rs`: SessionState, InjectionRecord, SessionRegistry.

## Risk Coverage

| Risk | Test |
|------|------|
| R-01 (lock contention) | Sequential access pattern tests |
| R-07 (CoAccessDedup regression) | Replicated CoAccessDedup tests |

## Unit Tests

### Session Lifecycle

#### test_register_and_get_state
```
Arrange: new SessionRegistry
Act: register_session("s1", Some("dev"), Some("col-008"))
Assert: get_state("s1") returns SessionState with role="dev", feature="col-008", empty injection_history, compaction_count=0
```

#### test_register_overwrites_existing
```
Arrange: registry with registered session "s1" (role="dev")
Act: register_session("s1", Some("architect"), None)
Assert: get_state("s1").role == Some("architect"), injection_history is empty (fresh state)
```

#### test_get_state_unknown_session
```
Arrange: new SessionRegistry
Act: get_state("unknown")
Assert: returns None
```

#### test_clear_session
```
Arrange: registry with registered session "s1"
Act: clear_session("s1")
Assert: get_state("s1") returns None
```

#### test_clear_session_unknown_noop
```
Arrange: new SessionRegistry
Act: clear_session("unknown")
Assert: no panic, no error
```

#### test_clear_session_only_affects_target
```
Arrange: registry with sessions "s1" and "s2"
Act: clear_session("s1")
Assert: get_state("s1") == None, get_state("s2") is Some
```

### Injection History

#### test_record_injection
```
Arrange: registry with session "s1"
Act: record_injection("s1", &[(1, 0.8), (2, 0.6)])
Assert: get_state("s1").injection_history has 2 records with correct entry_ids and confidences
```

#### test_record_injection_accumulates
```
Arrange: registry with session "s1"
Act: record_injection("s1", &[(1, 0.8)]); record_injection("s1", &[(2, 0.6)])
Assert: injection_history has 2 records total
```

#### test_record_injection_allows_duplicates
```
Arrange: registry with session "s1"
Act: record_injection("s1", &[(1, 0.8)]); record_injection("s1", &[(1, 0.9)])
Assert: injection_history has 2 records (both entry_id=1)
```

#### test_record_injection_unregistered_session_noop
```
Arrange: new SessionRegistry
Act: record_injection("unknown", &[(1, 0.8)])
Assert: no panic, get_state("unknown") == None
```

#### test_record_injection_sets_timestamp
```
Arrange: registry with session "s1"
Act: record_injection("s1", &[(1, 0.8)])
Assert: injection_history[0].timestamp > 0
```

### Co-Access Dedup (Replicate CoAccessDedup tests)

#### test_coaccess_new_set_returns_true
```
Arrange: registry with session "s1"
Act: check_and_insert_coaccess("s1", &[1, 2, 3])
Assert: returns true
```

#### test_coaccess_duplicate_returns_false
```
Arrange: registry with session "s1", already inserted [1,2,3]
Act: check_and_insert_coaccess("s1", &[1, 2, 3])
Assert: returns false
```

#### test_coaccess_different_set_returns_true
```
Arrange: registry with session "s1", already inserted [1,2,3]
Act: check_and_insert_coaccess("s1", &[1, 2, 4])
Assert: returns true
```

#### test_coaccess_different_session_returns_true
```
Arrange: registry with sessions "s1" and "s2", s1 has [1,2,3]
Act: check_and_insert_coaccess("s2", &[1, 2, 3])
Assert: returns true
```

#### test_coaccess_canonical_ordering
```
Arrange: registry with session "s1", inserted [3, 1, 2]
Act: check_and_insert_coaccess("s1", &[1, 2, 3])
Assert: returns false (same set, different order)
```

#### test_coaccess_clear_resets
```
Arrange: registry with session "s1", inserted [1,2,3]
Act: clear_session("s1"); register_session("s1", ...); check_and_insert_coaccess("s1", &[1,2,3])
Assert: returns true (fresh after clear)
```

#### test_coaccess_unregistered_session_returns_false
```
Arrange: new SessionRegistry
Act: check_and_insert_coaccess("unknown", &[1, 2, 3])
Assert: returns false
```

### Compaction Count

#### test_increment_compaction
```
Arrange: registry with session "s1"
Act: increment_compaction("s1")
Assert: get_state("s1").compaction_count == 1
```

#### test_increment_compaction_accumulates
```
Arrange: registry with session "s1"
Act: increment_compaction("s1"); increment_compaction("s1")
Assert: compaction_count == 2
```

#### test_increment_compaction_unregistered_noop
```
Arrange: new SessionRegistry
Act: increment_compaction("unknown")
Assert: no panic
```
