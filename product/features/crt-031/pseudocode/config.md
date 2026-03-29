# Component Pseudocode: infra/config (KnowledgeConfig extension)

## Purpose

Extend `KnowledgeConfig` with `adaptive_categories`, change `Default` to return `vec![]` for
both `boosted_categories` and `adaptive_categories`, add serde default functions for both fields,
add `ConfigError::AdaptiveCategoryNotInAllowlist`, extend `validate_config` and `merge_configs`,
and expose `default_boosted_categories_set()` as the single source of truth for the default value
used by all test infrastructure sites.

---

## Modified: `KnowledgeConfig` struct

### Current Shape (from codebase)
```
pub struct KnowledgeConfig {
    pub categories: Vec<String>,                          // serde default from struct-level #[serde(default)]
    pub boosted_categories: Vec<String>,                  // Default: vec!["lesson-learned"]
    pub freshness_half_life_hours: Option<f64>,
}
```

### New Shape
```
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]  // struct-level serde(default) retained
pub struct KnowledgeConfig {
    pub categories: Vec<String>,
    /// Categories that receive a provenance boost in search re-ranking.
    /// Default (serde): ["lesson-learned"]. Default (Rust): [].
    #[serde(default = "default_boosted_categories")]
    pub boosted_categories: Vec<String>,
    /// Categories eligible for automated lifecycle management (#409).
    /// Must be a subset of `categories`. Default (serde): ["lesson-learned"]. Default (Rust): [].
    #[serde(default = "default_adaptive_categories")]
    pub adaptive_categories: Vec<String>,
    pub freshness_half_life_hours: Option<f64>,
}
```

Note: `#[serde(default = "fn")]` on a field overrides the struct-level `#[serde(default)]`
for that field only. Both attributes coexist correctly.

---

## Modified: `Default for KnowledgeConfig`

### Current behavior
```
boosted_categories: vec!["lesson-learned".to_string()]
```

### New behavior
```
impl Default for KnowledgeConfig {
    fn default() -> Self {
        KnowledgeConfig {
            categories: INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
            boosted_categories: vec![],    // CHANGED: was vec!["lesson-learned"]
            adaptive_categories: vec![],   // NEW: empty is the programmatic default
            freshness_half_life_hours: None,
        }
    }
}
```

Rationale (ADR-001 decision 4): `Default` expresses programmatic absence; serde default fns
express the production config default. Separating them eliminates the test trap documented in
entry #3774 and the workaround in `test_empty_categories_documented_behavior`.

---

## New: Serde Default Functions

```
// Private — only used by #[serde(default = "...")] attributes above.
// Returns the value a config file omitting the field receives.

fn default_boosted_categories() -> Vec<String> {
    vec!["lesson-learned".to_string()]
}

fn default_adaptive_categories() -> Vec<String> {
    vec!["lesson-learned".to_string()]
}
```

Placement: immediately above the `KnowledgeConfig` struct definition (or in the same block
as the existing module-level constants section). Private — no `pub` qualifier.

---

## New: `default_boosted_categories_set` public helper

```
/// Returns the default boosted-categories set as a HashSet.
///
/// Single source of truth for the default value. Replaces the six
/// `HashSet::from(["lesson-learned".to_string()])` literals scattered across
/// test infrastructure files (crt-031 FR-16, SR-08 resolution).
///
/// Importable from all seven sites via `crate::infra::config::default_boosted_categories_set()`
/// without circular dependency (infra/config.rs has no upward dependency on any test file).
pub fn default_boosted_categories_set() -> HashSet<String> {
    default_boosted_categories().into_iter().collect()
}
```

Placement: at module level in `config.rs`, after the serde default functions.
Return type: `HashSet<String>` (the same type expected at all call sites).

---

## New: `ConfigError::AdaptiveCategoryNotInAllowlist` variant

### Location
The `ConfigError` enum in `config.rs` (same enum as `BoostedCategoryNotInAllowlist`).

### New Variant
```
ConfigError::AdaptiveCategoryNotInAllowlist {
    path: PathBuf,
    category: String,
}
```

### Display Implementation
```
// In the existing fmt::Display impl for ConfigError, add a new arm:
ConfigError::AdaptiveCategoryNotInAllowlist { path, category } => write!(
    f,
    "config error in {}: [knowledge] adaptive_categories contains {:?} \
     which is not present in the categories list; add it to [knowledge] categories first",
    path.display(),
    category
),
```

Pattern mirrors `BoostedCategoryNotInAllowlist` exactly. The `{:?}` debug formatter is used
for `category` to provide safe quoting and escape of operator-supplied strings (S-02).

---

## Modified: `validate_config`

### Insertion Point
Immediately after the existing `// --- Validate [knowledge] boosted_categories ---` block
(which ends at line ~1461 in the current codebase).

