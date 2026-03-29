# Test Plan: infra/config.rs

Component from IMPLEMENTATION-BRIEF.md §Component Map row 2.

---

## MANDATORY Pre-Implementation Grep Steps (R-11, R-01)

These grep steps are named test steps — they must be run before writing any code.
Document the output in the PR description (AC-26).

### Step 1: KnowledgeConfig::default() callers (R-11, FR-19)

```bash
grep -rn "KnowledgeConfig::default()" crates/
grep -rn "UnimatrixConfig::default()" crates/
```

Inspect every hit for implicit reliance on `boosted_categories == ["lesson-learned"]`.
Known affected site: `main_tests.rs` lines 393-404. Any other hits must be updated in
the same PR if they rely on the old Default value.

### Step 2: validate_config test fixture audit (R-01, AC-24)

```bash
grep -rn 'KnowledgeConfig {' crates/
```

Audit every `KnowledgeConfig { ... }` struct literal in tests. For each one with a custom
`categories` list (not `..Default::default()`), confirm BOTH `boosted_categories: vec![]`
AND `adaptive_categories: vec![]` are explicitly set. Any fixture that only zeroes
`boosted_categories` must gain `adaptive_categories: vec![]` alongside it.

The `test_empty_categories_documented_behavior` test (config.rs ~line 3032) is the primary
known case — it currently sets `boosted_categories: vec![]` explicitly and must gain
`adaptive_categories: vec![]`. After the Default impl change, the explicit `boosted_categories`
override becomes redundant (AC-21) and should be removed, keeping only `adaptive_categories:
vec![]` if the struct literal is used without `..Default::default()`.

**Required pattern for all test fixtures with custom categories:**
```rust
KnowledgeConfig {
    categories: vec![/* test-specific */],
    boosted_categories: vec![],    // zero — suppress boosted cross-check
    adaptive_categories: vec![],   // zero — suppress adaptive cross-check
    freshness_half_life_hours: None,
}
```

---

## Risks Addressed

- **R-01** (Critical): Parallel-list collision — all validate_config fixtures must zero both lists.
- **R-07** (High): merge_configs omission of adaptive_categories.
- **R-11** (Critical): KnowledgeConfig::Default change causes silent test failures.

---

## Unit Test Expectations

### KnowledgeConfig Serde Round-Trip (AC-01, AC-02, AC-03)

**`test_adaptive_categories_serde_round_trip`** (AC-01)
- Arrange: `KnowledgeConfig { adaptive_categories: vec!["custom-a".to_string(), "custom-b".to_string()], ..KnowledgeConfig::default() }`
  then serialize with `toml::to_string` and deserialize with `toml::from_str`
- Assert: round-tripped value has `adaptive_categories == ["custom-a", "custom-b"]`
- Covers: E-07 (serde round-trip for both new fields)

**`test_adaptive_categories_serde_default_when_omitted`** (AC-02)
- Arrange: `toml::from_str::<UnimatrixConfig>("")` (empty TOML)
- Assert: `config.knowledge.adaptive_categories == vec!["lesson-learned"]`
- Note: this is the serde path, NOT the Rust `Default` path

**`test_adaptive_categories_serde_explicit_two_values`** (AC-03)
- Arrange: parse TOML string `"[knowledge]\nadaptive_categories = [\"lesson-learned\", \"convention\"]"`
  into `UnimatrixConfig`
- Assert: `config.knowledge.adaptive_categories == vec!["lesson-learned", "convention"]`

**`test_adaptive_categories_serde_explicit_empty_list`**
- Arrange: parse TOML `"[knowledge]\nadaptive_categories = []"` into `UnimatrixConfig`
- Assert: `config.knowledge.adaptive_categories == vec![]`
- Covers: operator explicitly disabling adaptive management

---

### KnowledgeConfig Default Impl Rewrite (AC-17, AC-27, R-11)

**`test_knowledge_config_default_boosted_is_empty`** (AC-17)
- Assert: `KnowledgeConfig::default().boosted_categories.is_empty()`
- This is the canonical regression guard for the Default impl change

**`test_knowledge_config_default_adaptive_is_empty`** (AC-27)
- Assert: `KnowledgeConfig::default().adaptive_categories.is_empty()`
- Mirrors AC-17 for the new field

Both tests must be added to `infra/config.rs` test module AND/OR `main_tests.rs`. Having
them in `config.rs` gives faster feedback during development.

---

### validate_config: adaptive_categories cross-check (AC-04, AC-14, AC-15, AC-25)

**Critical fixture construction pattern**: Every test below MUST use the zeroed-both-lists
pattern. Any test that sets only one of the two parallel lists to `[]` risks collision.

