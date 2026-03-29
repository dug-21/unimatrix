# Test Plan: main.rs (startup wiring) + main_tests.rs

Component from IMPLEMENTATION-BRIEF.md §Component Map row 3.

---

## Risks Addressed

- **R-11** (Critical): `test_default_config_boosted_categories_is_lesson_learned` must be
  rewritten to cover the serde path, not the `Default` path.
- **I-01** (High): Both `CategoryAllowlist` construction call sites in `main.rs` must be
  updated to use `from_categories_with_policy`.

---

## main.rs Changes — Compile-Only Verification

`main.rs` changes are primarily verified at compile time. The `cargo build` or
`cargo check` gates catch all four signature changes (two `CategoryAllowlist` sites,
two `ServiceLayer::new()` sites, two `spawn_background_tick` sites).

**Verification step**: After implementing all components:
```bash
cargo check -p unimatrix-server 2>&1 | tail -5
```
Must exit 0. Any remaining errors indicate a wiring site was missed.

**Note on I-01**: Both `from_categories_with_policy` call sites in `main.rs` must read
`config.knowledge.adaptive_categories` — not `vec!["lesson-learned"]` hardcoded. The
tester must grep to confirm this:

```bash
grep -n "from_categories" crates/unimatrix-server/src/main.rs
```

Expected: two hits, each passing `config.knowledge.adaptive_categories` (or equivalent
variable) as the second argument. A hit showing `vec!["lesson-learned"]` as the second
argument indicates I-01 was not properly resolved.

---

## main_tests.rs: Test Rewrite (AC-18, R-11)

### Test to Rewrite: `test_default_config_boosted_categories_is_lesson_learned`

**Current behavior** (lines 393-404): calls `UnimatrixConfig::default()` and asserts
`boosted_categories == ["lesson-learned"]`. After the Default impl change this test fails
with an opaque assertion error.

**Required rewrite** (AC-18):

```rust
/// AC-18: Serde deserialization default for boosted_categories is ["lesson-learned"].
/// The Default impl returns vec![] (programmatic use); the serde default applies to
/// config files that omit the field.
#[test]
fn test_serde_default_boosted_categories_is_lesson_learned() {
    use unimatrix_server::infra::config::UnimatrixConfig;

    let config: UnimatrixConfig = toml::from_str("").unwrap();
    assert_eq!(
        config.knowledge.boosted_categories,
        vec!["lesson-learned".to_string()],
        "Serde default for boosted_categories must be ['lesson-learned'] when field is omitted"
    );
}
```

Key changes:
- Test name updated to reflect the serde invariant (not `Default`)
- Comment updated to document the Default/serde split
- `UnimatrixConfig::default()` replaced by `toml::from_str("")`
- The assertion stays the same — but now tests the right path

**Companion test** (AC-18, also covers R-11 regression guard):

```rust
/// Serde default for adaptive_categories is ["lesson-learned"] when field is omitted.
#[test]
fn test_serde_default_adaptive_categories_is_lesson_learned() {
    use unimatrix_server::infra::config::UnimatrixConfig;

    let config: UnimatrixConfig = toml::from_str("").unwrap();
    assert_eq!(
        config.knowledge.adaptive_categories,
        vec!["lesson-learned".to_string()],
        "Serde default for adaptive_categories must be ['lesson-learned'] when field is omitted"
    );
}
```

---

## Assertions Summary

| Assertion | Test | AC |
|-----------|------|----|
| `toml::from_str("").knowledge.boosted_categories == ["lesson-learned"]` | `test_serde_default_boosted_categories_is_lesson_learned` | AC-18 |
| `toml::from_str("").knowledge.adaptive_categories == ["lesson-learned"]` | `test_serde_default_adaptive_categories_is_lesson_learned` | (companion) |
| `cargo check` exits 0 after all 6 main.rs sites updated | compile gate | I-01 |
| `grep -n "from_categories" main.rs` shows operator-configured adaptive arg | grep step | I-01 |

---

## Integration Test Expectations

`main.rs` changes are not directly testable through the MCP interface beyond the smoke gate:
the server starting successfully with the default config is sufficient evidence that the
wiring is correct. The `test_status_category_lifecycle_field_present` integration test
(planned in OVERVIEW.md) validates that the wired `CategoryAllowlist` reaches
`StatusService` and produces correct output, covering the end-to-end wiring.
