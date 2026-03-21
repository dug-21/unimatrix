## ADR-005: Mandatory source_domain Guards in All Detection Rules

### Context

SR-07 (scope risk assessment) identified a silent data contamination risk: after
`parse_observation_rows` is changed to pass through unknown event types (rather than
dropping them with `_ => continue`), detection rules written for `"claude-code"` will
receive records from any domain.

The current 21 rules check `record.tool == Some("Bash")`, `record.hook == HookType::PreToolUse`,
etc. — they implicitly assume all records are Claude Code records because the `_ => continue`
filter prevented anything else from reaching them. After AC-11 (no silent drop), rules for
`"claude-code"` will see `source_domain = "unknown"` records and potentially `source_domain
= "sre"` records if multiple domains are registered.

A false finding occurs when a rule for domain X fires on an event from domain Y. For
example, if the `sleep_workarounds` rule matches a record with `event_type = "alert_fired"`
from an SRE domain (because it had `tool = "Bash"` in its payload for unrelated reasons),
it produces a phantom hotspot finding in the claude-code retrospective.

These false findings are silent and post-merge — they do not fail CI, they appear in
retrospective reports, and they are difficult to trace back to the cross-domain contamination.

### Decision

Every `DetectionRule` implementation — both built-in Rust rules and data-driven
`RuleEvaluator` instances — must filter `records` to its own `source_domain` as the first
operation in `detect()`. This is a **spec-level architectural contract**, not an
implementation suggestion.

**For built-in Rust rules (claude-code domain):**
Each of the 21 rules must begin with a filter:
```rust
let records: Vec<&ObservationRecord> = records
    .iter()
    .filter(|r| r.source_domain == "claude-code")
    .collect();
// All subsequent logic operates only on this filtered slice
```

This filter is the standard preamble for all claude-code detection rules. It is not
optional and is validated in the gate review checklist.

**For data-driven RuleEvaluator rules:**
The `source_domain` field in the rule descriptor is required (startup validation rejects
rules without it). The `RuleEvaluator::detect()` implementation applies the
`source_domain` filter before any other evaluation, regardless of the rule kind.

**For `find_completion_boundary()` and other shared helpers:**
These helpers must also guard on `source_domain = "claude-code"` before inspecting
`record.tool` for Claude Code-specific tool names like `"TaskUpdate"`. The shared helper
call sites in the detection modules apply the pre-filtered slice.

**Enforcement mechanism:**
The gate-3a review checklist for col-023 includes an explicit item:
"Every DetectionRule::detect() implementation applies source_domain filter as first operation."

Unit tests for each rule must include a test case that supplies a mix of `"claude-code"`
and `"unknown"` records and verifies the rule produces zero findings for the
`"unknown"` records.

### Consequences

**Easier:**
- Cross-domain false findings are architecturally impossible when the contract is followed
- Rules can be tested in isolation with synthetic mixed-domain record slices
- Future domain pack authors have an explicit, unambiguous contract to follow

**Harder:**
- The filter preamble is boilerplate that must be added to all 21 existing rules — the
  Wave 3 implementor must not skip any rule
- The helper functions `find_completion_boundary()`, `input_to_file_path()`,
  `input_to_command_string()` do not enforce the guard themselves — they operate on
  whatever slice they receive, so the responsibility lies with the caller
- The gate reviewer must manually verify the guard is present in each rule implementation