The `category_set: HashSet<&str>` built for the boosted check is reused — no redundant work.

### New Block (inserted after boosted check, before freshness check)

```
// --- Validate [knowledge] adaptive_categories ---
// Reuses the same `category_set` built for the boosted check above.
// Empty adaptive_categories is valid (disables automated management entirely, E-01).
for adaptive_cat in &config.knowledge.adaptive_categories {
    if !category_set.contains(adaptive_cat.as_str()) {
        return Err(ConfigError::AdaptiveCategoryNotInAllowlist {
            path: path.into(),
            category: adaptive_cat.clone(),
        })
    }
}
```

Error handling: fail-fast on first offending entry (same semantics as boosted check).
Multiple offending entries: only the first is reported; operator fixes iteratively.

---

## Modified: `merge_configs`

### Insertion Point
In the `knowledge: KnowledgeConfig { ... }` block of `merge_configs`, immediately after the
existing `boosted_categories` merge block (lines ~1818–1824 in current codebase).

### New Block

```
adaptive_categories: if project.knowledge.adaptive_categories
    != default.knowledge.adaptive_categories
{
    project.knowledge.adaptive_categories
} else {
    global.knowledge.adaptive_categories
},
```

Semantics: project value wins if it differs from the `Default` value (same comparison used for
`boosted_categories` and `categories`). After the `Default` change, `default.knowledge.adaptive_categories`
is `vec![]`, so: if the project config has any non-empty list, it wins; if the project config
omits `adaptive_categories` (producing serde default `["lesson-learned"]`), the comparison is
`["lesson-learned"] != []` which is true, so the project value wins.

Wait — this is a subtle interaction. When both configs use serde deserialization:
- Project omits `adaptive_categories` → serde produces `["lesson-learned"]`
- Global omits `adaptive_categories` → serde produces `["lesson-learned"]`
- `default.knowledge.adaptive_categories` is `vec![]` (from `Default` impl)
- `["lesson-learned"] != []` is true → project value wins even when both are default

This is the same behavior as `boosted_categories` after the `Default` change. The existing
`boosted_categories` merge block has the identical semantic subtlety. This is correct
per the spec (FR-10): project-overrides-global pattern is replicated exactly.

For the R-07 test: project `["pattern"]` vs global `["lesson-learned"]` — project wins because
`["pattern"] != []`. Test scenario 2 (project omits, global has non-default): both sides produce
serde default `["lesson-learned"]` and `["lesson-learned"] != []` is true, so project wins —
which happens to be `["lesson-learned"]` matching global. This produces the correct result in
the test scenario.

The implementation agent must verify the comparison semantics against the test scenarios in R-07.

---

## Modified: `config_with_categories` and similar test helpers

Any test helper that constructs `KnowledgeConfig` with a custom `categories` list must be
updated to set both parallel lists to `vec![]`:

```
// Required pattern for all test helpers with custom categories (AC-24, R-01):
KnowledgeConfig {
    categories: /* test-specific */,
    boosted_categories: vec![],      // suppress boosted cross-check
    adaptive_categories: vec![],     // suppress adaptive cross-check
    freshness_half_life_hours: None,
}
```

Grep target: `KnowledgeConfig {` across `crates/` before implementing. Every struct-literal
construction with a non-default `categories` field must be audited.

Known helper: `config_with_categories` in `config.rs` test helpers (lines ~2175–2178 in current
codebase). Add `adaptive_categories: vec![]` alongside `boosted_categories: vec![]` (if not
already present) or add both if missing.

---

## Tests

### AC-17: KnowledgeConfig Default returns empty boosted
```
test_knowledge_config_default_boosted_is_empty:
  config = KnowledgeConfig::default()
  assert config.boosted_categories.is_empty()
```

### AC-27: KnowledgeConfig Default returns empty adaptive
```
test_knowledge_config_default_adaptive_is_empty:
  config = KnowledgeConfig::default()
  assert config.adaptive_categories.is_empty()
```

### AC-01 + AC-02: serde deserialization default for adaptive_categories
```
test_adaptive_categories_serde_default_is_lesson_learned:
  // Parse minimal TOML with no adaptive_categories field
  config: UnimatrixConfig = toml::from_str("").unwrap()  // or minimal valid TOML
  assert config.knowledge.adaptive_categories == vec!["lesson-learned"]

test_adaptive_categories_explicit_value:
  toml_str = r#"
  [knowledge]
  categories = ["lesson-learned", "convention"]
  adaptive_categories = ["lesson-learned", "convention"]
  "#
  config: KnowledgeConfig = toml::from_str(toml_str).unwrap()
  assert config.knowledge.adaptive_categories == vec!["lesson-learned", "convention"]
```

