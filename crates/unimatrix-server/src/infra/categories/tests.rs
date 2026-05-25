//! Pre-existing tests for CategoryAllowlist (AC-12: must pass without modification).

use super::*;

// --- Pre-existing tests (AC-12: must pass without modification) ---

#[test]
fn test_validate_outcome() {
    // ADR-005: "outcome" retired from allowlist — new ingest is blocked.
    let al = CategoryAllowlist::new();
    assert!(al.validate("outcome").is_err());
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
fn test_validate_goal() {
    let al = CategoryAllowlist::new();
    assert!(al.validate("goal").is_ok());
}

#[test]
fn test_validate_feature() {
    let al = CategoryAllowlist::new();
    assert!(al.validate("feature").is_ok());
}

#[test]
fn test_validate_duties() {
    let al = CategoryAllowlist::new();
    assert!(al.validate("duties").is_err());
}

#[test]
fn test_validate_reference() {
    let al = CategoryAllowlist::new();
    assert!(al.validate("reference").is_err());
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
            assert_eq!(valid_categories.len(), 7);
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
    assert_eq!(list.len(), 7);
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
            assert!(valid_categories.contains(&"feature".to_string()));
            assert!(valid_categories.contains(&"goal".to_string()));
            assert!(valid_categories.contains(&"lesson-learned".to_string()));
            assert!(valid_categories.contains(&"pattern".to_string()));
            assert!(valid_categories.contains(&"procedure".to_string()));
            // ADR-005: "outcome" retired — must not appear in the valid-categories list.
            assert!(!valid_categories.contains(&"outcome".to_string()));
            // Removed in bugfix-436: duties and reference were stale categories.
            assert!(!valid_categories.contains(&"duties".to_string()));
            assert!(!valid_categories.contains(&"reference".to_string()));
        }
        _ => panic!("expected InvalidCategory"),
    }
}

