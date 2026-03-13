# Test Plan: uds-listener (C3)

**File under test**: `crates/unimatrix-server/src/uds/listener.rs`
**Risks covered**: R-01, R-04 (partial), R-06, R-07, R-10, R-12

## Unit Tests

Follow existing pattern: `make_store()`, `make_registry()`, `make_dispatch_deps()` helpers.

### cycle_start dispatch (R-01 -- High Priority)

```
test_dispatch_cycle_start_sets_feature_force
  Arrange: Register session "s1" with no feature. Build RecordEvent with event_type="cycle_start",
           payload={"feature_cycle":"col-022"}
  Act: dispatch_request(RecordEvent { event })
  Assert: registry.get_state("s1").feature == Some("col-022")

test_dispatch_cycle_start_overwrites_heuristic_attribution
  Arrange: Register session "s1" with feature="col-017" (set by eager attribution).
           Build cycle_start event with feature_cycle="col-022"
  Act: dispatch_request(RecordEvent { event })
  Assert: registry.get_state("s1").feature == Some("col-022")
  Assert: SetFeatureResult::Overridden { previous: "col-017" } (verify via log or return)

test_dispatch_cycle_start_already_matches
  Arrange: Register session "s1" with feature="col-022".
           Build cycle_start event with feature_cycle="col-022"
  Act: dispatch_request(RecordEvent { event })
  Assert: registry.get_state("s1").feature == Some("col-022") (unchanged)

test_dispatch_cycle_start_unknown_session
  Arrange: No session registered. Build cycle_start event for session "unknown"
  Act: dispatch_request(RecordEvent { event })
  Assert: No panic. Observation still recorded. Feature not set anywhere.
```

### Keywords persistence (R-06)

```
test_dispatch_cycle_start_persists_keywords
  Arrange: Register session "s1". Build cycle_start event with
           payload={"feature_cycle":"col-022","keywords":["attr","lifecycle"]}
  Act: dispatch_request(RecordEvent { event })
  Assert: After spawn_blocking settles, query sessions table:
          session.keywords == Some(r#"["attr","lifecycle"]"#)

test_dispatch_cycle_start_empty_keywords_stored_as_empty_array
  Arrange: Build cycle_start event with keywords=[]
  Act: dispatch_request
  Assert: session.keywords == Some("[]"), NOT None

test_dispatch_cycle_start_no_keywords_field
  Arrange: Build cycle_start event with payload={"feature_cycle":"col-022"} (no keywords key)
  Act: dispatch_request
  Assert: session.keywords == None (NULL in SQLite)

test_keywords_json_special_characters
  Arrange: keywords=["has \"quotes\"", "back\\slash", "uni\u00e9code"]
  Act: dispatch_request
  Assert: Round-trip: deserialize stored keywords JSON back to Vec<String>, verify equality
```

### update_session_keywords function (R-10)

```
test_update_session_keywords_valid
  Arrange: Insert session row in SQLite. keywords_json = r#"["a","b"]"#
  Act: update_session_keywords(store, "s1", keywords_json)
  Assert: Query session row, keywords column == r#"["a","b"]"#

test_update_session_keywords_unknown_session
  Arrange: No session row exists for "unknown"
  Act: update_session_keywords(store, "unknown", "[]")
  Assert: Returns Ok (no-op or graceful error), does NOT panic

test_update_session_keywords_malformed_json
  Arrange: Insert session row. keywords_json = "not-json"
  Act: update_session_keywords(store, "s1", "not-json")
  Assert: Function succeeds (raw TEXT column, no JSON validation at SQLite level)
  Note: Validation happens upstream in validate_cycle_params. This function stores as-is.
```

### cycle_stop dispatch (R-12)

```
test_dispatch_cycle_stop_records_observation
  Arrange: Register session "s1" with feature="col-022".
           Build RecordEvent with event_type="cycle_stop", payload={"feature_cycle":"col-022"}
  Act: dispatch_request
  Assert: Observation row exists with event_type="cycle_stop", session_id="s1"

test_dispatch_cycle_stop_does_not_modify_feature
  Arrange: Register session "s1" with feature="col-022".
           Build cycle_stop event
  Act: dispatch_request
  Assert: registry.get_state("s1").feature == Some("col-022") (unchanged)

test_dispatch_cycle_stop_without_prior_start
  Arrange: Register session "s1" with no feature. Build cycle_stop event
  Act: dispatch_request
  Assert: Observation recorded. Feature remains None.
```

### Event type constant agreement (R-04)

```
test_dispatch_cycle_start_matches_hook_constant
  Arrange: Build event using the SAME constant that hook.rs uses for cycle_start
  Act: dispatch_request
  Assert: cycle_start-specific handler runs (not generic RecordEvent handler)
  Note: If constants are defined in a shared module, this is a compile-time guarantee.
        If strings are inline, this test must explicitly verify the string values match.
```

### Concurrent force-set (R-07)

```
test_set_feature_force_sequential_different_topics
  Arrange: Register session "s1", no feature
  Act: set_feature_force("s1", "col-017"), then set_feature_force("s1", "col-022")
  Assert: Final feature == "col-022" (last writer wins)

test_set_feature_force_preserves_heuristic_path
  Arrange: Register session "s1", no feature
  Act: set_feature_if_absent("s1", "col-017") -- succeeds (heuristic)
       set_feature_force("s1", "col-022") -- overwrites (explicit)
       set_feature_if_absent("s1", "col-099") -- fails (already set)
  Assert: Final feature == "col-022"
```

## Integration Tests (Rust)

```
test_cycle_start_end_to_end_persistence
  Arrange: Full dispatch setup (store + registry + services)
  Act: dispatch cycle_start with feature_cycle and keywords
  Assert: SessionRegistry in-memory state matches
  Assert: SQLite sessions table row has feature_cycle and keywords populated
  Note: Must wait for spawn_blocking tasks to complete (use tokio::time::sleep or task joining)
```

## Edge Cases

- cycle_start with empty payload (no feature_cycle key) -- should fall through to generic handler
- Very large keywords array after validation (5 items x 64 chars = 320 chars JSON) -- within TEXT column limits
- cycle_start on a session that was already closed (status=Completed) -- session may not be in registry
