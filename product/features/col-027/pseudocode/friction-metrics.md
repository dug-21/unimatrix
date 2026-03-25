# Component: friction-metrics

**Files (atomic — all three must ship in the same commit):**
- `crates/unimatrix-observe/src/detection/friction.rs` — `PermissionRetriesRule` fix + `ToolFailureRule` + `make_failure` test helper
- `crates/unimatrix-observe/src/detection/mod.rs` — `default_rules()` registration + count update
- `crates/unimatrix-observe/src/metrics.rs` — `compute_universal()` terminal bucket fix

**Wave:** 2 (depends on core-constants for `hook_type::POSTTOOLUSEFAILURE`)
**Action:** Modify three files atomically (ADR-004, FR-06.4)

---

## Purpose

Fix the Pre-Post differential in two independent implementations that both undercount terminal
events by excluding `PostToolUseFailure`. Add `ToolFailureRule` to surface genuine per-tool
failure signals now that failure records will exist.

**Why all three files must be one commit (ADR-004):**
`friction.rs PermissionRetriesRule` and `metrics.rs compute_universal()` implement the same
Pre-Post differential independently. A partial fix causes them to report contradictory signals
from identical observation data. The integration test (T-FM-07) asserts both sites agree in
the same test function — this test enforces the atomicity requirement.

---

## Changes in friction.rs

### Change 1: PermissionRetriesRule — widen terminal bucket to include PostToolUseFailure

**Current algorithm (lines 30-75 in existing file):**
```
post_counts: HashMap<String, u64>  -- only counts "PostToolUse"
retries = pre_count.saturating_sub(post_count)
```

**Fixed algorithm:**

```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
    let records: Vec<&ObservationRecord> = records
        .iter()
        .filter(|r| r.source_domain == "claude-code")  // unchanged
        .collect();

    let mut pre_counts: HashMap<String, u64> = HashMap::new();
    // CHANGED: renamed from post_counts to terminal_counts (ADR-004)
    // Semantics widened: both PostToolUse and PostToolUseFailure are terminal events
    let mut terminal_counts: HashMap<String, u64> = HashMap::new();
    let mut evidence_records: HashMap<String, Vec<EvidenceRecord>> = HashMap::new();

    for record in &records {
        if let Some(tool) = &record.tool {
            if record.event_type == "PreToolUse" {
                *pre_counts.entry(tool.clone()).or_default() += 1;
                evidence_records
                    .entry(tool.clone())
                    .or_default()
                    .push(EvidenceRecord {
                        description: format!("PreToolUse for {tool}"),
                        ts: record.ts,
                        tool: Some(tool.clone()),
                        detail: format!("Pre-use event at ts={}", record.ts),
                    });
            } else if record.event_type == "PostToolUse" {
                // unchanged: PostToolUse is a terminal event
                *terminal_counts.entry(tool.clone()).or_default() += 1;
            } else if record.event_type == hook_type::POSTTOOLUSEFAILURE {
                // NEW: PostToolUseFailure is also a terminal event (col-027)
                // A failed call is still a resolved call -- not a retried/blocked call
                *terminal_counts.entry(tool.clone()).or_default() += 1;
            }
        }
    }

    let threshold = 2.0;
    let mut findings = Vec::new();

    for (tool, pre_count) in &pre_counts {
        // CHANGED: use terminal_counts instead of post_counts
        let terminal_count = terminal_counts.get(tool).copied().unwrap_or(0);
        let retries = pre_count.saturating_sub(terminal_count);
        if retries > threshold as u64 {
            findings.push(HotspotFinding {
                category: HotspotCategory::Friction,
                severity: Severity::Warning,
                rule_name: "permission_retries".to_string(), // UNCHANGED
                claim: format!(
                    "Tool '{tool}' had {retries} permission retries (Pre-Post differential)"
                ),                                            // UNCHANGED
                measured: retries as f64,
                threshold,
                evidence: evidence_records.remove(tool).unwrap_or_default(),
            });
        }
    }

    findings
}
```

**What does NOT change:**
- `rule_name`: `"permission_retries"` (deferred to col-028)
- `category`: `HotspotCategory::Friction`
- `severity`: `Severity::Warning`
- `claim` format string
- `threshold`: 2.0
- `source_domain == "claude-code"` pre-filter

