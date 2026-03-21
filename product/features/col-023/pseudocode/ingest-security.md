# Pseudocode: ingest-security

**Wave**: 4 (depends on all of Waves 1-3)
**Crate**: `unimatrix-server`
**Files modified**:
- `crates/unimatrix-server/src/services/observation.rs` — parse_observation_rows, json_depth, SqlObservationSource
- `crates/unimatrix-server/src/lib.rs` — DomainPackRegistry startup wiring
- `crates/unimatrix-server/src/uds/listener.rs` — any remaining HookType references
- `crates/unimatrix-server/src/background.rs` — any remaining HookType references
- `crates/unimatrix-observe/tests/extraction_pipeline.rs` — test fixture updates

## Purpose

Rewrite `parse_observation_rows()` to:
1. Remove the `HookType` match arm and `_ => continue` drop behavior
2. Assign `source_domain = "claude-code"` for all hook-path records
3. Apply payload size check (≤ 64 KB) before JSON parse
4. Apply JSON depth check (≤ 10 levels) after parse
5. Use `DomainPackRegistry` to resolve `source_domain` for records not from the hook path

Wire `DomainPackRegistry` as `Arc` into `SqlObservationSource` at server startup.
Update all test fixture construction sites to supply `event_type` and `source_domain`.

## SqlObservationSource struct extension

Current:
```
pub struct SqlObservationSource:
    store: Arc<SqlxStore>
```

After:
```
pub struct SqlObservationSource:
    store: Arc<SqlxStore>
    registry: Arc<DomainPackRegistry>    -- NEW
```

Updated constructor:
```
impl SqlObservationSource:
    pub fn new(store: Arc<SqlxStore>, registry: Arc<DomainPackRegistry>) -> Self:
        SqlObservationSource { store, registry }
```

The `registry` field is threaded in from the server startup wiring in `lib.rs`.
It is passed to `parse_observation_rows()`.

## parse_observation_rows() rewrite

Current signature:
```
fn parse_observation_rows(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<ObservationRecord>>
```

New signature:
```
fn parse_observation_rows(
    rows: Vec<sqlx::sqlite::SqliteRow>,
    registry: &DomainPackRegistry,
) -> Result<Vec<ObservationRecord>>
```

New implementation:
```
fn parse_observation_rows(
    rows: Vec<sqlx::sqlite::SqliteRow>,
    registry: &DomainPackRegistry,
) -> Result<Vec<ObservationRecord>>:

    let mut records = Vec::new()

    for row in rows:
        let session_id: String = row.get(0)
        let ts_millis: i64 = row.get(1)
        let hook_str: String = row.get(2)        -- DB column still named "hook"
        let tool: Option<String> = row.get(3)
        let input_str: Option<String> = row.get(4)
        let response_size: Option<i64> = row.get(5)
        let response_snippet: Option<String> = row.get(6)

        -- Set event_type from the raw hook string (no filtering — FR-03.1)
        let event_type: String = hook_str

        -- Assign source_domain = "claude-code" for all hook-path records (FR-03.3)
        -- The hook ingress path is always claude-code; domain is inferred from ingress
        -- not from the payload or event_type string.
        let source_domain: String = "claude-code".to_string()

        -- SECURITY BOUND 1: payload size check (NFR-02, FR-03.4, ADR-007)
        -- Check raw bytes of input_str BEFORE any JSON parsing
        if let Some(ref s) = input_str:
            if s.len() > 65_536:  -- 64 KB = 65,536 bytes
                log::warn!(
                    "PayloadTooLarge: session={}, event_type={}, size={}",
                    session_id, event_type, s.len()
                )
                -- Skip this record; continue to next (FM-02)
                continue

        -- Input deserialization (event_type-conditional, not source_domain-conditional)
        -- SubagentStart: input is plain text → Value::String
        -- Tool events: input is JSON → parse to Value::Object
        -- Preserve existing event_type-conditional logic:
        let input: Option<serde_json::Value> = match (event_type.as_str(), input_str):
            ("SubagentStart", Some(s)) => Some(serde_json::Value::String(s))
            (_, Some(s)) =>
                match serde_json::from_str::<serde_json::Value>(&s):
                    Ok(v) => Some(v)
                    Err(_) => None   -- malformed JSON: treat as no input
            (_, None) => None

        -- SECURITY BOUND 2: JSON depth check (NFR-02, FR-03.5, ADR-007)
        -- Applied AFTER parse (must have a serde_json::Value to walk)
        if let Some(ref v) = input:
            if !json_depth(v, 0, 10):  -- max depth = 10
                log::warn!(
                    "PayloadNestingTooDeep: session={}, event_type={}",
                    session_id, event_type
                )
                -- Skip this record; continue to next (FM-02)
                continue

        records.push(ObservationRecord {
            ts: ts_millis as u64,
            event_type,
            source_domain,
            session_id,
            tool,
            input,
            response_size: response_size.map(|v| v as u64),
            response_snippet,
        })

    Ok(records)
```

