//! Category allowlist: runtime-extensible validation of entry categories.

mod lifecycle; // reserved stub for #409

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::sync::RwLock;

use crate::error::ServerError;

pub(crate) const INITIAL_CATEGORIES: [&str; 5] = [
    "lesson-learned",
    "decision",
    "convention",
    "pattern",
    "procedure",
];

/// Runtime-extensible category validation with per-category lifecycle policy.
///
/// Two independent RwLock fields:
/// - `categories`: the presence set; consulted by `validate()` on every context_store.
/// - `adaptive`:   the lifecycle policy set; consulted by `is_adaptive()` in the tick
///                 and by `compute_report()` for status output.
///
/// Categories added at runtime via `add_category` are always pinned.
/// Only categories in `adaptive_categories` of the operator config are adaptive,
/// and this set is frozen after construction (no runtime mutation of `adaptive`).
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,
    adaptive: RwLock<HashSet<String>>,
}

impl CategoryAllowlist {
    /// Canonical constructor. All field initialization lives here.
    ///
    /// `cats` populates the validation set; `adaptive` populates the lifecycle
    /// policy set. No validation is performed — caller is responsible
    /// (`validate_config` runs before construction at startup).
    pub fn from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self {
        let categories_set: HashSet<String> = cats.into_iter().collect();
        let adaptive_set: HashSet<String> = adaptive.into_iter().collect();
        CategoryAllowlist {
            categories: RwLock::new(categories_set),
            adaptive: RwLock::new(adaptive_set),
        }
    }

    /// Create a new allowlist seeded from the supplied category list.
    ///
    /// Called from `main.rs` startup wiring after config load:
    ///   `CategoryAllowlist::from_categories(config.knowledge.categories)`
    ///
    /// The supplied list is assumed to have already been validated by
    /// `validate_config` — no re-validation is performed here.
    ///
    /// Delegates to `from_categories_with_policy` with `["lesson-learned"]` as
    /// the default adaptive set.
    pub fn from_categories(cats: Vec<String>) -> Self {
        CategoryAllowlist::from_categories_with_policy(cats, vec!["lesson-learned".to_string()])
    }

    /// Create a new allowlist with the initial 5 categories.
    ///
    /// Unchanged signature — all existing call sites remain valid.
    /// Delegates to `from_categories` to keep a single implementation path.
    pub fn new() -> Self {
        CategoryAllowlist::from_categories(
            INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        )
    }

    /// Validate a category string against the allowlist.
    pub fn validate(&self, category: &str) -> Result<(), ServerError> {
        let cats = self.categories.read().unwrap_or_else(|e| e.into_inner());
        if cats.contains(category) {
            Ok(())
        } else {
            let mut valid: Vec<String> = cats.iter().cloned().collect();
            valid.sort();
            Err(ServerError::InvalidCategory {
                category: category.to_string(),
                valid_categories: valid,
            })
        }
    }

    /// Add a new category to the allowlist at runtime.
    ///
    /// Domain pack categories registered at runtime are always pinned.
    /// Lifecycle policy (adaptive/pinned) is config-only and frozen after startup.
    /// Adding a category here never adds it to the adaptive set.
    ///
    /// To make a domain pack category adaptive, add it to `adaptive_categories`
    /// in config.toml before startup.
    pub fn add_category(&self, category: String) {
        let mut cats = self.categories.write().unwrap_or_else(|e| e.into_inner());
        cats.insert(category);
    }

    /// List all valid categories (sorted alphabetically).
    pub fn list_categories(&self) -> Vec<String> {
        let cats = self.categories.read().unwrap_or_else(|e| e.into_inner());
        let mut list: Vec<String> = cats.iter().cloned().collect();
        list.sort();
        list
    }

    /// Returns `true` if `category` is classified as adaptive (eligible for automated management).
    ///
    /// Reads only the `adaptive` lock — no contention on the hot `categories` path.
    /// Returns `false` for unknown categories (not in allowlist AND not adaptive).
    pub fn is_adaptive(&self, category: &str) -> bool {
        let guard = self.adaptive.read().unwrap_or_else(|e| e.into_inner());
        guard.contains(category)
    }

    /// Returns a sorted list of all categories in the adaptive set.
    ///
    /// Used by the maintenance tick (Step 10b) — acquired once per tick, not
    /// per-category. Callers must not call `list_adaptive` inside a loop and then
    /// call `is_adaptive` per item; use `list_adaptive` once instead (R-06).
    pub fn list_adaptive(&self) -> Vec<String> {
        let guard = self.adaptive.read().unwrap_or_else(|e| e.into_inner());
        let mut list: Vec<String> = guard.iter().cloned().collect();
        list.sort();
        list
    }
}