**Import requirement:** `use unimatrix_core::observation::hook_type;` must be added to friction.rs
if not already imported. Check existing imports — metrics.rs already has this import; friction.rs
may not. Add if missing.

---

### Change 2: ToolFailureRule — new struct implementing DetectionRule

**Location:** Add after `OutputParsingStruggleRule` (the last existing friction rule), before the
test module. This maintains the module's rule ordering pattern.

**Constant (top of friction.rs, alongside other thresholds like SEARCH_VIA_BASH_THRESHOLD_PCT):**

```
// col-027: ToolFailureRule threshold.
// Fires when a single tool accumulates strictly more than this many PostToolUseFailure records.
// ADR-005: hardcoded constant for col-027; extract to named constant for future configurability.
const TOOL_FAILURE_THRESHOLD: u64 = 3;
```

**Struct declaration:**

```
// col-027: New rule to surface per-tool failure counts
pub(crate) struct ToolFailureRule;
```

**DetectionRule implementation:**

```
impl DetectionRule for ToolFailureRule {
    fn name(&self) -> &str {
        "tool_failure_hotspot"  // ADR-005: canonical name; do not use "tool_failures"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        // Pre-filter: source_domain == "claude-code" only (ADR-005, R-07)
        // Consistent with all other friction rules
        let records: Vec<&ObservationRecord> = records
            .iter()
            .filter(|r| r.source_domain == "claude-code")
            .collect();

        // Count PostToolUseFailure records per tool + collect evidence
        let mut failure_counts: HashMap<String, u64> = HashMap::new();
        let mut evidence_map: HashMap<String, Vec<EvidenceRecord>> = HashMap::new();

        for record in &records {
            // Only count PostToolUseFailure events (event_type exact match)
            if record.event_type != hook_type::POSTTOOLUSEFAILURE {
                continue;
            }
            // Skip records with no tool (can happen if tool_name was absent in payload)
            let tool = match record.tool.as_ref() {
                Some(t) => t,
                None => continue,
            };

            *failure_counts.entry(tool.clone()).or_default() += 1;

            // Collect evidence: one EvidenceRecord per failure event (FR-07.7)
            evidence_map
                .entry(tool.clone())
                .or_default()
                .push(EvidenceRecord {
                    description: format!("PostToolUseFailure for {tool}"),
                    ts: record.ts,
                    tool: Some(tool.clone()),
                    // detail = response_snippet (the error string) if present (ADR-005)
                    detail: record
                        .response_snippet
                        .clone()
                        .unwrap_or_default(),
                });
        }

        // Emit one finding per tool that exceeds TOOL_FAILURE_THRESHOLD
        // Threshold is STRICTLY greater than (ADR-005): fires at count > 3, i.e., 4+
        let mut findings = Vec::new();
        for (tool, count) in &failure_counts {
            if *count > TOOL_FAILURE_THRESHOLD {
                findings.push(HotspotFinding {
                    category: HotspotCategory::Friction,
                    severity: Severity::Warning,
                    rule_name: "tool_failure_hotspot".to_string(),
                    // Claim format: "Tool 'X' failed N times" (FR-07.4)
                    claim: format!("Tool '{tool}' failed {count} times"),
                    measured: *count as f64,
                    threshold: TOOL_FAILURE_THRESHOLD as f64,  // 3.0
                    evidence: evidence_map.remove(tool).unwrap_or_default(),
                });
            }
        }

        findings
    }
}
```

**Key invariants:**
- One finding per tool exceeding threshold (not aggregate) — matches `PermissionRetriesRule` pattern
- `measured = count as f64` — the actual failure count
- `threshold = 3.0` — the constant, not the count
- Evidence: one `EvidenceRecord` per failure event for the tool (FR-07.7)

---

### Change 3: make_failure test helper — add to test module

**Location:** In `#[cfg(test)] mod tests { ... }` at the bottom of friction.rs, alongside
existing `make_pre` and `make_post` helpers.

**Pattern mirror (make_pre and make_post structure):**

