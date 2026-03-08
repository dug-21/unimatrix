# col-014: Feature Attribution Fix

## Problem Statement

`is_valid_feature_id()` in `crates/unimatrix-observe/src/attribution.rs:6-15` enforces a rigid `{alpha}-{digits}` format, silently rejecting feature IDs with letter suffixes like `col-010b` and `col-002b`. This causes `context_retrospective(feature_cycle: "col-010b")` to return "No observation data found" even though JSONL files contain col-010b references.

More fundamentally, Unimatrix is domain-agnostic (ASS-009). Enforcing a specific feature ID convention at the engine level is architecturally wrong. Feature ID format is a project-level concern, not an engine-level concern.

## Root Cause

```rust
fn is_valid_feature_id(s: &str) -> bool {
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 { return false; }
    !parts[0].is_empty()
        && parts[0].chars().all(|c| c.is_ascii_alphabetic())
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_ascii_digit())  // rejects "010b"
}
```

The function imposes structural constraints (`{alpha}-{digits}`) that belong to project convention, not to a domain-agnostic engine. This rejects valid feature IDs like `col-010b`, `PROJ-123`, `sprint-7-auth`, `v2.1-migration`.

## Scope

### In Scope

1. **Replace rigid format validation with permissive character/length gating** in `is_valid_feature_id()`
2. **Update existing tests** to reflect new permissive validation
3. **Add new tests** for the safety boundary (empty, too long, control chars, whitespace)

### Out of Scope

- Canonical validator in unimatrix-core (no cross-crate coupling needed)
- Server-side `validate_retrospective_params` changes (already permissive)
- Changes to any other crate

## Decision: Option 3 (revised) -- Permissive safety gating

Per issue #79 and human review, the revised direction is:

- **Remove structural format validation** (`{alpha}-{digits}` pattern)
- **Replace with permissive safety guards**:
  - Non-empty
  - Reasonable max length (128 chars, consistent with `MAX_FEATURE_CYCLE_LEN` in server validation)
  - Contains at least one hyphen (distinguishes feature IDs from plain words in free text extraction)
  - Only safe characters: ASCII alphanumeric, hyphens, underscores, dots
  - No control characters, no whitespace, no special characters

The hyphen requirement is the minimal structural constraint needed for attribution's text extraction: without it, `extract_feature_id_pattern` would match arbitrary single words from free text, producing excessive false positives.

**Why not Option 1 (fix suffix only)**: Still encodes project-specific convention. Breaks for `sprint-7-auth`, `v2.1-migration`, etc.

**Why not Option 2 (canonical validator)**: Cross-crate coupling for a private function is overengineering.

## Acceptance Criteria

1. `is_valid_feature_id("col-010b")` returns `true`
2. `is_valid_feature_id("col-002b")` returns `true`
3. `is_valid_feature_id("nxs-001")` returns `true` (regression)
4. `is_valid_feature_id("PROJ-123")` returns `true` (domain-agnostic)
5. `is_valid_feature_id("sprint-7-auth")` returns `true` (multi-hyphen)
6. `is_valid_feature_id("v2.1-migration")` returns `true` (dots allowed)
7. `is_valid_feature_id("my_project-feat_1")` returns `true` (underscores allowed)
8. `is_valid_feature_id("")` returns `false` (empty)
9. `is_valid_feature_id("nohyphen")` returns `false` (no hyphen)
10. `is_valid_feature_id("a]b-c")` returns `false` (special chars)
11. `is_valid_feature_id("a b-c")` returns `false` (whitespace)
12. 128-char ID accepted, 129-char ID rejected
13. All existing attribution integration tests continue to pass (existing IDs like `col-002`, `nxs-001`, `eng-001` remain valid)
14. End-to-end: `attribute_sessions` correctly attributes records referencing `col-010b`

## Affected Files

- `crates/unimatrix-observe/src/attribution.rs` (fix + tests)

## Risk Assessment

- **Low risk**: Single private function, comprehensive existing test suite
- **False positive risk**: Mitigated by retaining hyphen requirement -- plain English words won't match
- **Regression risk**: All previously-valid IDs remain valid (relaxing validation only adds matches)
- **Consistency**: Aligns with server's `MAX_FEATURE_CYCLE_LEN = 128` and permissive `validate_retrospective_params`
