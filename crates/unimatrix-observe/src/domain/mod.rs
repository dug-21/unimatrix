//! Domain pack registry and rule DSL evaluator (col-023 ADR-002, ADR-003).
//!
//! `DomainPackRegistry` is initialized at server startup from TOML config and
//! threaded as `Arc` into `SqlObservationSource`. The "claude-code" built-in pack
//! is always present and requires no config.
//!
//! External domain packs are declared via `[[observation.domain_packs]]` TOML stanzas.
//! DSL rule evaluation is in `evaluator.rs`.

pub mod evaluator;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub use evaluator::{RuleDescriptor, RuleEvaluator, TemporalWindowRule, ThresholdRule};

use crate::detection::DetectionRule;
use crate::error::ObserveError;

// ── DomainPack ────────────────────────────────────────────────────────────────

/// A domain pack declares the event types, knowledge categories, and DSL rules
/// for a specific source domain (e.g., "claude-code", "sre").
///
/// The "claude-code" pack is always built-in. Additional packs are loaded from
/// TOML config at startup (`[observation]` section, ADR-002).
#[derive(Debug, Clone)]
pub struct DomainPack {
    /// Unique identifier for this domain. Must match `^[a-z0-9_-]{1,64}$`.
    /// The value `"unknown"` is reserved and cannot be registered.
    pub source_domain: String,
    /// Event type strings this domain recognizes (e.g., "PreToolUse").
    /// Empty = this domain claims all event types (use with care).
    pub event_types: Vec<String>,
    /// Knowledge categories this domain contributes to `CategoryAllowlist`.
    pub categories: Vec<String>,
    /// Data-driven DSL rule descriptors. Built-in Rust rules are NOT listed here.
    pub rules: Vec<RuleDescriptor>,
}

/// Returns the built-in claude-code domain pack.
///
/// This pack is always loaded first; it cannot be absent. TOML config may
/// override it by specifying `source_domain = "claude-code"`.
fn builtin_claude_code_pack() -> DomainPack {
    DomainPack {
        source_domain: "claude-code".to_string(),
        event_types: vec![
            "PreToolUse".to_string(),
            "PostToolUse".to_string(),
            "SubagentStart".to_string(),
            "SubagentStop".to_string(),
        ],
        // All 5 active INITIAL_CATEGORIES from CategoryAllowlist (C-10).
        categories: vec![
            "outcome".to_string(),
            "lesson-learned".to_string(),
            "decision".to_string(),
            "convention".to_string(),
            "pattern".to_string(),
            "procedure".to_string(),
        ],
        // Built-in claude-code detection rules are Rust impls, not DSL descriptors.
        rules: vec![],
    }
}

// ── DomainPackRegistry ────────────────────────────────────────────────────────

/// Registry of domain packs, initialized at server startup.
///
/// The registry is `Clone`-able (inner state is `Arc`-wrapped) and is threaded
/// as `Arc` into `SqlObservationSource`. No public write methods exist after
/// construction (ADR-002, AC-08 — no runtime re-registration).
#[derive(Debug, Clone)]
pub struct DomainPackRegistry {
    inner: Arc<RwLock<HashMap<String, DomainPack>>>,
}