```
// In the test module:
fn make_failure(ts: u64, tool: &str) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: "PostToolUseFailure".to_string(),  // or: hook_type::POSTTOOLUSEFAILURE.to_string()
        source_domain: "claude-code".to_string(),
        session_id: "sess-1".to_string(),
        tool: Some(tool.to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    }
}
```

Note: `make_pre` and `make_post` do not set `response_snippet`; `make_failure` follows the same
convention. Tests that need `response_snippet` should construct the record inline or modify the
helper result.

---

## Changes in mod.rs

### Change: default_rules() — register ToolFailureRule, update count comments

**Current friction group (4 rules):**
```rust
// Friction (4)
Box::new(friction::PermissionRetriesRule),
Box::new(friction::SleepWorkaroundsRule),
Box::new(friction::SearchViaBashRule),
Box::new(friction::OutputParsingStruggleRule),
```

**After change (5 friction rules, 22 total):**
```rust
// Friction (5)
Box::new(friction::PermissionRetriesRule),
Box::new(friction::SleepWorkaroundsRule),
Box::new(friction::SearchViaBashRule),
Box::new(friction::OutputParsingStruggleRule),
Box::new(friction::ToolFailureRule),  // col-027: PostToolUseFailure per-tool count
```

**Doc comment update on `default_rules()`:**

Current:
```
/// Return the default set of detection rules (21 total).
```
and module-level:
```
//! Ships 21 rules across 4 categories: agent (7), friction (4), session (5), scope (5).
```

Update both to 22 total and friction count to 5:
```
/// Return the default set of detection rules (22 total).
```
```
//! Ships 22 rules across 4 categories: agent (7), friction (5), session (5), scope (5).
```

---

## Changes in metrics.rs

### Change: compute_universal() — widen terminal bucket to include PostToolUseFailure

**Current algorithm (lines 66-80 in existing file):**
```rust
let mut pre_counts: HashMap<&str, u64> = HashMap::new();
let mut post_counts: HashMap<&str, u64> = HashMap::new();
for r in &records {
    if let Some(tool) = &r.tool {
        if r.event_type == hook_type::PRETOOLUSE {
            *pre_counts.entry(tool).or_default() += 1;
        } else if r.event_type == hook_type::POSTTOOLUSE {
            *post_counts.entry(tool).or_default() += 1;
        }
    }
}
m.permission_friction_events = pre_counts
    .iter()
    .map(|(tool, &pre)| pre.saturating_sub(*post_counts.get(tool).unwrap_or(&0)))
    .sum();
```

**Fixed algorithm:**

```rust
let mut pre_counts: HashMap<&str, u64> = HashMap::new();
// Widened to count both PostToolUse and PostToolUseFailure as terminal events (col-027)
// Variable renamed from post_counts to terminal_counts for clarity (matches friction.rs rename)
let mut terminal_counts: HashMap<&str, u64> = HashMap::new();
for r in &records {
    if let Some(tool) = &r.tool {
        if r.event_type == hook_type::PRETOOLUSE {
            *pre_counts.entry(tool).or_default() += 1;
        } else if r.event_type == hook_type::POSTTOOLUSE {
            // unchanged: PostToolUse is a terminal event
            *terminal_counts.entry(tool.as_str()).or_default() += 1;
        } else if r.event_type == hook_type::POSTTOOLUSEFAILURE {
            // NEW: PostToolUseFailure is also a terminal event (col-027)
            *terminal_counts.entry(tool.as_str()).or_default() += 1;
        }
    }
}
// Formula: sum of max(pre - (post + failure), 0) per tool
// saturating_sub prevents negative values (R-12)
m.permission_friction_events = pre_counts
    .iter()
    .map(|(tool, &pre)| pre.saturating_sub(*terminal_counts.get(*tool).unwrap_or(&0)))
    .sum();
```

**Note on key type:** The existing `post_counts` uses `HashMap<&str, u64>` with the key being
`tool` (a `&str` borrowed from `r.tool.as_str()`). The renamed `terminal_counts` uses the same
type. The borrow structure is unchanged — `tool.as_str()` for PostToolUseFailure events follows
the same pattern.

