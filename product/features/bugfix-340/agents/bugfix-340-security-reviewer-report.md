# Security Review: bugfix-340-security-reviewer

## Risk Level: low

## Summary

PR #343 replaces `s.len()` (byte count) with `s.chars().count()` (Unicode scalar count) at three
call sites in `crates/unimatrix-server/src/infra/validation.rs`. The change fixes false rejections
of multibyte UTF-8 input (emoji, CJK) that were within the documented character limit. No new
dependencies were introduced, no validation was removed, and the fix is strictly minimal (3
one-line substitutions + 90 lines of new tests). No blocking findings.

---

## Findings

### Finding 1 — `.len()` retained at `tags.len()` (line 139) and `is_valid_feature_id` (line 369)

- **Severity**: informational
- **Location**: `validation.rs:139`, `validation.rs:369`
- **Description**: Two remaining `.len()` calls were not modified by this PR. At line 139,
  `tags.len()` counts elements of a `Vec<String>`, not string bytes — this is semantically correct
  and unrelated to the bug. At line 369, `s.len() <= MAX_CYCLE_TOPIC_LEN` is inside
  `is_valid_feature_id`, which enforces `is_ascii_alphanumeric() || '-' || '_' || '.'` on every
  character via the predicate immediately below. For ASCII-only strings `len()` equals
  `chars().count()`, so this is correct as-is.
- **Recommendation**: No action required. The gate report correctly documents this rationale.
- **Blocking**: no

### Finding 2 — Memory cost of `chars().count()` on large inputs

- **Severity**: informational
- **Location**: `validation.rs:40` (`check_length`)
- **Description**: `chars().count()` is O(n) in string length, traversing every byte to count
  scalar values. `s.len()` was O(1). For the largest constant in the file (`MAX_CONTENT_LEN =
  50_000` characters), a maximally-dense multibyte input could be 200 KB (4 bytes per char). This
  is processed synchronously in the validation layer on every store/correct request. At current
  usage scale (single MCP server, trusted-agent callers) this is negligible. If the server is ever
  exposed to untrusted high-volume traffic, this path could contribute to CPU amplification. This
  is a pre-existing architectural consideration, not introduced by the fix.
- **Recommendation**: No immediate action. If the server is ever exposed to untrusted callers,
  consider an early byte-length guard (e.g., reject if `value.len() > max * 4`) before calling
  `chars().count()` to bound the traversal cost. Not warranted now.
- **Blocking**: no

### Finding 3 — Outcome field: no control-character check (pre-existing)

- **Severity**: informational
- **Location**: `validation.rs:428-436` (`validate_cycle_params`, outcome arm)
- **Description**: The outcome field in `validate_cycle_params` only checks length; it does not
  call `check_control_chars`. In contrast, the `check_length` → `validate_string_field` path used
  for general fields does include `check_control_chars`. This asymmetry is pre-existing and not
  introduced by this PR. The fix correctly applies the same check (length only, via
  `s.chars().count()`) that existed before. The blast radius of this pre-existing gap is limited:
  outcome is stored as a structured tag value in `OUTCOME_INDEX`, read back only by the server
  itself.
- **Recommendation**: Track as a separate issue if control-character hygiene is required for
  outcome values. Out of scope for this bugfix.
- **Blocking**: no

---

## OWASP Checklist

| Category | Assessment |
|---|---|
| Injection (SQL, command, path) | No new injection surface. Validation functions are pure (no I/O). Outputs flow into SQLite via parameterized queries in the store crate (not changed here). |
| Broken access control | No change to trust/capability checks. Access control layer (`TrustLevel`, `Capability`) untouched. |
| Security misconfiguration | No configuration changes. No new feature flags, env vars, or server settings. |
| Vulnerable components | No new dependencies. Cargo.toml diff is empty. No CVE exposure introduced. |
| Data integrity failures | Fix narrows validation (fewer false rejections), does not loosen structural constraints. Existing content stored under old `.len()` semantics is ASCII-safe and remains valid. |
| Deserialization risks | No new deserialization paths. MCP parameter structs unchanged. |
| Input validation gaps | Validation is tightened correctly. Multibyte inputs that previously were incorrectly rejected are now correctly handled. The validation logic does not accept longer inputs — only correctly measures declared character limits. |
| Secrets / credentials | No hardcoded secrets, tokens, or API keys in the diff. |
| Unsafe code | Zero `unsafe` blocks in `validation.rs` (confirmed by gate report and grep). |

---

## Blast Radius Assessment

Worst case if the fix has a subtle bug:

- `chars().count()` is part of Rust's standard library and is not fallible for valid UTF-8. Rust
  strings are always valid UTF-8 by construction, so the traversal cannot panic.
- If `chars().count()` returned an incorrect value (hypothetically), inputs of legal character
  length could be accepted when they should be rejected (length enforcement failure), or correctly
  sized inputs could be rejected (denial of service for valid input). The former would allow
  oversized content to reach the SQLite store; the existing store layer has no secondary size
  enforcement on these fields. However, oversized content in an internal knowledge store does not
  enable privilege escalation or code execution.
- The blast radius is bounded: worst-case outcome is data quality degradation (oversized tag values
  stored), not data corruption, information disclosure, or privilege escalation.
- Failure mode: safe (error returned or oversized string stored silently). Not silent data
  corruption of existing records.

---

## Regression Risk

Low. The change affects 3 call sites, all in the validation layer:

1. `check_length` (general-purpose, called from `validate_string_field`) — behavior is identical
   for all-ASCII inputs (which covers all existing well-formed inputs from English-language agents).
   Multibyte inputs that were incorrectly rejected before are now accepted. This is the intended
   regression fix, not a regression introduced.

2. `validate_cycle_params` outcome — same analysis as above.

3. `validate_phase_field` phase/next_phase — same analysis. Note: the test
   `test_validate_phase_multibyte_at_max_passes` accepts emoji as valid phase names, which is
   intentional per the pre-existing character allowlist policy (any non-space, non-control
   character). The fix does not change what characters are accepted, only how many characters are
   counted.

Existing tests: 1915 passed, 0 failed (gate report). No xfail markers introduced. Risk of
regression to existing ASCII-only callers: negligible.

---

## PR Comments

- Posted 1 comment on PR #343 (informational, see below).
- Blocking findings: no.

---

## Knowledge Stewardship

- nothing novel to store — the generalizable anti-pattern ("use `chars().count()` not `len()` for
  character limits") was already captured in Unimatrix entries #3103 and #3105 by the investigator
  and rust-dev agents. No additional cross-feature pattern identified.
