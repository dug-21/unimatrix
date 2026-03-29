# Test Plan: search-comment

## Component: `src/services/search.rs` — FusionWeights Field Comment

### Risks Covered

- R-03 (High): Intentional `FusionWeights { w_coac: 0.10, ... }` test fixtures in search.rs unchanged
- R-04 (Medium): `default 0.10` comment on FusionWeights.w_coac updated
- R-05 (Medium): `CO_ACCESS_STALENESS_SECONDS` unchanged in search.rs
- R-06 (Medium): `compute_search_boost` function and call site present

---

## Unit Test Expectations

### Comment Verification (R-04)

After delivery:

| Check | Pattern | Expected |
|-------|---------|---------|
| Old comment gone | `default 0\.10` on `w_coac` line in FusionWeights | Zero matches |
| New comment present | `default 0\.0.*crt-032` in search.rs | 1 match on w_coac line |

---

### Fixture Count Verification (R-03)

**Before delivery**: Count `FusionWeights { w_coac: 0\.10` (or equivalent) in search.rs test section.
**After delivery**: Count must be identical.

These are intentional scoring-math test inputs. The comment change on line ~118 must NOT affect any of these literals.

---

### Non-Removal Verification (R-05, R-06)

| Check | Pattern | Expected |
|-------|---------|---------|
| compute_search_boost defined | `fn compute_search_boost` | Present |
| compute_briefing_boost defined | `fn compute_briefing_boost` | Present |
| compute_search_boost called | `compute_search_boost(` in search.rs | Present (1+ call sites) |
| CO_ACCESS_STALENESS_SECONDS in search.rs | `CO_ACCESS_STALENESS_SECONDS` | Present in search.rs (1 reference) |

---

## Edge Cases

- The comment update is on the struct field definition, not on test struct literals. Test struct literals do not have this comment. There is no risk of accidentally changing them when editing the field comment.
- Only line ~118 (field definition comment) changes. All other lines in search.rs are unchanged.