**What does NOT change:**
- `permission_friction_events` field name (FR-06.3)
- All other metric computations in `compute_universal()`
- `source_domain == "claude-code"` pre-filter (already applied at line 40)
- `saturating_sub` arithmetic (prevents R-12 underflow)

---

## Initialization Sequence

No constructors or lifecycle management. All three changes are to pure functions:
- `PermissionRetriesRule::detect()` — pure function over `&[ObservationRecord]`
- `ToolFailureRule::detect()` — pure function over `&[ObservationRecord]`
- `compute_universal()` — pure function over `&[ObservationRecord]` and `&[HotspotFinding]`
- `default_rules()` — pure function returning `Vec<Box<dyn DetectionRule>>`

---

## Data Flow

```
ObservationRecord slice (from observations table, deserialized)
  |
  +--> PermissionRetriesRule::detect():
  |       Filter: source_domain == "claude-code"
  |       Accumulate: pre_counts[tool] += 1 for PreToolUse
  |                   terminal_counts[tool] += 1 for PostToolUse OR PostToolUseFailure
  |       Emit: HotspotFinding(rule_name="permission_retries") when retries > 2
  |
  +--> ToolFailureRule::detect():
  |       Filter: source_domain == "claude-code"
  |       Accumulate: failure_counts[tool] += 1 for PostToolUseFailure
  |       Emit: HotspotFinding(rule_name="tool_failure_hotspot") when count > 3
  |
  +--> compute_universal() -> UniversalMetrics:
          Filter: source_domain == "claude-code"
          Accumulate: pre_counts[tool] for PreToolUse
                      terminal_counts[tool] for PostToolUse OR PostToolUseFailure
          Compute: permission_friction_events = sum(pre - terminal, 0) per tool
```

---

## Error Handling

| Condition | Handling |
|-----------|----------|
| Empty observation set | All maps empty; `detect()` returns `vec![]`; `permission_friction_events = 0` |
| Records with `tool = None` | `ToolFailureRule`: `match record.tool.as_ref() { None => continue }` — skipped |
| Records from non-claude-code domains | Pre-filter `source_domain == "claude-code"` excludes them (R-07) |
| `failure_count > pre_count` in compute_universal | `saturating_sub` returns 0 — never negative (R-12) |
| `failure_count == TOOL_FAILURE_THRESHOLD` exactly | `*count > TOOL_FAILURE_THRESHOLD` is false — no finding (R-06 boundary) |
| Multiple tools exceeding threshold | One finding per tool (not aggregate) — loop emits one per tool |

---

## Key Test Scenarios

### T-FM-01: PermissionRetriesRule does not fire when PostToolUseFailure balances pre (AC-05, R-04)

```
test permission_retries_no_finding_when_failure_balances_pre:
  records = [
    make_pre(1, "Bash"), make_pre(2, "Bash"), make_pre(3, "Bash"),
    make_pre(4, "Bash"), make_pre(5, "Bash"),
    make_failure(6, "Bash"), make_failure(7, "Bash"), make_failure(8, "Bash"),
    make_failure(9, "Bash"), make_failure(10, "Bash"),
    // 5 Pre, 0 Post, 5 Failure -> retries = 5 - (0 + 5) = 0
  ]
  findings = PermissionRetriesRule.detect(&records)
  assert!(findings.is_empty())
```

### T-FM-02: PermissionRetriesRule still fires for genuine pre-with-no-terminal imbalance (AC-06, R-04)

```
test permission_retries_fires_when_genuine_imbalance:
  records = [
    make_pre(1, "Bash"), make_pre(2, "Bash"), make_pre(3, "Bash"),
    make_pre(4, "Bash"), make_pre(5, "Bash"),
    make_post(6, "Bash"), make_post(7, "Bash"),
    // 5 Pre, 2 Post, 0 Failure -> retries = 5 - 2 = 3 > threshold (2)
  ]
  findings = PermissionRetriesRule.detect(&records)
  assert_eq!(findings.len(), 1)
  assert_eq!(findings[0].measured, 3.0)
```

### T-FM-03: PermissionRetriesRule fires for mixed-source imbalance (AC-05 + AC-06 combined)

