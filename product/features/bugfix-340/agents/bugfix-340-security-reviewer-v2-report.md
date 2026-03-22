# Security Review: bugfix-340-security-reviewer-v2

## Risk Level: low

## Summary

The expanded diff (v2) adds two security-adjacent gap fixes on top of the core `chars().count()` fix:
control-character rejection for the outcome field and an ASCII-only allowlist for phase/next_phase
fields. Both additions tighten security posture. No validation was loosened, no new dependencies
were introduced, and no hardcoded secrets are present. One informational finding on the U+007F (DEL)
boundary is documented below; it is consistent with the existing `check_control_chars` function and
is not a regression. No blocking findings.

---

## Findings

### Finding 1 — U+007F (DEL) not covered by outcome control-char check

- **Severity**: informational
- **Location**: `validation.rs:434` (outcome inline check)
- **Description**: The new inline check `(c as u32) <= 0x1F` covers the 32 C0 control characters
  (U+0000–U+001F) but does not cover U+007F (DEL, decimal 127), which is also defined as a control
  character in ISO 6429 / ASCII. DEL is above the 0x1F boundary and would pass this check. The
  pre-existing `check_control_chars` function (line 56) uses the same boundary (`code <= 0x1F`), so
  the gap is consistent across the codebase — this is not a regression introduced by the PR. DEL in
  a stored outcome value has no known injection or privilege-escalation vector in the current
  architecture (SQLite parameterized inserts, internal read-back only).
- **Recommendation**: No immediate action. If a future audit tightens control-character hygiene
  codebase-wide, both the inline outcome check and `check_control_chars` should be extended to
  `code <= 0x1F || code == 0x7F`. File as a separate low-priority issue if desired.
- **Blocking**: no

### Finding 2 — Outcome check does not use shared `check_control_chars` helper

- **Severity**: informational
- **Location**: `validation.rs:434`
- **Description**: The outcome control-char check is implemented inline rather than calling the
  existing `check_control_chars` helper at line 49. The inline check uses `(c as u32) <= 0x1F`
  while `check_control_chars` also uses `code <= 0x1F` — they are behaviorally identical (including
  the DEL gap noted in Finding 1). The inconsistency is cosmetic but means a future fix to
  `check_control_chars` would not automatically apply to the outcome field. The inline path also
  uses a plain `String` error (not `ServerError`) because `validate_cycle_params` returns
  `Result<_, String>`, which explains why the helper cannot be called directly here. This is a
  design constraint, not a security flaw.
- **Recommendation**: No action needed now. If `validate_cycle_params` is ever refactored to return
  `ServerError`, consolidate to the shared helper at that time.
- **Blocking**: no

### Finding 3 — Phase ASCII allowlist correctness

- **Severity**: informational (confirmed correct)
- **Location**: `validation.rs:468`
- **Description**: The allowlist `c.is_ascii_alphanumeric() || c == '-' || c == '_'` is a strict
  positive allowlist applied after `.to_lowercase()`. Every character that is not `[a-z0-9\-_]`
  (post-lowercase) is rejected. This correctly excludes: emoji, CJK, non-ASCII Latin, spaces,
  punctuation, and U+007F. The allowlist is more conservative than necessary (it would block
  uppercase letters, but `.to_lowercase()` is applied first, making that moot) and does not have
  the DEL gap issue. The allowlist is correct and complete for the intended domain (phase name
  slugs like "discovery", "phase-end", "delivery").
- **Recommendation**: No action required. Confirming correctness for the record.
- **Blocking**: no

### Finding 4 — Test name/assertion mismatch (cosmetic, pre-existing after gap-2 fix)

- **Severity**: informational
- **Location**: `validation.rs:1747` (`test_validate_phase_multibyte_at_max_passes`)
- **Description**: The test function name implies the operation should pass, but it asserts
  `is_err()`. This is documented in the gate-v2 report: the name reflects the original test intent
  (verifying that 64 emoji chars equals 64 char-count), which was valid before the ASCII allowlist
  was added. After the gap-2 fix, emoji are rejected by the allowlist, and the assertion was
  updated to `is_err()` without renaming the function. The test is correct; the name is misleading.
  This is not a security issue.