/// Helper: poison the categories RwLock by panicking in a write thread.
pub(super) fn poison_allowlist(al: &std::sync::Arc<CategoryAllowlist>) {
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

/// Helper: poison the adaptive RwLock by panicking in a write thread.
pub(super) fn poison_adaptive_lock(al: &std::sync::Arc<CategoryAllowlist>) {
    let al_clone = std::sync::Arc::clone(al);
    let handle = std::thread::spawn(move || {
        let mut guard = al_clone.adaptive.write().unwrap();
        guard.insert("pre-panic-insert".to_string());
        panic!("intentional poison for testing");
    });
    let _ = handle.join();
}

#[test]
fn test_poison_recovery_validate() {
    let al = std::sync::Arc::new(CategoryAllowlist::new());
    poison_allowlist(&al);
    // validate() should recover from the poisoned lock.
    // ADR-005: "outcome" is no longer in the default allowlist.
    assert!(al.validate("outcome").is_err());
    assert!(al.validate("decision").is_ok());
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
    // Should have initial 7 + "pre-panic-insert" from the poisoning thread.
    // ADR-005: "outcome" is no longer a default category.
    assert!(!list.contains(&"outcome".to_string()));
    assert!(list.contains(&"convention".to_string()));
    assert!(list.len() >= 7);
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
    // ADR-005: "outcome" is no longer a default category.
    assert!(!list.contains(&"outcome".to_string()));
    assert!(list.contains(&"decision".to_string()));
}

// --- dsn-001: from_categories tests ---

#[test]
fn test_new_delegates_to_from_categories_initial() {
    let from_new = CategoryAllowlist::new();
    let from_cats = CategoryAllowlist::from_categories(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
    );
    for cat in INITIAL_CATEGORIES {
        assert_eq!(
            from_new.validate(cat).is_ok(),
            from_cats.validate(cat).is_ok(),
            "new() and from_categories(INITIAL) differ for category '{}'",
            cat
        );
    }
}

#[test]
fn test_new_allows_outcome_and_decision() {
    let al = CategoryAllowlist::new();
    // ADR-005: "outcome" retired — must not be in default allowlist.
    assert!(
        al.validate("outcome").is_err(),
        "outcome must not be in default allowlist"
    );
    assert!(
        al.validate("decision").is_ok(),
        "decision must be in default allowlist"
    );
    assert!(
        al.validate("pattern").is_ok(),
        "pattern must be in default allowlist"
    );
    assert!(
        al.validate("lesson-learned").is_ok(),
        "lesson-learned must be in default"
    );
}

#[test]
fn test_new_rejects_unknown_category() {
    let al = CategoryAllowlist::new();
    assert!(
        al.validate("hypothetical_new_category").is_err(),
        "unknown categories must be rejected by default allowlist"
    );
    assert!(
        al.validate("ruling").is_err(),
        "'ruling' (legal domain) must not be in default allowlist"
    );
}

#[test]
fn test_from_categories_custom_list_replaces_defaults() {
    let al = CategoryAllowlist::from_categories(vec!["custom-cat".into()]);
    assert!(
        al.validate("custom-cat").is_ok(),
        "'custom-cat' must be allowed when in the supplied list"
    );
    assert!(
        al.validate("outcome").is_err(),
        "'outcome' must not be allowed when not in the custom list"
    );
    assert!(
        al.validate("decision").is_err(),
        "'decision' must not be allowed when not in the custom list"
    );
    assert!(
        al.validate("lesson-learned").is_err(),
        "'lesson-learned' must not be allowed when not in the custom list"
    );
}

#[test]
fn test_from_categories_single_element_list() {
    let al = CategoryAllowlist::from_categories(vec!["ruling".into()]);
    assert!(al.validate("ruling").is_ok());
    assert!(al.validate("outcome").is_err());
}

#[test]
fn test_from_categories_multiple_custom_categories() {
    let cats = vec![
        "ruling".into(),
        "statute".into(),
        "brief".into(),
        "precedent".into(),
    ];
    let al = CategoryAllowlist::from_categories(cats.clone());
    for cat in &cats {
        assert!(al.validate(cat).is_ok(), "'{}' must be allowed", cat);
    }
    assert!(al.validate("decision").is_err());
    assert!(al.validate("lesson-learned").is_err());
}

#[test]
fn test_from_categories_empty_list_accepts_nothing() {
    let al = CategoryAllowlist::from_categories(vec![]);
    // All categories rejected — degenerate but valid configuration.
    assert!(al.validate("outcome").is_err());
    assert!(al.validate("decision").is_err());
    assert!(al.validate("custom-cat").is_err());
    // Must not panic.
}

// --- crt-025 ADR-005: outcome category retirement tests ---

/// AC-15, FR-08.2: CategoryAllowlist::new() must have exactly 7 categories.
#[test]
fn test_category_allowlist_has_seven_categories() {
    let al = CategoryAllowlist::new();
    assert_eq!(
        al.list_categories().len(),
        7,
        "INITIAL_CATEGORIES must contain exactly 7 entries (goal and feature added)"
    );
}

/// AC-15: "outcome" must not be in the allowlist after retirement.
#[test]
fn test_outcome_category_is_not_in_allowlist() {
    let al = CategoryAllowlist::new();
    assert!(
        al.validate("outcome").is_err(),
        "outcome must be rejected after ADR-005 retirement"
    );
}

/// AC-15: validate("outcome") must return Err with a meaningful message.
#[test]
fn test_outcome_category_validate_err() {
    let al = CategoryAllowlist::new();
    let err = al.validate("outcome").unwrap_err();
    match err {
        ServerError::InvalidCategory {
            category,
            valid_categories,
        } => {
            assert_eq!(category, "outcome");
            assert_eq!(valid_categories.len(), 7);
            assert!(!valid_categories.contains(&"outcome".to_string()));
        }
        _ => panic!("expected InvalidCategory error for retired category"),
    }
}

/// R-03: All 7 categories validate successfully (regression guard).
#[test]
fn test_all_remaining_categories_valid() {
    let al = CategoryAllowlist::new();
    for cat in &INITIAL_CATEGORIES {
        assert!(
            al.validate(cat).is_ok(),
            "category '{}' must be valid (present in INITIAL_CATEGORIES)",
            cat
        );
    }
}

/// R-03: Removal of "outcome" is surgical — other categories are not affected.
#[test]
fn test_only_outcome_removed_not_others() {
    let al = CategoryAllowlist::new();
    assert!(al.validate("decision").is_ok());
    assert!(al.validate("convention").is_ok());
    assert!(al.validate("outcome").is_err());
}

/// Poison recovery path must also reflect the updated INITIAL_CATEGORIES (no outcome).
#[test]
fn test_category_allowlist_poison_recovery() {
    let al = std::sync::Arc::new(CategoryAllowlist::new());
    poison_allowlist(&al);
    // After recovery, outcome must still be absent.
    assert!(
        al.validate("outcome").is_err(),
        "poison recovery must not restore outcome to the allowlist"
    );
    // Other standard categories must survive.
    assert!(al.validate("decision").is_ok());
    assert!(al.validate("convention").is_ok());
}

// -----------------------------------------------------------------------
// #635: Category authority enforcement tests
// -----------------------------------------------------------------------

/// Domain pack categories not in the operator-configured allowlist must be
/// rejected by validate(). This proves the authority boundary is enforced.
#[test]
fn test_domain_pack_category_rejected_when_not_in_operator_config() {
    // Operator configures a restricted allowlist (only 2 categories).
    let al =
        CategoryAllowlist::from_categories(vec!["decision".to_string(), "pattern".to_string()]);
    // Simulate domain pack categories — some overlap, some do not.
    let pack_categories = vec![
        "decision".to_string(),       // in allowlist
        "pattern".to_string(),        // in allowlist
        "lesson-learned".to_string(), // NOT in allowlist
        "convention".to_string(),     // NOT in allowlist
    ];
    let mut accepted = vec![];
    let mut rejected = vec![];
    for cat in &pack_categories {
        if al.validate(cat).is_ok() {
            accepted.push(cat.clone());
        } else {
            rejected.push(cat.clone());
        }
    }
    assert_eq!(
        accepted,
        vec!["decision".to_string(), "pattern".to_string()],
        "only categories in operator config must be accepted"
    );
    assert_eq!(
        rejected,
        vec!["lesson-learned".to_string(), "convention".to_string()],
        "categories NOT in operator config must be rejected"
    );
}

/// When the operator configures all 7 default categories, all domain pack
/// categories from the builtin claude-code pack should be accepted.
#[test]
fn test_domain_pack_categories_accepted_when_all_in_allowlist() {
    let al = CategoryAllowlist::new(); // all 7 INITIAL_CATEGORIES
    let builtin_pack_categories = vec![
        "convention",
        "decision",
        "feature",
        "goal",
        "lesson-learned",
        "pattern",
        "procedure",
    ];
    for cat in builtin_pack_categories {
        assert!(
            al.validate(cat).is_ok(),
            "category '{}' must be accepted when using default allowlist",
            cat
        );
    }
}

/// When the operator configures an empty allowlist, ALL domain pack
/// categories must be rejected.
#[test]
fn test_domain_pack_categories_all_rejected_with_empty_allowlist() {
    let al = CategoryAllowlist::from_categories(vec![]);
    let pack_categories = vec!["decision", "pattern", "lesson-learned"];
    for cat in pack_categories {
        assert!(
            al.validate(cat).is_err(),
            "category '{}' must be rejected with empty allowlist",
            cat
        );
    }
}

/// add_category is restricted to #[cfg(test)] — verify it still works in tests.
#[test]
fn test_add_category_available_in_test_code() {
    let al = CategoryAllowlist::new();
    assert!(al.validate("test-only-cat").is_err());
    al.add_category("test-only-cat".to_string());
    assert!(al.validate("test-only-cat").is_ok());
}
