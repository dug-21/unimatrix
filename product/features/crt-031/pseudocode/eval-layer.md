# Component Pseudocode: eval-layer (boosted_categories de-hardcoding)

## Purpose

Two related tasks share this file:

1. **`eval/profile/layer.rs` Step 12** — Replace one `HashSet::from(["lesson-learned".to_string()])`
   literal with derivation from `profile.config_overrides.knowledge.boosted_categories`.
   One-line fix. No threading change.

2. **Six test infrastructure sites** — Replace `HashSet::from(["lesson-learned".to_string()])`
   literals with `crate::infra::config::default_boosted_categories_set()`.

Both share the same root cause (hardcoded literal) and the same resolution pattern (use the
config-layer source of truth). The test infra removal is safe to merge into this file because
the pattern at all 6 sites is identical and there is no architectural complexity.

---

## Part 1: `eval/profile/layer.rs` Step 12

### Context

`layer.rs` has a function `from_profile(profile: &EvalProfile)`. At Step 12 (~line 277),
`boosted_categories` is built from a literal rather than the profile config:

```
// CURRENT (to be replaced):
let boosted_categories: HashSet<String> = HashSet::from(["lesson-learned".to_string()]);
```

`EvalProfile` carries `config_overrides: UnimatrixConfig`. This field is in scope at Step 12
because `profile` is the function parameter for `from_profile`. `config_overrides` is already
accessed at Step 2 (line ~122) and Step 3 (line ~149), confirming it is in scope (ARCHITECTURE.md
OQ-5 resolution).

### Change

```
// REPLACEMENT:
// Step 12: Boosted categories — derived from profile config, not a literal (crt-031 FR-15).
let boosted_categories: HashSet<String> =
    profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect();
```

One line replaced by two. No new imports needed (profile is a `&EvalProfile` already in scope,
`HashSet` is already imported, `.iter().cloned().collect()` is idiomatic).

### Correctness Check

`profile.config_overrides` is of type `UnimatrixConfig`. `UnimatrixConfig.knowledge` is of
type `KnowledgeConfig`. `KnowledgeConfig.boosted_categories` is `Vec<String>`.

After the `Default` change in `infra/config.rs`: if `profile.config_overrides` was constructed
via `Default::default()`, `boosted_categories` will be `vec![]`. The eval harness must ensure
that `EvalProfile` is populated from a deserialized config (which gets the serde default
`["lesson-learned"]`) rather than from a `Default` construction. If any eval test constructs
`EvalProfile { config_overrides: UnimatrixConfig::default(), .. }`, the `boosted_categories`
will be empty after this change.

Flag for implementation agent: run `grep -rn "EvalProfile.*default\|config_overrides.*default"
crates/` to check for any eval test constructing via Default. Update if found.

---

## Part 2: Six Test Infrastructure Literal Removal Sites

### Shared Pattern

Current literal at every site:
```
HashSet::from(["lesson-learned".to_string()])
```

Replacement at every site:
```
crate::infra::config::default_boosted_categories_set()
```

The replacement is a function call returning `HashSet<String>`. The type matches.
No `use` statement needed (fully qualified path). If a file already has
`use crate::infra::config::...` imports, the unqualified name `default_boosted_categories_set()`
can be used after adding it to the existing use statement.

### Site 1: `server.rs` (~line 287)

File: `crates/unimatrix-server/src/server.rs`

Context: Test infrastructure for `server.rs`. The literal appears in a test or
default initialization block.

```
// CURRENT:
let boosted_categories = HashSet::from(["lesson-learned".to_string()]);

// REPLACEMENT:
let boosted_categories = crate::infra::config::default_boosted_categories_set();
```

### Site 2: `infra/shutdown.rs` (~line 308)

File: `crates/unimatrix-server/src/infra/shutdown.rs`

First occurrence in shutdown test infrastructure.

```
// CURRENT:
HashSet::from(["lesson-learned".to_string()])

// REPLACEMENT:
crate::infra::config::default_boosted_categories_set()
```

### Site 3: `infra/shutdown.rs` (~line 408)

Second occurrence in shutdown test infrastructure. Same file, different test.

```
// CURRENT:
HashSet::from(["lesson-learned".to_string()])

// REPLACEMENT:
crate::infra::config::default_boosted_categories_set()
```