### AC-03: Multiple adaptive values
```
test_adaptive_categories_multiple_values:
  toml = r#"[knowledge]
  categories = ["lesson-learned", "convention"]
  adaptive_categories = ["lesson-learned", "convention"]"#
  config = toml::from_str(toml_str)
  assert config.knowledge.adaptive_categories.len() == 2
```

### AC-04: validate_config rejects adaptive category not in categories
```
test_validate_config_adaptive_not_in_categories:
  config = KnowledgeConfig {
      categories: vec!["lesson-learned"],
      boosted_categories: vec![],    // zeroed — suppress boosted cross-check
      adaptive_categories: vec!["unknown-cat"],
      freshness_half_life_hours: None,
  }
  result = validate_config(&UnimatrixConfig { knowledge: config, ..Default::default() }, path)
  assert matches!(result, Err(ConfigError::AdaptiveCategoryNotInAllowlist { category, .. })
    if category == "unknown-cat")
```

### AC-25: fixture isolation — adaptive error does not fire boosted error first
```
test_validate_config_adaptive_error_not_masked_by_boosted:
  config = KnowledgeConfig {
      categories: vec!["lesson-learned"],
      boosted_categories: vec![],     // explicitly zeroed
      adaptive_categories: vec!["nonexistent"],
      freshness_half_life_hours: None,
  }
  result = validate_config(...)
  assert matches!(result, Err(ConfigError::AdaptiveCategoryNotInAllowlist { .. }))
  // NOT BoostedCategoryNotInAllowlist
```

### AC-14: empty adaptive passes validate
```
test_validate_config_empty_adaptive_is_valid:
  config = KnowledgeConfig {
      categories: vec!["lesson-learned"],
      boosted_categories: vec![],
      adaptive_categories: vec![],
      freshness_half_life_hours: None,
  }
  assert validate_config(...).is_ok()
```

### AC-15: multi-entry adaptive passes validate
```
test_validate_config_multiple_adaptive_valid:
  config = KnowledgeConfig {
      categories: vec!["lesson-learned", "pattern"],
      boosted_categories: vec![],
      adaptive_categories: vec!["lesson-learned", "pattern"],
      freshness_half_life_hours: None,
  }
  assert validate_config(...).is_ok()
```

### AC-16: merge_configs project-wins semantics
```
test_merge_configs_adaptive_project_wins:
  // project has non-default adaptive_categories
  global = UnimatrixConfig { knowledge: KnowledgeConfig {
      adaptive_categories: vec!["lesson-learned"], .. }, .. }
  project = UnimatrixConfig { knowledge: KnowledgeConfig {
      adaptive_categories: vec!["pattern"], .. }, .. }
  merged = merge_configs(global, project)
  assert merged.knowledge.adaptive_categories == vec!["pattern"]  // project wins

test_merge_configs_adaptive_global_fallback:
  // project uses default (empty from Default::default())
  // global has a specific list via serde deserialization
  global = // deserialized from TOML with adaptive_categories = ["lesson-learned", "convention"]
  project = // Default::default() — adaptive_categories: vec![]
  merged = merge_configs(global, project)
  // project value is [] which differs from Default [] — see note above about semantics
  // Implementation agent must verify against actual merge_configs comparison logic
```

### E-07: serde round-trip
```
test_knowledge_config_serde_round_trip:
  original = KnowledgeConfig {
      categories: INITIAL_CATEGORIES...,
      boosted_categories: vec!["lesson-learned"],
      adaptive_categories: vec!["lesson-learned", "pattern"],
      freshness_half_life_hours: None,
  }
  serialized = toml::to_string(&original).unwrap()
  deserialized: KnowledgeConfig = toml::from_str(&serialized).unwrap()
  assert deserialized.adaptive_categories == vec!["lesson-learned", "pattern"]
  assert deserialized.boosted_categories == vec!["lesson-learned"]
```

### default_boosted_categories_set returns expected set
```
test_default_boosted_categories_set:
  set = default_boosted_categories_set()
  assert set.contains("lesson-learned")
  assert set.len() == 1
```

---

## Error Handling

`validate_config` is already `Result<(), ConfigError>`. The new block returns the new variant
on first mismatch, identical to the boosted check above it. Callers treat both variants the
same (fail-fast startup abort). No new propagation logic needed.

---

## Integration Notes

- `infra/config.rs` imports `crate::infra::categories::INITIAL_CATEGORIES` (existing).
- The new `default_boosted_categories_set()` function is importable as
  `crate::infra::config::default_boosted_categories_set` from any file in the server crate.
  `infra/config.rs` has no `use` statement pointing to `server.rs`, `shutdown.rs`,
  `test_support.rs`, `services/index_briefing.rs`, or `uds/listener.rs`.
  Circular dependency risk: none (confirmed, ARCHITECTURE.md SR-08 resolution).
