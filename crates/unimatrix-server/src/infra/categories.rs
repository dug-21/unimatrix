//! Category allowlist: runtime-extensible validation of entry categories.

use std::collections::HashSet;
use std::sync::RwLock;

use crate::error::ServerError;

const INITIAL_CATEGORIES: [&str; 8] = [
    "outcome",
    "lesson-learned",
    "decision",
    "convention",
    "pattern",
    "procedure",
    "duties",    // role duties for context_briefing
    "reference", // general reference material
];

/// Runtime-extensible category validation.
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,
}

impl CategoryAllowlist {
    /// Create a new allowlist with the initial 8 categories.
    pub fn new() -> Self {
        let mut set = HashSet::new();
        for cat in INITIAL_CATEGORIES {
            set.insert(cat.to_string());
        }
        CategoryAllowlist {
            categories: RwLock::new(set),
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_outcome() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("outcome").is_ok());
    }

    #[test]
    fn test_validate_lesson_learned() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("lesson-learned").is_ok());
    }

    #[test]
    fn test_validate_decision() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("decision").is_ok());
    }

    #[test]
    fn test_validate_convention() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("convention").is_ok());
    }

    #[test]
    fn test_validate_pattern() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("pattern").is_ok());
    }

    #[test]
    fn test_validate_procedure() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("procedure").is_ok());
    }

    #[test]
    fn test_validate_duties() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("duties").is_ok());
    }

    #[test]
    fn test_validate_reference() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("reference").is_ok());
    }

    #[test]
    fn test_validate_unknown_rejected() {
        let al = CategoryAllowlist::new();
        let err = al.validate("unknown").unwrap_err();
        match err {
            ServerError::InvalidCategory {
                category,
                valid_categories,
            } => {
                assert_eq!(category, "unknown");
                assert_eq!(valid_categories.len(), 8);
            }
            _ => panic!("expected InvalidCategory"),
        }
    }

    #[test]
    fn test_validate_case_sensitive() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("Convention").is_err());
    }

    #[test]
    fn test_validate_empty_string_rejected() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("").is_err());
    }

    #[test]
    fn test_add_category_then_validate() {
        let al = CategoryAllowlist::new();
        assert!(al.validate("custom").is_err());
        al.add_category("custom".to_string());
        assert!(al.validate("custom").is_ok());
    }

    #[test]
    fn test_list_categories_sorted() {
        let al = CategoryAllowlist::new();
        let list = al.list_categories();
        assert_eq!(list.len(), 8);
        // Verify sorted
        for i in 1..list.len() {
            assert!(list[i] >= list[i - 1]);
        }
    }

    #[test]
    fn test_error_lists_all_valid_categories() {
        let al = CategoryAllowlist::new();
        let err = al.validate("bogus").unwrap_err();
        match err {
            ServerError::InvalidCategory {
                valid_categories, ..
            } => {
                assert!(valid_categories.contains(&"convention".to_string()));
                assert!(valid_categories.contains(&"decision".to_string()));
                assert!(valid_categories.contains(&"duties".to_string()));
                assert!(valid_categories.contains(&"lesson-learned".to_string()));
                assert!(valid_categories.contains(&"outcome".to_string()));
                assert!(valid_categories.contains(&"pattern".to_string()));
                assert!(valid_categories.contains(&"procedure".to_string()));
                assert!(valid_categories.contains(&"reference".to_string()));
            }
            _ => panic!("expected InvalidCategory"),
        }
    }

    /// Helper: poison the RwLock by panicking in a write thread.
    fn poison_allowlist(al: &std::sync::Arc<CategoryAllowlist>) {
        let al_clone = std::sync::Arc::clone(al);
        let handle = std::thread::spawn(move || {
            // Acquire write lock directly (field is accessible in same-crate tests)
            let mut guard = al_clone.categories.write().unwrap();
            guard.insert("pre-panic-insert".to_string());
            panic!("intentional poison for testing");
        });
        // Thread panicked — lock is now poisoned.
        let _ = handle.join();
    }

    #[test]
    fn test_poison_recovery_validate() {
        let al = std::sync::Arc::new(CategoryAllowlist::new());
        poison_allowlist(&al);
        // validate() should recover from the poisoned lock.
        assert!(al.validate("outcome").is_ok());
        assert!(al.validate("bogus").is_err());
    }

    #[test]
    fn test_poison_recovery_add_category() {
        let al = std::sync::Arc::new(CategoryAllowlist::new());
        poison_allowlist(&al);
        // add_category() should recover from the poisoned lock.
        al.add_category("custom-after-poison".to_string());
        assert!(al.validate("custom-after-poison").is_ok());
    }

    #[test]
    fn test_poison_recovery_list_categories() {
        let al = std::sync::Arc::new(CategoryAllowlist::new());
        poison_allowlist(&al);
        // list_categories() should recover and return valid data.
        let list = al.list_categories();
        // Should have initial 8 + "pre-panic-insert" from the poisoning thread.
        assert!(list.contains(&"outcome".to_string()));
        assert!(list.contains(&"convention".to_string()));
        assert!(list.len() >= 8);
    }

    #[test]
    fn test_poison_recovery_data_integrity() {
        let al = std::sync::Arc::new(CategoryAllowlist::new());
        al.add_category("custom-before".to_string());
        poison_allowlist(&al);
        let list = al.list_categories();
        // Data from before the poison should still be present.
        assert!(list.contains(&"custom-before".to_string()));
        // The insert from the panicking thread may or may not be present
        // (depends on timing), but the initial categories must survive.
        assert!(list.contains(&"outcome".to_string()));
        assert!(list.contains(&"decision".to_string()));
    }
}
