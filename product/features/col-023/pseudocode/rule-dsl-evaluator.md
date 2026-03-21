# Pseudocode: rule-dsl-evaluator

**Wave**: 2 (parallel with domain-pack-registry and config-extension)
**Crate**: `unimatrix-observe`
**File**: `crates/unimatrix-observe/src/domain/mod.rs` (same file as domain-pack-registry)

## Purpose

Defines `RuleDescriptor` (the TOML-deserialized rule spec), `ThresholdRule`,
`TemporalWindowRule`, and `RuleEvaluator` (which implements `DetectionRule`).
This is the data-driven rule evaluation engine for external domain packs.
Built-in "claude-code" rules are NOT converted to DSL — they remain as Rust impls.

## Types

### RuleDescriptor enum

```
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleDescriptor:
    Threshold(ThresholdRule)
    TemporalWindow(TemporalWindowRule)
```

The `#[serde(tag = "kind")]` enables TOML deserialization where `kind = "threshold"` or
`kind = "temporal_window"` selects the variant.

### ThresholdRule

```
#[derive(Debug, Clone, Deserialize)]
pub struct ThresholdRule:
    pub name: String
    pub source_domain: String          -- REQUIRED; validated at load time
    pub event_type_filter: Vec<String> -- empty = all event types for this domain
    pub field_path: String             -- json_pointer; empty = count events
    pub threshold: f64
    pub severity: String
    pub claim_template: String
```

### TemporalWindowRule

```
#[derive(Debug, Clone, Deserialize)]
pub struct TemporalWindowRule:
    pub name: String
    pub source_domain: String          -- REQUIRED; validated at load time
    pub event_type_filter: Vec<String> -- empty = all event types for this domain
    pub window_secs: u64               -- must be > 0; validated at load time
    pub threshold: f64
    pub severity: String
    pub claim_template: String
```

## Startup Validation

### validate_rule_descriptor(descriptor: &RuleDescriptor, pack_domain: &str) -> Result<(), ObserveError>

Called from `DomainPackRegistry::new()` for each rule in each external pack:

```
fn validate_rule_descriptor(descriptor: &RuleDescriptor, pack_domain: &str) -> Result<(), ObserveError>:
    match descriptor:
        RuleDescriptor::Threshold(rule) =>
            -- source_domain must match pack's domain
            if rule.source_domain != pack_domain:
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: format!(
                        "rule source_domain '{}' does not match pack source_domain '{}'",
                        rule.source_domain, pack_domain
                    ),
                })
            -- source_domain must not be empty or "unknown"
            if rule.source_domain.is_empty() || rule.source_domain == "unknown":
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: "source_domain must be non-empty and not 'unknown'".to_string(),
                })
            -- field_path may be empty (count-based) or a valid JSON Pointer
            -- JSON Pointer validity: must start with "" or "/" if non-empty
            if !rule.field_path.is_empty() && !rule.field_path.starts_with('/'):
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: "field_path must be empty or a valid JSON Pointer (starts with '/')".to_string(),
                })

        RuleDescriptor::TemporalWindow(rule) =>
            -- source_domain validation (same as Threshold)
            if rule.source_domain != pack_domain:
                return Err(ObserveError::InvalidRuleDescriptor { ... })
            if rule.source_domain.is_empty() || rule.source_domain == "unknown":
                return Err(ObserveError::InvalidRuleDescriptor { ... })
            -- window_secs = 0 is invalid (EC-08, Constraint 11)
            if rule.window_secs == 0:
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: "window_secs must be > 0".to_string(),
                })

    Ok(())
```

## RuleEvaluator

```
pub struct RuleEvaluator:
    descriptor: RuleDescriptor
```

```
impl RuleEvaluator:
    pub fn new(descriptor: RuleDescriptor) -> Self:
        RuleEvaluator { descriptor }
```

### DetectionRule implementation for RuleEvaluator

```
impl DetectionRule for RuleEvaluator:
    fn name(&self) -> &str:
        match &self.descriptor:
            RuleDescriptor::Threshold(r) => &r.name
            RuleDescriptor::TemporalWindow(r) => &r.name

    fn category(&self) -> HotspotCategory:
        -- All DSL rules use HotspotCategory::Agent as the default category
        -- (DSL does not encode category; external domain packs use Agent)
        HotspotCategory::Agent

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
        match &self.descriptor:
            RuleDescriptor::Threshold(rule) => detect_threshold(rule, records)
            RuleDescriptor::TemporalWindow(rule) => detect_temporal_window(rule, records)
```

### detect_threshold(rule: &ThresholdRule, records: &[ObservationRecord]) -> Vec<HotspotFinding>

```
fn detect_threshold(rule: &ThresholdRule, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    -- STEP 1: source_domain guard (MANDATORY FIRST FILTER — ADR-005)
    let domain_records: Vec<&ObservationRecord> = records
        .iter()
        .filter(|r| r.source_domain == rule.source_domain)
        .collect()

    -- STEP 2: event_type filter
    let filtered: Vec<&ObservationRecord> = if rule.event_type_filter.is_empty():
        domain_records    -- empty filter = all event types for this domain
    else:
        domain_records.into_iter()
            .filter(|r| rule.event_type_filter.contains(&r.event_type))
            .collect()

    -- STEP 3: compute measured value
    let measured: f64 = if rule.field_path.is_empty():
        -- Count-based: count matching records
        filtered.len() as f64
    else:
        -- Field extraction: sum or count numeric values at field_path
        -- For each record, extract payload field at json_pointer
        -- Non-numeric values are silently skipped with a WARN log (R-08)
        let mut total = 0.0f64
        let mut extracted_count = 0u64
        for record in &filtered:
            if let Some(input) = &record.input:
                if let Some(val) = input.pointer(&rule.field_path):
                    if let Some(n) = val.as_f64():
                        total += n
                        extracted_count += 1
                    else:
                        log::warn!(
                            "rule '{}': field_path '{}' resolved to non-numeric value; skipping",
                            rule.name, rule.field_path
                        )
                -- Missing key: silently skip (no log for missing key — normal case)
        if extracted_count > 0: total
        else: 0.0

    -- STEP 4: threshold comparison
    if measured > rule.threshold:
        let claim = format_claim(&rule.claim_template, measured, None)
        vec![HotspotFinding {
            category: HotspotCategory::Agent,
            severity: parse_severity(&rule.severity),
            rule_name: rule.name.clone(),
            claim,
            measured,
            threshold: rule.threshold,
            evidence: vec![],
        }]
    else:
        vec![]
```

