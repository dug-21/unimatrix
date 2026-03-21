//! Data-driven rule DSL: `RuleDescriptor`, `ThresholdRule`, `TemporalWindowRule`,
//! and `RuleEvaluator` (which implements `DetectionRule`).
//!
//! Built-in "claude-code" rules remain as Rust `DetectionRule` impls in
//! `detection/`. This module only covers external domain pack rules (ADR-003).

use serde::{Deserialize, Serialize};

use crate::detection::DetectionRule;
use crate::error::ObserveError;
use crate::types::{HotspotCategory, HotspotFinding, ObservationRecord, Severity};

// ── RuleDescriptor ────────────────────────────────────────────────────────────

/// Data-driven rule specification for external domain packs.
///
/// Two rule kinds are supported:
/// - `threshold`: count of matching records > threshold
/// - `temporal_window`: max events within T seconds > threshold
///
/// For TOML deserialization, use `kind = "threshold"` or `kind = "temporal_window"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleDescriptor {
    Threshold(ThresholdRule),
    TemporalWindow(TemporalWindowRule),
}

/// Count-based threshold rule.
///
/// Fires when count (or field-extracted numeric sum) of matching records exceeds
/// `threshold`. `field_path` is a JSON Pointer; empty means count events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdRule {
    /// Unique rule name within the pack.
    pub name: String,
    /// Required: must match the owning pack's `source_domain`. Validated at startup.
    pub source_domain: String,
    /// Event type filter. Empty = all event types for this domain.
    pub event_type_filter: Vec<String>,
    /// JSON Pointer into the record's `input` payload. Empty = count events.
    pub field_path: String,
    /// Threshold to exceed (strictly greater than).
    pub threshold: f64,
    /// Severity string: "critical"/"error" → `Severity::Critical`,
    /// "warning"/"warn" → `Severity::Warning`, anything else → `Severity::Info`.
    pub severity: String,
    /// Claim template. `{measured}` is replaced with the measured value.
    pub claim_template: String,
}

/// Sliding-window temporal rule.
///
/// Fires when the maximum count of matching events within any `window_secs`-second
/// window exceeds `threshold`. The `detect()` implementation sorts records by `ts`
/// before the two-pointer scan (ADR-003, Constraint 12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalWindowRule {
    /// Unique rule name within the pack.
    pub name: String,
    /// Required: must match the owning pack's `source_domain`. Validated at startup.
    pub source_domain: String,
    /// Event type filter. Empty = all event types for this domain.
    pub event_type_filter: Vec<String>,
    /// Rolling window width in seconds. Must be > 0; validated at startup.
    pub window_secs: u64,
    /// Threshold to exceed (strictly greater than).
    pub threshold: f64,
    /// Severity string: see `ThresholdRule.severity`.
    pub severity: String,
    /// Claim template. `{measured}` and `{window_secs}` are replaced.
    pub claim_template: String,
}

// ── RuleEvaluator ─────────────────────────────────────────────────────────────

/// Data-driven rule evaluator that implements `DetectionRule`.
///
/// Created by `DomainPackRegistry::rules_for_domain()` — one evaluator per
/// `RuleDescriptor` in the registered pack.
#[derive(Debug)]
pub struct RuleEvaluator {
    descriptor: RuleDescriptor,
}

impl RuleEvaluator {
    /// Wrap a `RuleDescriptor` in an evaluator.
    ///
    /// No validation here — descriptors must be validated at pack registration time
    /// via `DomainPackRegistry::new()`.
    pub fn new(descriptor: RuleDescriptor) -> Self {
        RuleEvaluator { descriptor }
    }
}

impl DetectionRule for RuleEvaluator {
    fn name(&self) -> &str {
        match &self.descriptor {
            RuleDescriptor::Threshold(r) => &r.name,
            RuleDescriptor::TemporalWindow(r) => &r.name,
        }
    }

