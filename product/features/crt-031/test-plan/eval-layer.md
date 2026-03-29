# Test Plan: eval/profile/layer.rs + six test infrastructure literal removal sites

Component from IMPLEMENTATION-BRIEF.md §Component Map rows 6 (eval-layer) and
implicit (test-infra literal removal, FR-16).

---

## Risks Addressed

- **R-09** (Low): `server.rs` default init silent policy — `default_boosted_categories_set()`
  must produce the same value as the old literal.

---

## Verification Strategy

This component has no net behavior change. All six test-infrastructure literal sites
continue to produce `{"lesson-learned"}` — the same set as before. The eval layer fix
is also a single-line substitution with no behavior change. Tests here are primarily
**grep verifications** that the literals are gone and the helper is correct.

---

## Pre-Verification Grep: Confirm Literals Removed (AC-19, AC-20)

Run these after implementation before gate 3c:

### AC-19: eval/profile/layer.rs

```bash
grep -n 'lesson-learned' crates/unimatrix-server/src/eval/profile/layer.rs
```

Expected: zero hits. Any hit indicates the literal was not replaced.

### AC-20: Six test infrastructure sites

```bash
grep -rn 'HashSet::from.*lesson-learned' \
  crates/unimatrix-server/src/server.rs \
  crates/unimatrix-server/src/infra/shutdown.rs \
  crates/unimatrix-server/src/test_support.rs \
  crates/unimatrix-server/src/services/index_briefing.rs \
  crates/unimatrix-server/src/uds/listener.rs
```

Expected: zero hits across all five files. Both occurrences in `shutdown.rs` (~lines 308
and 408) must be replaced.

---

## Unit Test Expectations

### default_boosted_categories_set() correctness

**`test_default_boosted_categories_set_is_lesson_learned`**
- Location: `infra/config.rs` tests
- Assert: `default_boosted_categories_set() == HashSet::from(["lesson-learned".to_string()])`
- Assert: `default_boosted_categories_set().len() == 1`
- Rationale: all 6 sites that previously had `HashSet::from(["lesson-learned"...])` now call
  this helper. If the helper returns the wrong value, all 6 sites are broken simultaneously.

### eval/profile/layer.rs: no literal regression test needed

The `eval/profile/layer.rs` change is:
- Before: `HashSet::from(["lesson-learned".to_string()])`
- After: `profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect()`

The behavior is identical for any caller that passes the default `UnimatrixConfig` (serde
default `boosted_categories = ["lesson-learned"]`). No existing test asserts on the literal
value directly — the AC-19 grep is sufficient verification.

### Regression: cargo test --workspace (AC-23)

After removing all six literals:
```bash
cargo test --workspace 2>&1 | tail -30
```

Must exit 0. Any failure indicates a literal removal site broke an adjacent test.
If a test was relying on `HashSet::from(["lesson-learned"...])` being a specific expression
(e.g., a helper that constructed a `HashSet` inline for a comparison), the test must be
updated to use `default_boosted_categories_set()` for the comparison.

---

## Circular Dependency Verification

Before implementation, verify `infra/config.rs` can be imported from all 7 sites without
a circular dependency:

```bash
grep -rn "use crate::infra::config" \
  crates/unimatrix-server/src/server.rs \
  crates/unimatrix-server/src/infra/shutdown.rs \
  crates/unimatrix-server/src/test_support.rs \
  crates/unimatrix-server/src/services/index_briefing.rs \
  crates/unimatrix-server/src/uds/listener.rs \
  crates/unimatrix-server/src/eval/profile/layer.rs
```

All 6 files (7 literal sites, but `eval/profile/layer.rs` uses the config field directly)
should already import from `crate::infra::config` or can do so without a circular dep.
`cargo check` after the changes is the definitive verification.

---

## Assertions Summary

| Assertion | Method | AC |
|-----------|--------|----|
| `grep 'lesson-learned' eval/profile/layer.rs` returns 0 hits | grep | AC-19 |
| `grep 'HashSet::from.*lesson-learned'` across 5 files returns 0 hits | grep | AC-20 |
| `default_boosted_categories_set().contains("lesson-learned")` | unit test | — |
| `cargo test --workspace` exits 0 | shell | AC-23 |

---

## Integration Test Expectations

The eval harness literal fix is not directly visible through the MCP interface in isolation
(boosted_categories values are the same before and after — just sourced from config instead
of a literal). The smoke gate is sufficient for this component.
