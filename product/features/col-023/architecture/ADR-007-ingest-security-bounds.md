## ADR-007: Ingest Security Bounds for Untyped External Event Payloads

### Context

After the `ObservationRecord` generalization, payloads from any domain flow through the
pipeline without schema validation. The existing `HookType` enum provided an implicit
safety bound: only 4 known event types were accepted, and their payloads were validated by
the Claude Code hook schema. With arbitrary `event_type` strings and free-form JSON
payloads, the ingest boundary must enforce explicit security constraints.

The scope defines four bounds:
1. Payload max 64 KB
2. JSON nesting depth ≤ 10 levels
3. `source_domain` validated `^[a-z0-9_-]{1,64}$`
4. Extraction rule sandboxing (JSON path only, no eval, no fs/env)

These bounds must be enforced at the ingest point, before the payload reaches the
observation pipeline or storage layer.

### Decision

All security checks are applied in `parse_observation_rows()` (and at the equivalent
boundary for any future non-hook ingress path). Violations produce specific `ObserveError`
variants and are logged at WARN level. The event is discarded — not stored, not processed.

**Bound 1: Payload size (64 KB)**
Before JSON deserialization, the raw `input` string byte length is checked:
```rust
if input_str.len() > 65_536 {
    return Err(ObserveError::PayloadTooLarge {
        session_id,
        event_type,
        size: input_str.len(),
    });
}
```
The check is on raw bytes before parse, so it cannot be bypassed by Unicode tricks.

**Bound 2: JSON nesting depth (≤ 10 levels)**
A recursive depth-counter function walks the deserialized `serde_json::Value` and rejects
nesting deeper than 10:
```rust
fn json_depth(v: &serde_json::Value, current: usize, max: usize) -> bool {
    if current > max { return false; }
    match v {
        Value::Object(m) => m.values().all(|child| json_depth(child, current + 1, max)),
        Value::Array(a)  => a.iter().all(|child| json_depth(child, current + 1, max)),
        _ => true,
    }
}
```
This is O(n) in the total number of values. Combined with the 64 KB size bound, the
maximum number of values is bounded — no unbounded recursion.

**Bound 3: source_domain validation**
`source_domain` is validated at two points:
- At domain pack registration (startup config load): invalid domain names prevent server start
- At ingest (for any future path where source_domain might be externally supplied):
  `^[a-z0-9_-]{1,64}$` validated via a regex compiled at startup and stored as a `OnceLock<Regex>`

For the hook ingress path (W1-5 scope), `source_domain` is always `"claude-code"` — set
server-side, never client-declared — so ingest validation is a no-op for the current path.
The validation is implemented now so future ingress paths inherit it automatically.

**Bound 4: Extraction rule sandboxing**
As specified in ADR-003: rule descriptors are parsed at startup. The `field_path` in
rule descriptors is validated as a legal JSON Pointer string (`/`-delimited path, no
`..`, no `$`, no env references) at load time. The `serde_json::Value::pointer()`
function is the only runtime evaluation path — no `eval`, no filesystem reads, no
environment variable access. Rule descriptors that fail validation prevent server start.

**Error variants added to ObserveError:**
```rust
PayloadTooLarge { session_id: String, event_type: String, size: usize }
PayloadNestingTooDeep { session_id: String, event_type: String, depth: usize }
InvalidSourceDomain { domain: String }
InvalidRuleDescriptor { rule_name: String, reason: String }
```

**Behavior for rejected events:**
- `PayloadTooLarge` and `PayloadNestingTooDeep`: event is skipped; remainder of session
  is processed normally; error is logged at WARN
- `InvalidSourceDomain`: server start fails if in a domain pack; at ingest for future
  paths, event is skipped with WARN log
- `InvalidRuleDescriptor`: server start fails

### Consequences

**Easier:**
- All four bounds are enforced at a single ingest boundary — no defense-in-depth needed
  across multiple pipeline stages
- The depth check is stateless and requires no new dependencies
- Server start failure on invalid rule descriptors prevents silent misconfiguration

**Harder:**
- The size and depth bounds are per-record, not per-session: a pathological session with
  many 63-KB payloads is accepted; only payloads exceeding exactly one bound per record
  are rejected
- The `json_depth` function uses recursion; a 10-level limit makes stack depth safe but
  the implementor must confirm there is no possibility of a malformed payload with 10
  levels of array nesting reaching the recursion limit before the depth check fires
  (confirmed safe: the depth counter short-circuits at level > 10 before recursing deeper)
- `source_domain` validation on the hook ingress path is a no-op today but adds a code
  path that must be tested; the structural test for this must use synthetic non-hook
  ingress scenarios (AC-05 synthetic test)