**`test_validate_config_adaptive_category_not_in_allowlist`** (AC-04)
- Arrange:
  ```rust
  KnowledgeConfig {
      categories: vec!["lesson-learned".to_string()],
      boosted_categories: vec![],    // zero — suppress boosted check
      adaptive_categories: vec!["nonexistent".to_string()],
      freshness_half_life_hours: None,
  }
  ```
- Assert: `matches!(result, Err(ConfigError::AdaptiveCategoryNotInAllowlist { category, .. }) if category == "nonexistent")`

**`test_validate_config_adaptive_empty_list_ok`** (AC-14)
- Arrange: default categories list, `adaptive_categories: vec![]`, `boosted_categories: vec![]`
- Assert: `validate_config` returns `Ok(())`

**`test_validate_config_adaptive_multi_entry_subset_ok`** (AC-15)
- Arrange: `categories: INITIAL_CATEGORIES`, `adaptive_categories: vec!["lesson-learned", "convention"]`,
  `boosted_categories: vec![]`
- Assert: `Ok(())`

**`test_validate_config_adaptive_error_isolated_from_boosted`** (AC-25, R-01 scenario 2)
- Arrange:
  ```rust
  KnowledgeConfig {
      categories: vec!["lesson-learned".to_string()],
      boosted_categories: vec![],                           // MUST be zeroed
      adaptive_categories: vec!["nonexistent".to_string()], // under test
      freshness_half_life_hours: None,
  }
  ```
- Assert: error is `AdaptiveCategoryNotInAllowlist`, NOT `BoostedCategoryNotInAllowlist`
- This test is the canonical proof that fixture isolation is correct

**`test_validate_config_boosted_error_isolated_from_adaptive`** (R-01 scenario 3)
- Arrange:
  ```rust
  KnowledgeConfig {
      categories: vec!["lesson-learned".to_string()],
      boosted_categories: vec!["nonexistent".to_string()], // under test
      adaptive_categories: vec![],                          // MUST be zeroed
      freshness_half_life_hours: None,
  }
  ```
- Assert: error is `BoostedCategoryNotInAllowlist`, NOT `AdaptiveCategoryNotInAllowlist`
- Confirms check ordering (boosted before adaptive per FR-03)

**`test_validate_config_both_parallel_lists_zeroed_ok`** (R-01 scenario 1)
- Arrange:
  ```rust
  KnowledgeConfig {
      categories: vec!["custom".to_string()],
      boosted_categories: vec![],
      adaptive_categories: vec![],
      freshness_half_life_hours: None,
  }
  ```
- Assert: `Ok(())`
- This is the canonical pattern test confirming the zeroed fixture works

---

### validate_config: existing test cleanup (AC-21, R-11)

**`test_empty_categories_documented_behavior`** — existing test, must be updated:
- Remove the explicit `boosted_categories: vec![]` line and its workaround comment
- Add `adaptive_categories: vec![]` explicitly (since a custom `categories` list is used)
- Test must still pass after this change
- Assertion: `validate_config` returns `Ok(())` for an empty categories list with both
  parallel lists zeroed

---

### merge_configs: adaptive_categories (AC-16, R-07)

**`test_merge_configs_adaptive_project_wins`** (AC-16 scenario 1, R-07 scenario 1)
- Arrange: project config `adaptive_categories = ["pattern"]`, global config
  `adaptive_categories = ["lesson-learned"]`
- Act: `merge_configs(&global, &project)`
- Assert: merged `knowledge.adaptive_categories == vec!["pattern"]`

**`test_merge_configs_adaptive_global_fallback`** (AC-16 scenario 2, R-07 scenario 2)
- Arrange: project config has `adaptive_categories` at its Default value (`vec![]`),
  global config has `adaptive_categories = ["lesson-learned", "convention"]`
- Act: `merge_configs(&global, &project)`
- Assert: merged `knowledge.adaptive_categories == vec!["lesson-learned", "convention"]`
- Note: project Default (`vec![]`) != global value, so global wins per project-overrides-global
  semantics. Verify the comparison uses the Default value correctly.

---

### default_boosted_categories_set helper

**`test_default_boosted_categories_set_contains_lesson_learned`**
- Assert: `default_boosted_categories_set().contains("lesson-learned")`
- Assert: `default_boosted_categories_set().len() == 1`
- Covers: the helper function is correct before being used at all 6 replacement sites

---

## Integration Test Expectations

No dedicated integration test needed for config.rs — validation happens at startup, not
through the MCP interface. Failures produce a startup abort that the integration harness
would catch as a binary crash (not a test-specific scenario).

The AC-23 cargo test gate covers `config.rs` tests indirectly:
- `cargo test -p unimatrix-server -- config` for fast targeted feedback
- `cargo test --workspace` for final gate