### detect_temporal_window(rule: &TemporalWindowRule, records: &[ObservationRecord]) -> Vec<HotspotFinding>

```
fn detect_temporal_window(rule: &TemporalWindowRule, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    -- STEP 1: source_domain guard (MANDATORY FIRST FILTER — ADR-005)
    let domain_records: Vec<&ObservationRecord> = records
        .iter()
        .filter(|r| r.source_domain == rule.source_domain)
        .collect()

    -- STEP 2: event_type filter
    let filtered: Vec<&ObservationRecord> = if rule.event_type_filter.is_empty():
        domain_records
    else:
        domain_records.into_iter()
            .filter(|r| rule.event_type_filter.contains(&r.event_type))
            .collect()

    if filtered.is_empty():
        return vec![]

    -- STEP 3: Sort by ts (MANDATORY — Constraint 12, R-07)
    let mut sorted: Vec<&ObservationRecord> = filtered
    sorted.sort_by_key(|r| r.ts)

    -- STEP 4: Two-pointer sliding window max-count
    let window_ms: u64 = rule.window_secs * 1000
    let mut max_in_window: u64 = 0
    let mut left = 0usize

    for right in 0..sorted.len():
        -- Advance left pointer to shrink window to window_ms
        while sorted[right].ts.saturating_sub(sorted[left].ts) > window_ms:
            left += 1
        let count_in_window = (right - left + 1) as u64
        if count_in_window > max_in_window:
            max_in_window = count_in_window

    let measured = max_in_window as f64

    -- STEP 5: threshold comparison
    if measured > rule.threshold:
        let claim = format_claim(&rule.claim_template, measured, Some(rule.window_secs))
        vec![HotspotFinding {
            category: HotspotCategory::Agent,
            severity: parse_severity(&rule.severity),
            rule_name: rule.name.clone(),
            claim,
            measured,
            threshold: rule.threshold,
            evidence: vec![],
        }]
    else:
        vec![]
```

## Helper Functions

### format_claim(template: &str, measured: f64, window_secs: Option<u64>) -> String

```
fn format_claim(template: &str, measured: f64, window_secs: Option<u64>) -> String:
    let mut s = template.replace("{measured}", &format!("{measured:.1}"))
    if let Some(w) = window_secs:
        s = s.replace("{window_secs}", &w.to_string())
    s
```

### parse_severity(s: &str) -> Severity

```
fn parse_severity(s: &str) -> Severity:
    match s.to_lowercase().as_str():
        "critical" | "error" => Severity::Critical
        "warning" | "warn"   => Severity::Warning
        _                    => Severity::Info
```

If `Severity` does not have a `Critical` variant, use `Warning` for "critical"/"error".
Check the existing `Severity` enum in `unimatrix-observe/src/types.rs` and match
its actual variants.

## Error Handling

- `validate_rule_descriptor()` returns `Err(ObserveError::InvalidRuleDescriptor)` for
  malformed rules. This is called at startup — errors cause server startup failure (FM-01).
- `detect_threshold()` and `detect_temporal_window()` are infallible at runtime.
  Non-numeric `field_path` values log a WARN and are skipped (R-08).
- Empty input slices: both detect functions return `vec![]` without panicking (EC-06).

## Key Test Scenarios

1. **Threshold rule fires**: N+1 matching records > threshold; assert one finding with
   correct `measured`, `rule_name`, and `claim`.

2. **Threshold rule silent at threshold**: exactly N records; assert no finding.

3. **Threshold count-based (empty field_path)**: counts all matching event_type records.

4. **Threshold with field_path**: numeric extraction from payload; `field_path = "/count"`.

5. **Threshold field_path non-numeric (R-08)**: field resolves to a string; assert no
   finding and a WARN log is emitted.

6. **Threshold field_path missing key (R-08)**: key absent from payload; assert no finding,
   no panic.

7. **Temporal window fires with sorted input**: N+1 events within window; assert finding.

8. **Temporal window fires with unsorted input (R-07)**: same records in reverse ts order;
   assert same finding (sort step produces correct result).

9. **Temporal window at boundary**: exactly N within window → no finding;
   N+1 within window → one finding.

10. **Temporal window rule with window_secs=0 rejected at startup (EC-08)**: returns
    `Err(InvalidRuleDescriptor)`.

11. **Rule source_domain mismatch (EC-09)**: rule with `source_domain = "claude-code"`
    in an "sre" pack → `Err(InvalidRuleDescriptor)` at startup.

12. **source_domain guard isolation**: supply mixed `"sre"` + `"unknown"` records to
    a `"sre"` RuleEvaluator threshold rule; only `"sre"` records are counted (R-01).

13. **Empty records**: both rule types return `vec![]` without panicking (EC-06).
