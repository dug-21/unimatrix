# dsn-001 Test Plan — category-allowlist

Component: `crates/unimatrix-server/src/infra/categories.rs`

Risks covered: R-17, IR-05, AC-02, AC-10 (partial — allowlist behavior).

---

## Scope of Changes

A new constructor `from_categories(Vec<String>) -> Self` is added.
The existing `CategoryAllowlist::new()` is changed to delegate to
`from_categories(INITIAL_CATEGORIES.to_vec())`. No other behavioral change.

**Non-negotiable constraints:**
- `CategoryAllowlist::new()` signature must not change.
- All existing test call sites using `new()` must continue to compile and pass.
- `new()` and `from_categories(INITIAL_CATEGORIES)` must produce identical results.

---

## `new()` Delegation and Identity (R-17, IR-05)

### test_new_delegates_to_from_categories_initial

```rust
fn test_new_delegates_to_from_categories_initial() {
    let from_new = CategoryAllowlist::new();
    let from_cats = CategoryAllowlist::from_categories(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect()
    );
    // Both constructors must produce identical allowlist behavior.
    // Use representative categories from INITIAL_CATEGORIES.
    for cat in INITIAL_CATEGORIES {
        assert_eq!(
            from_new.is_allowed(cat),
            from_cats.is_allowed(cat),
            "new() and from_categories(INITIAL) differ for category '{}'", cat
        );
    }
}
```

### test_new_allows_outcome_and_decision

```rust
fn test_new_allows_outcome_and_decision() {
    // Verify that compiled defaults include the expected categories.
    let al = CategoryAllowlist::new();
    assert!(al.is_allowed("outcome"),  "outcome must be in default allowlist");
    assert!(al.is_allowed("decision"), "decision must be in default allowlist");
    assert!(al.is_allowed("pattern"),  "pattern must be in default allowlist");
    assert!(al.is_allowed("lesson-learned"), "lesson-learned must be in default");
}
```

### test_new_rejects_unknown_category

```rust
fn test_new_rejects_unknown_category() {
    let al = CategoryAllowlist::new();
    assert!(!al.is_allowed("hypothetical_new_category"),
        "unknown categories must be rejected by default allowlist");
    assert!(!al.is_allowed("ruling"),
        "'ruling' (legal domain) must not be in default allowlist");
}
```

---

## `from_categories` Custom List (AC-02)

### test_from_categories_custom_list_replaces_defaults

```rust
fn test_from_categories_custom_list_replaces_defaults() {
    let al = CategoryAllowlist::from_categories(vec!["custom-cat".into()]);
    // Custom list replaces compiled defaults entirely.
    assert!(al.is_allowed("custom-cat"),
        "'custom-cat' must be allowed when in the supplied list");
    assert!(!al.is_allowed("outcome"),
        "'outcome' must not be allowed when not in the custom list");
    assert!(!al.is_allowed("decision"),
        "'decision' must not be allowed when not in the custom list");
    assert!(!al.is_allowed("lesson-learned"),
        "'lesson-learned' must not be allowed when not in the custom list");
}
```

### test_from_categories_single_element_list

```rust
fn test_from_categories_single_element_list() {
    let al = CategoryAllowlist::from_categories(vec!["ruling".into()]);
    assert!(al.is_allowed("ruling"));
    assert!(!al.is_allowed("outcome"));
}
```

### test_from_categories_multiple_custom_categories

```rust
fn test_from_categories_multiple_custom_categories() {
    let cats = vec!["ruling".into(), "statute".into(), "brief".into(), "precedent".into()];
    let al = CategoryAllowlist::from_categories(cats.clone());
    for cat in &cats {
        assert!(al.is_allowed(cat), "'{}' must be allowed", cat);
    }
    // Compiled defaults not in the custom list are excluded.
    assert!(!al.is_allowed("decision"));
    assert!(!al.is_allowed("lesson-learned"));
}
```

---

## Empty List (EC-01 — allowlist perspective)

### test_from_categories_empty_list_accepts_nothing

```rust
fn test_from_categories_empty_list_accepts_nothing() {
    let al = CategoryAllowlist::from_categories(vec![]);
    // All categories rejected — degenerate but valid configuration.
    assert!(!al.is_allowed("outcome"));
    assert!(!al.is_allowed("decision"));
    assert!(!al.is_allowed("custom-cat"));
    // Must not panic.
}
```

---

## Existing Test Call Sites Unchanged (R-17)

This is validated by the full test suite: `cargo test --workspace 2>&1 | tail -30`
must show zero failures. Any test using `CategoryAllowlist::new()` must compile and
pass without modification.

Specifically, the following must hold after the change:
- All tests in `unimatrix-server` that call `CategoryAllowlist::new()` compile.
- `new().is_allowed("outcome")` returns `true` (same as pre-dsn-001).
- `new().is_allowed("hypothetical_new_category")` returns `false`.

These are not new tests — they are regression verification from existing test suite
runs. If any existing test fails after `new()` is changed to delegate, that is a
regression (R-17 materialized).

---

## Integration Notes

`CategoryAllowlist::from_categories` is called from `main.rs` after config load:
```rust
CategoryAllowlist::from_categories(config.knowledge.categories)
```

This wiring is tested in `startup-wiring.md`, not here. The component tests above
cover only the allowlist logic in isolation.
