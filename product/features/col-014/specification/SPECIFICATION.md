# col-014: Specification

## Function Specification: `is_valid_feature_id`

### Signature

```rust
fn is_valid_feature_id(s: &str) -> bool
```

### Behavior

Returns `true` if `s` is a plausible feature identifier suitable for attribution extraction.

### Validation Rules

1. **Non-empty**: `s.is_empty()` => `false`
2. **Max length**: `s.len() > 128` => `false`
3. **Contains hyphen**: `!s.contains('-')` => `false`
4. **Safe characters only**: Every character must be ASCII alphanumeric, hyphen (`-`), underscore (`_`), or dot (`.`). Any other character => `false`
5. **No leading/trailing hyphen**: `s.starts_with('-') || s.ends_with('-')` => `false` (prevents matching partial tokens from text splitting)

Rules are evaluated in short-circuit order for efficiency.

### Constants

```rust
const MAX_FEATURE_ID_LEN: usize = 128;
```

Consistent with `MAX_FEATURE_CYCLE_LEN` in server validation.

### Acceptance Criteria Traceability

| AC | Rule | Test |
|----|------|------|
| AC-1: `col-010b` accepted | Rules 1-5 pass | `test_is_valid_feature_id_suffixed` |
| AC-2: `col-002b` accepted | Rules 1-5 pass | `test_is_valid_feature_id_suffixed` |
| AC-3: `nxs-001` accepted | Rules 1-5 pass (regression) | `test_is_valid_feature_id_positive` (existing) |
| AC-4: `PROJ-123` accepted | Rules 1-5 pass | `test_is_valid_feature_id_domain_agnostic` |
| AC-5: `sprint-7-auth` accepted | Rules 1-5 pass (multi-hyphen) | `test_is_valid_feature_id_domain_agnostic` |
| AC-6: `v2.1-migration` accepted | Rules 1-5 pass (dot) | `test_is_valid_feature_id_domain_agnostic` |
| AC-7: `my_project-feat_1` accepted | Rules 1-5 pass (underscore) | `test_is_valid_feature_id_domain_agnostic` |
| AC-8: `""` rejected | Rule 1 | `test_is_valid_feature_id_negative` (existing) |
| AC-9: `nohyphen` rejected | Rule 3 | `test_is_valid_feature_id_no_hyphen` |
| AC-10: `a]b-c` rejected | Rule 4 | `test_is_valid_feature_id_special_chars` |
| AC-11: `a b-c` rejected | Rule 4 | `test_is_valid_feature_id_whitespace` |
| AC-12: 128/129 boundary | Rule 2 | `test_is_valid_feature_id_length_boundary` |
| AC-13: Existing IDs valid | Rules 1-5 pass | Existing tests (regression) |
| AC-14: E2E attribution | Full pipeline | `test_attribute_sessions_suffixed_feature` |

### Test Updates Required

**Existing tests to update**:
- `test_is_valid_feature_id_negative`: Remove `col-abc` assertion (now valid -- it contains a hyphen and safe chars). Keep empty, no-hyphen, leading/trailing hyphen assertions.

**New tests to add**:
- `test_is_valid_feature_id_suffixed`: `col-010b`, `col-002b`
- `test_is_valid_feature_id_domain_agnostic`: `PROJ-123`, `sprint-7-auth`, `v2.1-migration`, `my_project-feat_1`
- `test_is_valid_feature_id_no_hyphen`: `nohyphen`, `justletters`, `12345`
- `test_is_valid_feature_id_special_chars`: `a]b-c`, `feat<script>-1`, `col-001;drop`
- `test_is_valid_feature_id_whitespace`: `a b-c`, `col -001`
- `test_is_valid_feature_id_length_boundary`: 128-char (pass), 129-char (fail)
- `test_is_valid_feature_id_leading_trailing_hyphen`: `-abc`, `abc-`, `-`
- `test_attribute_sessions_suffixed_feature`: E2E with `col-010b` records

### Domain Model

No changes. `ObservationRecord`, `ParsedSession`, and `HookType` remain unchanged. The fix is purely in the validation predicate.

### Constraints

- Function remains private (`fn`, not `pub fn`)
- No new dependencies
- No changes to callers (`extract_from_path`, `extract_feature_id_pattern`, `extract_from_git_checkout`)
- No changes outside `crates/unimatrix-observe/src/attribution.rs`