```
test permission_retries_partial_failure_balance:
  records = [
    make_pre(1, "Bash"), make_pre(2, "Bash"), make_pre(3, "Bash"),
    make_pre(4, "Bash"), make_pre(5, "Bash"),
    make_post(6, "Bash"),
    make_failure(7, "Bash"),
    // 5 Pre, 1 Post, 1 Failure -> terminal = 2, retries = 3 > threshold
  ]
  findings = PermissionRetriesRule.detect(&records)
  assert_eq!(findings.len(), 1)
  assert_eq!(findings[0].measured, 3.0)
```

### T-FM-04: ToolFailureRule fires when count strictly exceeds threshold (AC-08, R-06)

```
test tool_failure_rule_fires_at_four:
  records = [
    make_failure(1, "Bash"), make_failure(2, "Bash"),
    make_failure(3, "Bash"), make_failure(4, "Bash"),
    // 4 failures > threshold (3) -> 1 finding
  ]
  findings = ToolFailureRule.detect(&records)
  assert_eq!(findings.len(), 1)
  assert_eq!(findings[0].rule_name, "tool_failure_hotspot")
  assert_eq!(findings[0].measured, 4.0)
  assert_eq!(findings[0].threshold, 3.0)
  assert!(findings[0].claim.contains("Bash"))
  assert!(findings[0].claim.contains("4"))
```

### T-FM-05: ToolFailureRule does NOT fire at exactly threshold (AC-09, R-06 boundary)

```
test tool_failure_rule_no_finding_at_threshold:
  records = [
    make_failure(1, "Read"), make_failure(2, "Read"), make_failure(3, "Read"),
    // 3 failures == threshold -> no finding (strictly greater than required)
  ]
  findings = ToolFailureRule.detect(&records)
  assert!(findings.is_empty())
```

### T-FM-06: ToolFailureRule excludes non-claude-code records (R-07)

```
test tool_failure_rule_source_domain_guard:
  // 5 failures but all from non-claude-code domain
  records = (1..=5).map(|ts| ObservationRecord {
    ts,
    event_type: "PostToolUseFailure".to_string(),
    source_domain: "sre".to_string(),  // not claude-code
    session_id: "sess-1".to_string(),
    tool: Some("Bash".to_string()),
    ...
  }).collect()
  findings = ToolFailureRule.detect(&records)
  assert!(findings.is_empty())

  // 4 from claude-code + 5 from sre -> only claude-code counted -> 4 > 3 -> 1 finding
  records_mixed = 4 claude-code failures + 5 sre failures for "Bash"
  findings_mixed = ToolFailureRule.detect(&records_mixed)
  assert_eq!(findings_mixed.len(), 1)
  assert_eq!(findings_mixed[0].measured, 4.0)
```

### T-FM-07: Two-site coherence — both sites agree on same observation set (R-02, ADR-004)

This test MUST exercise both `compute_universal()` and `PermissionRetriesRule::detect()` on
the same observation set in the same test function (not in separate test functions in separate files).

```
test two_site_differential_coherence:
  // Scenario 1: 4 Pre + 2 Post + 2 Failure -> both sites agree: 0 imbalance
  records = [
    make_pre(1, "Bash"), make_pre(2, "Bash"), make_pre(3, "Bash"), make_pre(4, "Bash"),
    make_post(5, "Bash"), make_post(6, "Bash"),
    make_failure(7, "Bash"), make_failure(8, "Bash"),
  ]
  // Both sites must agree: pre(4) - terminal(2+2) = 0
  metrics = compute_universal(&records, &[])
  assert_eq!(metrics.permission_friction_events, 0)
  friction_findings = PermissionRetriesRule.detect(&records)
  assert!(friction_findings.is_empty())   // retries = 0 <= threshold(2)

  // Scenario 2: 5 Pre + 2 Post + 1 Failure -> both sites agree: 2 imbalance
  records2 = [
    make_pre(1, "Read"), make_pre(2, "Read"), make_pre(3, "Read"),
    make_pre(4, "Read"), make_pre(5, "Read"),
    make_post(6, "Read"), make_post(7, "Read"),
    make_failure(8, "Read"),
  ]
  // pre(5) - terminal(2+1) = 2
  metrics2 = compute_universal(&records2, &[])
  assert_eq!(metrics2.permission_friction_events, 2)
  friction_findings2 = PermissionRetriesRule.detect(&records2)
  // retries = 2, threshold = 2 -> 2 > 2 is false -> no finding
  assert!(friction_findings2.is_empty())
```