### Site 4: `test_support.rs` (~line 129)

File: `crates/unimatrix-server/src/test_support.rs`

Function: `build_service_layer_for_test` (or equivalent helper).
This is likely a `boosted_categories` argument to `ServiceLayer::new()`.

```
// CURRENT:
boosted_categories: HashSet::from(["lesson-learned".to_string()]),

// REPLACEMENT:
boosted_categories: crate::infra::config::default_boosted_categories_set(),
```

Note: `test_support.rs` builds service layers for integration tests. This is exactly the
"single source of truth for test defaults" scenario that `default_boosted_categories_set()`
is designed for.

### Site 5: `services/index_briefing.rs` (~line 627)

File: `crates/unimatrix-server/src/services/index_briefing.rs`

Context: Test code inside `#[cfg(test)]` module in `index_briefing.rs`.

```
// CURRENT:
HashSet::from(["lesson-learned".to_string()])

// REPLACEMENT:
crate::infra::config::default_boosted_categories_set()
```

### Site 6: `uds/listener.rs` (~line 2783)

File: `crates/unimatrix-server/src/uds/listener.rs`

Context: Test code inside the large `#[cfg(test)]` section of `listener.rs`.
This file is large; the literal is deep in the test section.

```
// CURRENT:
HashSet::from(["lesson-learned".to_string()])

// REPLACEMENT:
crate::infra::config::default_boosted_categories_set()
```

---

## Verification Steps (AC-19, AC-20)

After all 7 replacements (1 production + 6 test):

```bash
# AC-19: no literal in layer.rs
grep -n 'lesson-learned' crates/unimatrix-server/src/eval/profile/layer.rs
# Expected: zero hits

# AC-20: no HashSet literals in test infrastructure files
grep -rn 'HashSet::from.*lesson-learned' \
  crates/unimatrix-server/src/server.rs \
  crates/unimatrix-server/src/infra/shutdown.rs \
  crates/unimatrix-server/src/test_support.rs \
  crates/unimatrix-server/src/services/index_briefing.rs \
  crates/unimatrix-server/src/uds/listener.rs
# Expected: zero hits
```

---

## Sequencing Constraint

Part 2 (test infra literal removal) depends on Part 1 (config layer):
`default_boosted_categories_set()` must exist in `infra/config.rs` before the literal
replacement in test infrastructure files. This function is added as part of the config
component (Wave 1). The literal replacement is Wave 3.

Part 1 (`eval/profile/layer.rs`) also depends on Wave 1 because:
- After the `Default` change, `profile.config_overrides.knowledge.boosted_categories` may be `[]`
  if constructed via `Default` (see correctness check above).
- The wave ordering ensures the config component is in place before the eval fix is implemented.

---

## Error Handling

Neither change introduces error paths. Both are pure value transformations:
- `profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect()` is infallible.
- `default_boosted_categories_set()` returns a value directly, no I/O.

---

## Key Test Scenarios

### AC-19: layer.rs literal removed
```
test_layer_rs_boosted_categories_from_profile_config:
  // Construct EvalProfile with non-default boosted_categories
  profile = EvalProfile {
      config_overrides: UnimatrixConfig {
          knowledge: KnowledgeConfig {
              boosted_categories: vec!["pattern".to_string()],
              ..
          },
          ..
      },
      ..
  }
  // Run from_profile, assert the boosted_categories used is ["pattern"], not ["lesson-learned"]
  // This test does not exist if the layer builds a HashSet internally without exposing it,
  // but the intent is: after the fix, the eval path reads from config, not a literal.
```

### AC-20: no literal regressions
```
// Verified by grep (see verification steps above), not by a runtime test.
// Compile test: cargo build -p unimatrix-server succeeds after all 7 replacements.
```

### Regression: boosted categories still function correctly in briefing path
```
test_index_briefing_still_boosts_lesson_learned_by_default:
  // After the literal removal in index_briefing.rs, verify that
  // default_boosted_categories_set() returns the same value as the old literal.
  // This is implicitly covered by the test for default_boosted_categories_set()
  // in config.rs (see config pseudocode test section).
  set = crate::infra::config::default_boosted_categories_set()
  assert set.contains("lesson-learned")
  assert set.len() == 1
```