impl DomainPackRegistry {
    /// Construct a registry seeded with the built-in claude-code pack plus any
    /// caller-supplied packs.
    ///
    /// Validation enforced at construction time:
    /// - `"unknown"` is rejected as a `source_domain` (reserved, EC-04).
    /// - `source_domain` must match `^[a-z0-9_-]{1,64}$` (AC-07).
    /// - Every `RuleDescriptor` in each pack is validated (source_domain match,
    ///   `window_secs > 0`, `field_path` JSON Pointer syntax).
    ///
    /// Returns `Err` on any validation failure — the server must not start with
    /// an invalid domain pack (FM-01).
    pub fn new(packs: Vec<DomainPack>) -> Result<Self, ObserveError> {
        let mut map: HashMap<String, DomainPack> = HashMap::new();
        // Built-in claude-code pack is always loaded first.
        map.insert("claude-code".to_string(), builtin_claude_code_pack());

        for pack in packs {
            // "unknown" is reserved — reject it (EC-04).
            if pack.source_domain == "unknown" {
                return Err(ObserveError::InvalidSourceDomain {
                    domain: "unknown".to_string(),
                });
            }

            // Validate source_domain format: ^[a-z0-9_-]{1,64}$ (AC-07)
            if !validate_source_domain_format(&pack.source_domain) {
                return Err(ObserveError::InvalidSourceDomain {
                    domain: pack.source_domain.clone(),
                });
            }

            // Validate all rule descriptors in this pack.
            for rule in &pack.rules {
                evaluator::validate_rule_descriptor(rule, &pack.source_domain)?;
            }

            // Insert — overrides built-in claude-code if source_domain == "claude-code".
            map.insert(pack.source_domain.clone(), pack);
        }

        Ok(DomainPackRegistry {
            inner: Arc::new(RwLock::new(map)),
        })
    }

    /// Construct a registry with only the built-in claude-code pack.
    ///
    /// Use at startup when no TOML `[observation]` config section is present.
    /// This is the zero-config path (ADR-002, AC-03).
    pub fn with_builtin_claude_code() -> Self {
        let mut map = HashMap::new();
        map.insert("claude-code".to_string(), builtin_claude_code_pack());
        DomainPackRegistry {
            inner: Arc::new(RwLock::new(map)),
        }
    }

    /// Look up a domain pack by `source_domain`.
    ///
    /// Acquires a read lock and clones the result to avoid holding the lock.
    /// Returns `None` if the domain is not registered.
    pub fn lookup(&self, source_domain: &str) -> Option<DomainPack> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        guard.get(source_domain).cloned()
    }

    /// Return `RuleEvaluator` instances for all DSL rules in the given domain's pack.
    ///
    /// Built-in claude-code Rust rules are NOT returned here — they live in
    /// `default_rules()`. This method only returns DSL evaluators for external packs.
    ///
    /// Returns an empty `Vec` if the domain is not registered or has no DSL rules.
    pub fn rules_for_domain(&self, source_domain: &str) -> Vec<Box<dyn DetectionRule>> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        match guard.get(source_domain) {
            None => vec![],
            Some(pack) => pack
                .rules
                .iter()
                .map(|d| Box::new(RuleEvaluator::new(d.clone())) as Box<dyn DetectionRule>)
                .collect(),
        }
    }

    /// Resolve a raw `event_type` string to its registered `source_domain`.
    ///
    /// Iterates all registered packs and returns the domain whose `event_types`
    /// list contains `event_type`. If no pack claims the event type, returns
    /// `"unknown"`.
    ///
    /// If `event_types` is empty for a domain, that domain claims ALL event types.
    ///
    /// # EC-07 note
    /// HashMap iteration order is non-deterministic. If two packs share an
    /// `event_type` string, the result is one of their domains (non-deterministic).
    /// This is acceptable in W1-5 because the hook ingress path always assigns
    /// `source_domain = "claude-code"` directly — this method is only called for
    /// records that do NOT come from the hook path.
    pub fn resolve_source_domain(&self, event_type: &str) -> String {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        for (domain, pack) in guard.iter() {
            if pack.event_types.is_empty() || pack.event_types.iter().any(|et| et == event_type) {
                return domain.clone();
            }
        }
        "unknown".to_string()
    }

    /// Iterate all registered domain packs.
    ///
    /// Used at server startup (Wave 4) to register each pack's `categories`
    /// into `CategoryAllowlist` via `CategoryAllowlist::from_categories()`.
    pub fn iter_packs(&self) -> Vec<DomainPack> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        guard.values().cloned().collect()
    }
}

// ── Validation helpers ────────────────────────────────────────────────────────

/// Validate `source_domain` format: `^[a-z0-9_-]{1,64}$`.
///
/// No regex crate needed — manual char check avoids a new dependency (ADR-003).
pub(crate) fn validate_source_domain_format(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 64 {
        return false;
    }
    domain
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}