Notes:
1. `source_domain` validation against `^[a-z0-9_-]{1,64}$` is applied at domain pack
   REGISTRATION (startup), not at ingest for hook-path records (which always get
   `"claude-code"` — a pre-validated string). For future ingress paths that accept
   client-declared `source_domain`, validation must be added here (SEC-05).

2. The `registry` parameter is available for `resolve_source_domain()` if a future
   ingress path provides an event_type without a known domain. For the hook path in
   W1-5, `source_domain` is always `"claude-code"` — the registry is not queried
   during normal operation. Pass it in anyway to satisfy the architecture contract
   (IR-01) and for use in tests.

3. The `_ => continue` behavior for unknown hook types is REMOVED (FR-03.1, AC-11).
   All event_type strings pass through.

## json_depth() helper function

New helper function in `services/observation.rs`:

```
/// Recursively check the nesting depth of a serde_json::Value.
///
/// Returns true if the value's nesting depth is <= max_depth.
/// Returns false immediately when the nesting depth exceeds max_depth.
///
/// O(n) walk over all nodes in the JSON tree; short-circuits at max_depth + 1.
/// Safe against stack overflow: at depth max_depth+1 (11 levels), recursion stops.
/// Combined with the 64 KB size pre-check, the total node count is bounded.
///
/// # Arguments
/// - `v`: the JSON value to inspect
/// - `current`: the current recursion depth (call with 0 from the top level)
/// - `max`: the maximum allowed depth (ADR-007 specifies 10)
fn json_depth(v: &serde_json::Value, current: usize, max: usize) -> bool:
    if current > max:
        return false   -- exceeded max depth: short-circuit

    match v:
        serde_json::Value::Object(map) =>
            for (_, child) in map.iter():
                if !json_depth(child, current + 1, max):
                    return false
            true

        serde_json::Value::Array(arr) =>
            for child in arr.iter():
                if !json_depth(child, current + 1, max):
                    return false
            true

        -- Scalar values (Null, Bool, Number, String) have no children
        _ => true
```

Depth semantics:
- `current = 0` at the root value
- An `Object { key: Object { key: ... } }` at depth 0 has one child at depth 1
- `json_depth(root, 0, 10)` returns false if any node is at depth 11

Boundary test:
- Depth exactly 10: `json_depth(depth_10_object, 0, 10)` returns true
- Depth 11: returns false (the guard `if current > max` fires at current=11 > max=10)

## lib.rs startup wiring

In the server startup sequence (approximately where `SqlObservationSource` is currently
constructed):

```
-- OLD:
let obs_source = SqlObservationSource::new(store.clone())

-- NEW:
-- 1. Convert DomainPackConfig entries to DomainPack
let packs: Vec<DomainPack> = config.observation.domain_packs
    .into_iter()
    .map(domain_pack_from_config)
    .collect::<Result<_, _>>()?

-- 2. Build registry (validates all rule descriptors, "claude-code" always included)
let registry = DomainPackRegistry::new(packs)?

-- 3. Register domain pack categories into CategoryAllowlist (IR-02 ordering constraint)
for pack in registry.iter_packs():   -- add iter_packs() if not already present
    for category in &pack.categories:
        category_allowlist.add_category(category)

-- 4. Thread registry as Arc into SqlObservationSource
let registry_arc = Arc::new(registry)
let obs_source = SqlObservationSource::new(store.clone(), registry_arc)
```

