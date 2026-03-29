//! crt-031 lifecycle policy tests for CategoryAllowlist.
//!
//! Tests for: from_categories_with_policy, is_adaptive, list_adaptive,
//! delegation chain, pinned-by-default, and poison recovery for adaptive lock.

use super::*;
use tests::{poison_adaptive_lock, poison_allowlist as _poison_allowlist};

// --- crt-031: new lifecycle policy tests ---

/// AC-13, R-09: new() delegation chain sets lesson-learned as adaptive.
#[test]
fn test_new_delegates_adaptive_policy() {
    let al = CategoryAllowlist::new();
    assert!(
        al.is_adaptive("lesson-learned"),
        "new() must produce lesson-learned as adaptive via delegation chain"
    );
    assert!(
        !al.is_adaptive("decision"),
        "decision must be pinned (not adaptive) by default"
    );
    // validate still works (categories lock unaffected)
    assert!(al.validate("decision").is_ok());
}

/// AC-05: is_adaptive returns true for lesson-learned with default policy.
#[test]
fn test_is_adaptive_lesson_learned_default_true() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec!["lesson-learned".to_string()],
    );
    assert!(al.is_adaptive("lesson-learned"));
}

/// AC-06: is_adaptive returns false for non-adaptive category.
#[test]
fn test_is_adaptive_decision_default_false() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec!["lesson-learned".to_string()],
    );
    assert!(!al.is_adaptive("decision"));
}

/// AC-07: is_adaptive returns false for unknown category.
#[test]
fn test_is_adaptive_unknown_category_false() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec!["lesson-learned".to_string()],
    );
    assert!(!al.is_adaptive("nonexistent-category"));
    assert!(!al.is_adaptive(""));
}

/// E-06: is_adaptive is case-sensitive.
#[test]
fn test_is_adaptive_case_sensitive() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec!["lesson-learned".to_string()],
    );
    assert!(!al.is_adaptive("Lesson-Learned"));
    assert!(!al.is_adaptive("LESSON-LEARNED"));
}

/// E-03: single-character category name works correctly.
#[test]
fn test_is_adaptive_single_char_category() {
    let al = CategoryAllowlist::from_categories_with_policy(
        vec!["x".to_string()],
        vec!["x".to_string()],
    );
    assert!(al.is_adaptive("x"));
}

/// from_categories_with_policy: custom two-adaptive policy.
#[test]
fn test_from_categories_with_policy_custom_adaptive() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec!["lesson-learned".to_string(), "pattern".to_string()],
    );
    assert!(al.is_adaptive("lesson-learned"));
    assert!(al.is_adaptive("pattern"));
    assert!(!al.is_adaptive("decision"));
    assert!(!al.is_adaptive("convention"));
    // list_adaptive returns sorted output
    let adaptive = al.list_adaptive();
    assert_eq!(adaptive, vec!["lesson-learned", "pattern"]);
}

/// from_categories_with_policy: categories still accessible.
#[test]
fn test_from_categories_with_policy_categories_accessible() {
    let al = CategoryAllowlist::from_categories_with_policy(
        vec!["lesson-learned".to_string(), "decision".to_string()],
        vec!["lesson-learned".to_string()],
    );
    assert!(al.validate("lesson-learned").is_ok());
    assert!(al.validate("decision").is_ok());
    assert!(al.validate("pattern").is_err());
}

/// E-01: empty adaptive list — is_adaptive always false.
#[test]
fn test_from_categories_with_policy_empty_adaptive() {
    let al =
        CategoryAllowlist::from_categories_with_policy(vec!["lesson-learned".to_string()], vec![]);
    assert!(al.validate("lesson-learned").is_ok());
    assert!(!al.is_adaptive("lesson-learned"));
    assert!(al.list_adaptive().is_empty());
}

/// E-02: all 5 categories marked adaptive.
#[test]
fn test_from_categories_with_policy_all_adaptive() {
    let all: Vec<String> = INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect();
    let al = CategoryAllowlist::from_categories_with_policy(all.clone(), all);
    for cat in INITIAL_CATEGORIES {
        assert!(al.is_adaptive(cat), "category '{}' should be adaptive", cat);
    }
}

