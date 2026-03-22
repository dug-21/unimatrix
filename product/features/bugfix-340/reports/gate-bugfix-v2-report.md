# Gate Bugfix Report (v2): bugfix-340

> Gate: Bugfix Validation (re-run after scope expansion)
> Date: 2026-03-22
> Agent: 340-gate-bugfix-v2
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | All three `s.len()` → `s.chars().count()` substitutions present at lines 40, 431, 462 |
| Gap 1 (outcome control chars) | PASS | Line 434: rejects all codepoints <= U+001F including newline |
| Gap 2 (phase ASCII allowlist) | PASS | Line 468: `is_ascii_alphanumeric() \|\| '-' \|\| '_'` — emoji rejected |
| Remaining `s.len()` use correct | PASS | Line 369 in `is_valid_feature_id` operates on ASCII-filtered string only |
| No stubs / placeholders | PASS | Zero `todo!()`, `unimplemented!()`, TODO, FIXME in production code |
| `unwrap()` in non-test code | PASS | All `unwrap()` calls inside `#[cfg(test)]` |
| No unsafe code | PASS | Zero `unsafe` blocks in `validation.rs` |
| Tests pass — full workspace | PASS | All test bins pass; 27 ignored are pre-existing |
| New tests — Phase 1 (multibyte) | PASS | 8 tests at lines 1705–1783 all pass |
| New tests — Phase 2 (gaps) | PASS | 6 tests at lines 1788–1855 all pass |
| Test semantics correct | PASS | `test_validate_phase_multibyte_at_max_passes` asserts `is_err()` — emoji correctly rejected |
| No new clippy warnings | PASS | 0 errors/warnings in unimatrix-server; unimatrix-store pre-existing tracked GH#342 |
| Fix is minimal | PASS | Only `validation.rs` modified; changes targeted to diagnosed sites |
| New tests catch original bugs | PASS | Regression tests present for all three fix sites |
| Integration smoke tests | PASS | 20/20 per tester report; 144 integration tests passed, 2 pre-existing xfails |
| File size | WARN | 1856 lines — pre-existing condition, not introduced by this fix |
| Investigator stewardship | PASS | Queried: #604, #308; Stored: #3103 |
| Rust-dev (fix) stewardship | PASS | Queried: #3103, #604, #308; Stored: #3105 |
| Rust-dev (gaps) stewardship | PASS | Queried: #3103, #3105, #604, #308; Stored: nothing novel |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

Three `s.len()` → `s.chars().count()` substitutions confirmed in production code:

- Line 40 (`check_length`): `value.chars().count() > max`
- Line 431 (`validate_cycle_params`, outcome): `s.chars().count() > MAX_OUTCOME_LEN`
- Line 462 (`validate_phase_field`): `normalized.chars().count() > MAX_PHASE_LEN`

Line 369 (`is_valid_feature_id`) correctly retains `s.len()` — the function operates on strings already filtered to ASCII-only characters, making byte count and character count identical.

### Gap 1 Fix: Outcome Control-Character Rejection

**Status**: PASS

Line 434: `if s.chars().any(|c| (c as u32) <= 0x1F)` — rejects all control characters including newline (U+000A) and tab (U+0009). This is more restrictive than `validate_string_field`'s `allow_newline_tab=true` path, which is correct for structured outcome values.

Tests confirming the fix:
- `test_validate_cycle_outcome_control_char_x01_rejected` (line 1788)
- `test_validate_cycle_outcome_newline_rejected` (line 1804)

### Gap 2 Fix: Phase ASCII-Only Allowlist

**Status**: PASS

Line 468: `!normalized.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')` — rejects any character not in `[a-z0-9\-_]` (after lowercase normalization). Emoji and other non-ASCII are rejected.

Tests confirming the fix:
- `test_validate_phase_emoji_prefix_rejected` (line 1822)
- `test_validate_phase_discovery_slug_passes` (line 1833) — valid slug passes
- `test_validate_phase_hyphen_slug_passes` (line 1840) — hyphenated slug passes
- `test_validate_next_phase_emoji_rejected` (line 1847)

### Test Semantics Note

The test `test_validate_phase_multibyte_at_max_passes` (line 1747) has a name that references "multibyte at max passes" but asserts `is_err()`. The name reflects what the ORIGINAL test was checking (that 64 emoji chars == 64 char-count, so should pass). With the gap-2 allowlist fix, emoji are now rejected for a different reason. The test name is slightly misleading but the assertion is accurate and the test correctly exercises the post-fix behavior.

### Knowledge Stewardship

All three agent stewardship blocks present and complete:
- Investigator: Queried #604, #308; Stored #3103 ("str::len() Returns Bytes, Not Chars — Use chars().count() for Character Limits")
- Rust-dev (fix): Queried #3103, #604, #308; Stored #3105 ("Use chars().count() not len() for character limits in validation.rs")
- Rust-dev (gaps): Queried #3103, #3105, #604, #308; Stored: nothing novel to store — cross-feature patterns already captured

### File Size

**Status**: WARN

`validation.rs` is 1856 lines, exceeding the 500-line gate guideline. This is a pre-existing condition (1775 lines before Phase 2 additions; the file was already over-limit before this bugfix). The Phase 2 additions (81 lines) are all test code. This is tracked separately and does not block this fix.

### Security Reviewer Report vs Final Code

The security reviewer report under `bugfix-340/agents/bugfix-340-security-reviewer-report.md` was written after Phase 1 but before Phase 2. Its Finding 3 ("outcome field: no control-character check") and comment that emoji phase names are accepted describe the pre-Phase-2 state. Both gaps are now closed in the committed code. The report remains accurate as a historical review document.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store -- single-feature gate re-validation; patterns already captured in #3103 and #3105. Security reviewer report staleness (written before gap fixes) is not a recurring cross-feature pattern.
