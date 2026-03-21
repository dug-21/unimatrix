## ADR-003: Rule Evaluator DSL for Data-Driven Detection Rules

### Context

SR-01 (scope risk assessment) identified that `serde_json`'s `json_pointer` facility is
path-extraction only — it cannot express temporal window rules (N events within T seconds)
or count aggregations across records. The scope commits to supporting both threshold rules
and temporal window rules for external domain packs without new crate dependencies.

This creates a design gap: the chosen DSL tool (`json_pointer`) is insufficient for
temporal aggregation, but a full Turing-complete evaluator is explicitly out of scope
(security constraint, no `eval`, no dynamic code loading).

Three options were evaluated:

**Option A: json_pointer + host-side aggregation without DSL**
Rules consist of a filter expression (which records to count) and an aggregation spec
(what to count / measure). The `RuleEvaluator` struct in the registry module implements
the aggregation logic. Rule descriptors are pure data — no code execution.

**Option B: Embedded scripting (Lua, Rhai)**
Rejected: new crate dependency, security surface, Turing-complete (explicitly forbidden
by scope constraints).

**Option C: SQL-like expression parser**
Rejected: new crate dependency; overkill for the narrow use case.

**Option A is chosen.** The DSL is a declarative rule descriptor that the `RuleEvaluator`
struct interprets. It has a fixed, bounded operator set with no Turing-complete features.

### Decision

The Rule Descriptor DSL is expressed as TOML (for config-file rules) or JSON (for runtime
submission). It has two rule kinds:

**Kind 1: Threshold rule**
Counts records matching a filter and fires if the count exceeds a threshold.

```toml
[[observation.domain_packs.rules]]
name = "high_incident_rate"
kind = "threshold"
source_domain = "sre"           # REQUIRED: explicit domain guard
event_type_filter = ["incident_opened"]  # empty = all event types in domain
field_path = ""                 # json_pointer into payload (empty = count events)
threshold = 5.0
severity = "warning"
claim_template = "High incident rate: {measured} incidents"
```

**Kind 2: Temporal window rule**
Counts events of a specific type within a rolling time window and fires if count exceeds
threshold.

```toml
[[observation.domain_packs.rules]]
name = "alert_storm"
kind = "temporal_window"
source_domain = "sre"           # REQUIRED: explicit domain guard
event_type_filter = ["alert_fired"]
window_secs = 300               # 5-minute window
threshold = 10.0
severity = "critical"
claim_template = "Alert storm: {measured} alerts in {window_secs}s window"
```

The `RuleEvaluator` struct:

```rust
pub struct RuleEvaluator {
    descriptor: RuleDescriptor,
}

impl DetectionRule for RuleEvaluator {
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        // 1. Filter by source_domain (MANDATORY guard — no cross-domain evaluation)
        // 2. Filter by event_type_filter (empty = all event types for this domain)
        // 3. Apply kind-specific aggregation:
        //    - threshold: count filtered records; optionally extract field_path value
        //    - temporal_window: sliding max-count within window_secs
        // 4. Compare against threshold and emit finding if exceeded
    }
}
```

**Operator surface (complete and bounded):**
- `threshold`: count of matching records > threshold
- `temporal_window`: max events within T seconds > threshold

No other operators. Field extraction via `json_pointer` is available only for the
`threshold` kind when `field_path` is non-empty, and it must resolve to a numeric value.
String matching on payload fields is not supported in v1 — rules filter on `event_type`
and `source_domain` only.

**Temporal state management:**
Temporal window rules do not maintain cross-call state. The `detect()` method receives
the full `records` slice for the session. The evaluator scans the slice with a two-pointer
window over the sorted-by-timestamp records. This is O(n) per rule and stateless between
detect() calls. No external state store is needed.

**Security bounds:**
- `source_domain` guard is validated at rule-load time: rules without a `source_domain`
  are rejected with a startup error
- `field_path` strings are validated as legal JSON Pointer syntax at load time; execution
  calls `serde_json::Value::pointer()` which is already a transitive dependency
- No filesystem or env references in rule descriptors
- Rule descriptors are parsed at startup; malformed descriptors fail fast with a clear
  error message and prevent server start

**Built-in Rust rules (claude-code domain pack):**
The 21 built-in detection rules are NOT converted to the DSL. They remain as Rust
`DetectionRule` implementations. The DSL is only for external domain packs specified via
`rule_file` in TOML config or inline rule descriptors. This preserves the full
expressiveness of the existing rules.

### Consequences

**Easier:**
- No new crate dependencies — uses only `serde_json::Value::pointer()` and `serde` for
  TOML deserialization, both already transitive dependencies
- Bounded operator surface makes security review straightforward
- `RuleEvaluator` is testable in isolation with synthetic `ObservationRecord` slices
- Temporal window logic is stateless across calls — no complexity from shared mutable state

**Harder:**
- External domain pack authors are constrained to threshold and temporal window rules only;
  more complex patterns (e.g., sequence detection, ratio computation) require a Rust
  `DetectionRule` implementation
- `field_path` numeric extraction is limited: non-numeric fields silently produce no
  finding rather than an error (evaluate-time skip, not startup error)
- The two-pointer temporal window assumes records are sorted by timestamp; `detect()` must
  sort or verify sort order before applying temporal rules