`iter_packs()` on `DomainPackRegistry`:
```
pub fn iter_packs(&self) -> Vec<DomainPack>:
    let guard = self.inner.read().unwrap_or_else(|e| e.into_inner())
    guard.values().cloned().collect()
```

## All internal calls to parse_observation_rows

The function is called from two places in `SqlObservationSource`:
- `load_feature_observations()`
- `load_unattributed_sessions()`

Both call sites must pass `&self.registry`:
```
-- OLD: parse_observation_rows(rows)
-- NEW: parse_observation_rows(rows, &self.registry)
```

## uds/listener.rs and background.rs

These files may reference `HookType` directly. After Wave 4:
- Remove any `use unimatrix_observe::types::HookType` imports
- Replace any remaining `HookType::X` references with the string equivalents

If no direct `HookType` references exist in these files, no change is needed.
Use `grep -r "HookType" crates/unimatrix-server/` to identify all remaining callsites.

## tests/extraction_pipeline.rs (and all other test fixture sites)

All `ObservationRecord` construction sites in test files must supply both fields:
```
-- OLD:
ObservationRecord { hook: HookType::PreToolUse, session_id: "sess-1", ... }

-- NEW:
ObservationRecord {
    event_type: "PreToolUse".to_string(),
    source_domain: "claude-code".to_string(),
    session_id: "sess-1".to_string(),
    ...
}
```

Static verification after Wave 4:
```
grep -r 'source_domain: ""' unimatrix-observe/tests/  -- must return zero matches
grep -r 'HookType::' crates/                           -- must return zero matches
```

## Error Handling

- **PayloadTooLarge**: log WARN, skip the record, continue processing remaining records
  in the session (FM-02). Do NOT return an error that would abort the entire session.
- **PayloadNestingTooDeep**: same as PayloadTooLarge — log and skip.
- **DomainPackRegistry startup failure**: propagated as a startup error before the server
  accepts any requests (FM-01). Client-side: server does not start.
- **RwLock poison**: `registry.inner.read().unwrap_or_else(|e| e.into_inner())` — same
  recovery pattern as `CategoryAllowlist` (established pattern, no new behavior).
- **JSON parse failure on input**: `serde_json::from_str(...).ok()` → `None` for input.
  Record is still accepted, just with no input value.

## Key Test Scenarios

1. **AC-11 unknown event passthrough**: insert a record with an unregistered `event_type`
   (e.g., `"widget_exploded"`); assert it appears in `parse_observation_rows` output with
   `source_domain = "claude-code"` and is NOT dropped.

2. **AC-06 payload size boundary**: payload of exactly 65,536 bytes passes;
   65,537 bytes is skipped with a WARN log. Session continues without the oversized record.

3. **AC-06 depth boundary**: JSON of exactly 10 levels deep passes;
   11 levels deep is skipped with a WARN log.

4. **SEC-01 multi-byte UTF-8**: a payload with exactly 65,536 UTF-8 bytes (multi-byte
   characters) passes; 65,537 raw bytes is rejected regardless of character count.

5. **json_depth() edge cases**:
   - Empty object `{}` at depth 0: returns true (depth 1 is within limit)
   - Scalar value `42` at depth 0: returns true
   - Nested array at depth 10: returns true; depth 11: returns false

6. **IR-01 registry injection**: create `SqlObservationSource` with a real
   `DomainPackRegistry` containing the claude-code pack; ingest `"PreToolUse"` event;
   assert `source_domain = "claude-code"` in the output record.

7. **R-03 test fixture audit**: all existing service tests updated to construct
   `ObservationRecord` with both `event_type` and `source_domain`. No test uses
   `hook: HookType::...` anywhere.

8. **R-09 startup failure on bad rule_file**: provide a `DomainPackConfig` with a
   non-existent `rule_file` path; assert server startup returns an error naming the file.

9. **SubagentStart input preserved as String**: `"SubagentStart"` event with plain-text
   input passes the depth check (depth 0 for a `Value::String`) and is stored as
   `Some(Value::String(...))`.

10. **Mixed event session**: session with valid + oversized records; assert valid records
    appear in output and oversized records are absent (FM-02 behavior).

11. **AC-03 default registry**: server started with no `[observation]` config; assert
    `DomainPackRegistry` contains the claude-code pack and `"PreToolUse"` events are
    processed correctly.