    fn category(&self) -> HotspotCategory {
        // DSL rules do not encode category; external domain packs default to Agent.
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        match &self.descriptor {
            RuleDescriptor::Threshold(rule) => detect_threshold(rule, records),
            RuleDescriptor::TemporalWindow(rule) => detect_temporal_window(rule, records),
        }
    }
}

// ── Detection functions ───────────────────────────────────────────────────────

/// Threshold detection logic for `ThresholdRule`.
///
/// Step order:
/// 1. source_domain guard (ADR-005 mandatory first filter)
/// 2. event_type filter
/// 3. Count events or extract numeric field via JSON Pointer
/// 4. Compare against threshold; emit finding if exceeded
pub(crate) fn detect_threshold(
    rule: &ThresholdRule,
    records: &[ObservationRecord],
) -> Vec<HotspotFinding> {
    // STEP 1: source_domain guard — mandatory first filter (ADR-005)
    let domain_records: Vec<&ObservationRecord> = records
        .iter()
        .filter(|r| r.source_domain == rule.source_domain)
        .collect();

    // STEP 2: event_type filter (empty = all event types for this domain)
    let filtered: Vec<&ObservationRecord> = if rule.event_type_filter.is_empty() {
        domain_records
    } else {
        domain_records
            .into_iter()
            .filter(|r| rule.event_type_filter.contains(&r.event_type))
            .collect()
    };

    // STEP 3: compute measured value
    let measured: f64 = if rule.field_path.is_empty() {
        // Count-based: count all matching records
        filtered.len() as f64
    } else {
        // Field extraction: sum numeric values at field_path via JSON Pointer (R-08)
        let mut total = 0.0f64;
        let mut extracted_count = 0u64;
        for record in &filtered {
            if let Some(input) = &record.input {
                if let Some(val) = input.pointer(&rule.field_path) {
                    if let Some(n) = val.as_f64() {
                        total += n;
                        extracted_count += 1;
                    }
                    // Non-numeric field: silently skip (R-08).
                    // Note: a WARN-level log would be emitted here once tracing is
                    // added to unimatrix-observe's dependencies.
                }
                // Missing key: silently skip (normal case — not every record has every field)
            }
        }
        if extracted_count > 0 { total } else { 0.0 }
    };

    // STEP 4: threshold comparison (strictly greater than)
    if measured > rule.threshold {
        let claim = format_claim(&rule.claim_template, measured, None);
        vec![HotspotFinding {
            category: HotspotCategory::Agent,
            severity: parse_severity(&rule.severity),
            rule_name: rule.name.clone(),
            claim,
            measured,
            threshold: rule.threshold,
            evidence: vec![],
        }]
    } else {
        vec![]
    }
}