/// E-04: duplicates in adaptive list are silently deduplicated.
#[test]
fn test_from_categories_with_policy_duplicate_adaptive_deduplicates() {
    let al = CategoryAllowlist::from_categories_with_policy(
        vec!["lesson-learned".to_string()],
        vec!["lesson-learned".to_string(), "lesson-learned".to_string()],
    );
    let adaptive = al.list_adaptive();
    assert_eq!(adaptive, vec!["lesson-learned"]);
}

/// from_categories delegates with lesson-learned as default adaptive.
#[test]
fn test_from_categories_delegates_with_lesson_learned_adaptive() {
    let al = CategoryAllowlist::from_categories(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
    );
    assert!(al.is_adaptive("lesson-learned"));
}

/// list_adaptive returns alphabetically sorted output.
#[test]
fn test_list_adaptive_returns_sorted() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec!["pattern".to_string(), "lesson-learned".to_string()],
    );
    let result = al.list_adaptive();
    assert_eq!(result, vec!["lesson-learned", "pattern"]);
}

/// list_adaptive with unsorted multi-item input produces sorted output.
#[test]
fn test_list_adaptive_sorted() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec![
            "procedure".to_string(),
            "decision".to_string(),
            "lesson-learned".to_string(),
        ],
    );
    let result = al.list_adaptive();
    assert_eq!(result, vec!["decision", "lesson-learned", "procedure"]);
}

/// list_adaptive: empty when no adaptive categories.
#[test]
fn test_list_adaptive_empty_when_no_adaptive() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec![],
    );
    assert!(al.list_adaptive().is_empty());
}

/// list_adaptive: returns all adaptive categories.
#[test]
fn test_list_adaptive_returns_all_adaptive() {
    let al = CategoryAllowlist::from_categories_with_policy(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        vec![
            "lesson-learned".to_string(),
            "decision".to_string(),
            "convention".to_string(),
        ],
    );
    let result = al.list_adaptive();
    assert_eq!(result.len(), 3);
    assert!(result.contains(&"lesson-learned".to_string()));
    assert!(result.contains(&"decision".to_string()));
    assert!(result.contains(&"convention".to_string()));
}

/// R-03 scenario 1: add_category always defaults to pinned.
#[test]
fn test_add_category_defaults_to_pinned() {
    let al = CategoryAllowlist::new();
    al.add_category("custom".to_string());
    assert!(al.validate("custom").is_ok());
    assert!(!al.is_adaptive("custom"));
}

/// R-03 scenario 2: validate passes and is_adaptive is false simultaneously.
#[test]
fn test_validate_passes_is_adaptive_false_simultaneously() {
    let al = CategoryAllowlist::new();
    al.add_category("new-cat".to_string());
    assert!(al.validate("new-cat").is_ok());
    assert!(!al.is_adaptive("new-cat"));
}

/// AC-08: Poison recovery for adaptive lock — is_adaptive must not panic.
#[test]
fn test_poison_recovery_is_adaptive() {
    let al = std::sync::Arc::new(CategoryAllowlist::new());
    poison_adaptive_lock(&al);
    // is_adaptive must not panic after adaptive lock is poisoned.
    let result_ll = al.is_adaptive("lesson-learned");
    let result_dec = al.is_adaptive("decision");
    // After poison recovery, the data inserted before the panic is still present.
    // "lesson-learned" was in adaptive before poisoning, so it remains adaptive.
    assert!(
        result_ll,
        "lesson-learned must still be adaptive after poison recovery"
    );
    assert!(
        !result_dec,
        "decision must remain non-adaptive after poison recovery"
    );
}

/// Poison recovery for adaptive lock — list_adaptive must not panic.
#[test]
fn test_poison_recovery_list_adaptive() {
    let al = std::sync::Arc::new(CategoryAllowlist::new());
    poison_adaptive_lock(&al);
    // list_adaptive must not panic after adaptive lock is poisoned.
    let result = al.list_adaptive();
    // lesson-learned is in the initial adaptive set and survives poison recovery.
    assert!(
        result.contains(&"lesson-learned".to_string()),
        "lesson-learned must be present after adaptive lock poison recovery"
    );
}