### T-FM-08: default_rules() contains ToolFailureRule, total count is 22 (R-13, FR-07.6)

```
test default_rules_contains_tool_failure_rule:
  rules = default_rules(None)
  // Count must be 22 (from 21 + 1)
  assert_eq!(rules.len(), 22)
  // ToolFailureRule must be present by name
  let names: Vec<&str> = rules.iter().map(|r| r.name()).collect()
  assert!(names.contains(&"tool_failure_hotspot"))
```

### T-FM-09: PermissionRetriesRule existing tests pass unchanged (AC-06 regression guard)

All existing `PermissionRetriesRule` tests (`test_permission_retries_exceeds_threshold`,
`test_permission_retries_at_threshold`, etc.) must pass without modification to their fixture data.
The `terminal_counts` rename is internal; the external behavior for records with only `PostToolUse`
events is unchanged.

Verification: run `cargo test -p unimatrix-observe detection::friction` before and after the change.
All tests that passed before must still pass.

### T-FM-10: compute_universal saturating_sub prevents negative metric (R-12)

```
test compute_universal_no_negative_friction_events:
  // Edge case: 1 Pre, 0 Post, 5 Failure (pathological: more failures than pre-events)
  records = [
    make_pre(1, "Bash"),
    make_failure(2, "Bash"), make_failure(3, "Bash"), make_failure(4, "Bash"),
    make_failure(5, "Bash"), make_failure(6, "Bash"),
  ]
  metrics = compute_universal(&records, &[])
  // saturating_sub: 1 - min(5, 1) should not go negative
  assert!(metrics.permission_friction_events == 0)
```

### T-FM-11: ToolFailureRule multiple tools, only exceeding threshold fires (R-06)

```
test tool_failure_rule_multiple_tools_selective:
  records = [
    make_failure(1, "Read"), make_failure(2, "Read"), make_failure(3, "Read"),  // 3 = threshold, no finding
    make_failure(4, "Bash"), make_failure(5, "Bash"), make_failure(6, "Bash"), make_failure(7, "Bash"),  // 4 > threshold, finding
    make_failure(8, "Write"), make_failure(9, "Write"),  // 2 < threshold, no finding
  ]
  findings = ToolFailureRule.detect(&records)
  assert_eq!(findings.len(), 1)
  assert_eq!(findings[0].tool_name_in_claim, "Bash")  // only Bash exceeds
  assert!(findings[0].claim.contains("'Bash'"))
  assert_eq!(findings[0].measured, 4.0)
```

---

## Anti-Patterns to Avoid

- Do NOT update `friction.rs` without also updating `metrics.rs` in the same commit (ADR-004)
- Do NOT update `metrics.rs` without also updating `friction.rs` in the same commit (ADR-004)
- Do NOT fire `ToolFailureRule` at count == 3 — threshold is strictly greater than (ADR-005, R-06)
- Do NOT rename `PermissionRetriesRule` rule_name, category, severity, or claim text (deferred col-028)
- Do NOT add source_domain filtering for records other than `"claude-code"` (R-07 is about INCLUDING non-claude-code, not excluding from all)
- Do NOT modify existing test fixture data in `PermissionRetriesRule` tests (AC-06 regression constraint)
- Do NOT create a `ToolFailureRule` finding for tools with `tool = None` (skip None-tool records)
- Do NOT forget to add `use unimatrix_core::observation::hook_type;` import to friction.rs if not present

## Knowledge Stewardship

- Queried: /uni-query-patterns for PostToolUseFailure hook event type dispatch patterns -- no direct pattern results; ADR files are the authoritative source
- Queried: /uni-query-patterns for col-027 architectural decisions -- no direct results; fell back to ADR files in product/features/col-027/architecture/
- Deviations from established patterns: none — all changes follow existing patterns (string constants over enum, explicit match arms, fire-and-forget RecordEvent, make_pre/make_post test helper pattern, sibling-function extractor pattern)