/// Temporal window detection logic for `TemporalWindowRule`.
///
/// Step order:
/// 1. source_domain guard (ADR-005 mandatory first filter)
/// 2. event_type filter
/// 3. Sort by ts (Constraint 12 — sort is mandatory before two-pointer scan)
/// 4. Two-pointer sliding window: find maximum count within window_secs
/// 5. Compare against threshold; emit finding if exceeded
pub(crate) fn detect_temporal_window(
    rule: &TemporalWindowRule,
    records: &[ObservationRecord],
) -> Vec<HotspotFinding> {
    // STEP 1: source_domain guard — mandatory first filter (ADR-005)
    let domain_records: Vec<&ObservationRecord> = records
        .iter()
        .filter(|r| r.source_domain == rule.source_domain)
        .collect();

    // STEP 2: event_type filter (empty = all event types for this domain)
    let filtered: Vec<&ObservationRecord> = if rule.event_type_filter.is_empty() {
        domain_records
    } else {
        domain_records
            .into_iter()
            .filter(|r| rule.event_type_filter.contains(&r.event_type))
            .collect()
    };

    if filtered.is_empty() {
        return vec![];
    }

    // STEP 3: sort by ts — mandatory before two-pointer scan (Constraint 12, R-07)
    let mut sorted: Vec<&ObservationRecord> = filtered;
    sorted.sort_by_key(|r| r.ts);

    // STEP 4: two-pointer sliding window max-count
    let window_ms: u64 = rule.window_secs * 1000;
    let mut max_in_window: u64 = 0;
    let mut left = 0usize;

    for right in 0..sorted.len() {
        // Advance left pointer until window fits within window_ms
        while sorted[right].ts.saturating_sub(sorted[left].ts) > window_ms {
            left += 1;
        }
        let count_in_window = (right - left + 1) as u64;
        if count_in_window > max_in_window {
            max_in_window = count_in_window;
        }
    }

    let measured = max_in_window as f64;

    // STEP 5: threshold comparison (strictly greater than)
    if measured > rule.threshold {
        let claim = format_claim(&rule.claim_template, measured, Some(rule.window_secs));
        vec![HotspotFinding {
            category: HotspotCategory::Agent,
            severity: parse_severity(&rule.severity),
            rule_name: rule.name.clone(),
            claim,
            measured,
            threshold: rule.threshold,
            evidence: vec![],
        }]
    } else {
        vec![]
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Replace `{measured}` and `{window_secs}` placeholders in a claim template.
pub(crate) fn format_claim(template: &str, measured: f64, window_secs: Option<u64>) -> String {
    let mut s = template.replace("{measured}", &format!("{measured:.1}"));
    if let Some(w) = window_secs {
        s = s.replace("{window_secs}", &w.to_string());
    }
    s
}

/// Map a severity string to the `Severity` enum.
///
/// - "critical" or "error" → `Severity::Critical`
/// - "warning" or "warn" → `Severity::Warning`
/// - anything else → `Severity::Info`
pub(crate) fn parse_severity(s: &str) -> Severity {
    match s.to_lowercase().as_str() {
        "critical" | "error" => Severity::Critical,
        "warning" | "warn" => Severity::Warning,
        _ => Severity::Info,
    }
}

/// Validate a single `RuleDescriptor` against its owning pack's `source_domain`.
///
/// Called from `DomainPackRegistry::new()` for each rule in each pack.
/// Returns startup-fatal error (FM-01) on any validation failure.
pub(crate) fn validate_rule_descriptor(
    descriptor: &RuleDescriptor,
    pack_domain: &str,
) -> Result<(), ObserveError> {
    match descriptor {
        RuleDescriptor::Threshold(rule) => {
            // source_domain must match the pack's domain.
            if rule.source_domain != pack_domain {
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: format!(
                        "rule source_domain '{}' does not match pack source_domain '{}'",
                        rule.source_domain, pack_domain
                    ),
                });
            }
            // source_domain must not be empty or "unknown".
            if rule.source_domain.is_empty() || rule.source_domain == "unknown" {
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: "source_domain must be non-empty and not 'unknown'".to_string(),
                });
            }
            // field_path: if non-empty, must be a valid JSON Pointer (starts with '/').
            if !rule.field_path.is_empty() && !rule.field_path.starts_with('/') {
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: "field_path must be empty or a valid JSON Pointer (starts with '/')"
                        .to_string(),
                });
            }
        }
        RuleDescriptor::TemporalWindow(rule) => {
            // source_domain must match the pack's domain.
            if rule.source_domain != pack_domain {
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: format!(
                        "rule source_domain '{}' does not match pack source_domain '{}'",
                        rule.source_domain, pack_domain
                    ),
                });
            }
            // source_domain must not be empty or "unknown".
            if rule.source_domain.is_empty() || rule.source_domain == "unknown" {
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: "source_domain must be non-empty and not 'unknown'".to_string(),
                });
            }
            // window_secs = 0 is invalid (EC-08, Constraint 11).
            if rule.window_secs == 0 {
                return Err(ObserveError::InvalidRuleDescriptor {
                    rule_name: rule.name.clone(),
                    reason: "window_secs must be > 0".to_string(),
                });
            }
        }
    }
    Ok(())
}
