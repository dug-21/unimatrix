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
        let cats = self.categories.read().expect("category lock poisoned");
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
        let mut cats = self.categories.write().expect("category lock poisoned");
        cats.insert(category);
    }

    /// List all valid categories (sorted alphabetically).
    pub fn list_categories(&self) -> Vec<String> {
        let cats = self.categories.read().expect("category lock poisoned");
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
}
