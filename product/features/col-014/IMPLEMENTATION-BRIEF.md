# col-014: Implementation Brief

## Summary

Replace rigid `{alpha}-{digits}` validation in `is_valid_feature_id()` with permissive character/length safety gating. Single file change in `crates/unimatrix-observe/src/attribution.rs`.

## Change Specification

### File: `crates/unimatrix-observe/src/attribution.rs`

**Replace** lines 5-15 (`is_valid_feature_id` function):

```rust
const MAX_FEATURE_ID_LEN: usize = 128;

/// Check if a string is a plausible feature ID.
///
/// Permissive safety gating: non-empty, reasonable length, contains a hyphen,
/// only safe characters (ASCII alphanumeric, hyphen, underscore, dot).
/// No leading/trailing hyphens.
///
/// Unimatrix is domain-agnostic (ASS-009) -- feature ID format is a
/// project-level concern, not an engine-level concern.
fn is_valid_feature_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_FEATURE_ID_LEN
        && s.contains('-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}
```

### Test Updates

**Update** `test_is_valid_feature_id_negative`:
- Remove `assert!(!is_valid_feature_id("col-abc"))` -- now valid under permissive rules
- Keep: empty, `col-`, `-002` assertions
- Add: `assert!(!is_valid_feature_id("nohyphen"))`

**Add** new test functions as specified in SPECIFICATION.md (see AC traceability table).

## Implementation Order

1. Add `MAX_FEATURE_ID_LEN` constant
2. Replace `is_valid_feature_id` function body
3. Update `test_is_valid_feature_id_negative`
4. Add new test functions
5. Run `cargo test -p unimatrix-observe` to verify

## Estimated Effort

~30 minutes. Single function replacement + test updates. No cross-crate coordination needed.

## Dependencies

None. No schema changes, no API changes, no configuration changes.

## GH Issue

#79