- **Recommendation**: Rename to `test_validate_phase_emoji_at_max_rejected_by_allowlist` in a
  future cleanup pass. Not blocking.
- **Blocking**: no

---

## OWASP Checklist

| Category | Assessment |
|---|---|
| Injection (SQL, command, path) | No new injection surface. All validation functions are pure (no I/O). Outputs flow into SQLite via parameterized queries in `unimatrix-store` (unchanged). |
| Broken access control | No change to trust/capability checks. `TrustLevel`, `Capability`, and `validate_enroll_params` are untouched. |
| Security misconfiguration | No configuration changes, no new feature flags, no new env vars. |
| Vulnerable components | No new dependencies. Cargo.toml is unchanged. No CVE exposure introduced. |
| Data integrity failures | Validation is tightened on two new fields (outcome, phase). No validation was removed. Characters that previously could have been stored (control chars in outcome, emoji in phase) are now rejected at the boundary. |
| Deserialization risks | No new deserialization paths. MCP parameter structs unchanged. |
| Input validation gaps | The two gap fixes close previously-identified asymmetries. The remaining gap (U+007F) is consistent with the pre-existing `check_control_chars` implementation and not introduced by this PR. |
| Secrets / credentials | No hardcoded secrets, tokens, API keys, or credentials anywhere in the diff. |
| Unsafe code | Zero `unsafe` blocks in `validation.rs`. Confirmed by gate-v2 report. |

---

## Blast Radius Assessment

Worst case if the gap-fix code has a subtle bug:

**Outcome control-char check (gap-1)**: If the check incorrectly passed a control character, the
worst outcome is a control character stored in the OUTCOME_INDEX `outcome` column. This value is
read back by the server's own query layer, not rendered to a browser. There is no known injection
or display-corruption vector at the current architecture layer. Blast radius: negligible.

**Phase ASCII allowlist (gap-2)**: If the allowlist had a logic error that accepted characters it
should reject (e.g., an off-by-one in the predicate), non-ASCII characters could reach the
`phase`/`next_phase` columns. These values are stored in OUTCOME_INDEX and used in query filters.
Again, no SQL injection risk (parameterized queries), no rendering path. Blast radius: negligible.

**False-negative risk (allowlist too strict)**: If the allowlist incorrectly rejects valid phase
names, agents calling `context_cycle` with well-formed phase slugs would receive validation errors.
This is a denial-of-service on the tool call, not a security risk. Given the allowlist is a simple
`[a-z0-9\-_]` positive match and the test suite covers "discovery" and "phase-end" as passing
cases, this risk is low.

---

## Regression Risk

Low. The change set is additive: two new rejection rules (control chars in outcome, non-ASCII in
phase) and no removed validation. The only regression vector is:

- Agents currently sending phase names with non-ASCII characters would now receive errors. Given
  that all existing protocol-defined phase names ("start", "phase-end", "stop", "discovery",
  "delivery") are ASCII slug-safe, and the gate-v2 report confirms all 144 integration tests pass,
  this regression risk is negligible in practice.

- The `chars().count()` substitution is regression-safe for all-ASCII inputs (byte count equals
  char count for ASCII). The 14 new tests exercise multibyte boundary conditions specifically.

---

## PR Comments

- Posted 1 comment on PR #343 (informational summary).
- Blocking findings: no.

---

## Knowledge Stewardship

- nothing novel to store -- the DEL gap (U+007F) in control-character checks is a known
  OWASP input-validation nuance, but it is consistent with the pre-existing `check_control_chars`
  implementation and does not represent a new pattern introduced by this PR. The generalizable
  anti-pattern ("use chars().count() not len() for character limits") was already stored as entries
  #3103 and #3105. No additional cross-feature pattern identified.
