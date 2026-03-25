# Security Review: col-027-security-reviewer

## Risk Level: low

## Summary

col-027 adds `PostToolUseFailure` hook registration and an observation/detection pipeline to fix a long-standing false-positive in `PermissionRetriesRule`. The diff is a targeted, additive change with strong defensive coding patterns. No blocking security findings were identified. One low-severity observation regarding the hook binary path hardcoding is noted for awareness.

---

## Findings

### Finding 1: Hook binary path hardcoded to dev workspace
- **Severity**: low
- **Location**: `.claude/settings.json` line 64
- **Description**: The `command` field is `"/workspaces/unimatrix/target/release/unimatrix hook PostToolUseFailure"`. This is consistent with all other hook registrations in the same file (`PreToolUse`, `PostToolUse`, etc.) and is an established pattern in this project. The path assumes the binary is always built and present at the release target path. This is not a new risk introduced by col-027 — it matches the existing hook registration pattern exactly (same binary, same path prefix, different subcommand argument).
- **Recommendation**: Acceptable as-is for a dev-environment tool. If hooks are ever ported to a shared/multi-user environment, path parameterisation should be considered. No action required for this PR.
- **Blocking**: no

### Finding 2: Untrusted `payload["error"]` stored as response_snippet (acknowledged in RISK-TEST-STRATEGY.md)
- **Severity**: low
- **Location**: `listener.rs` `extract_error_field()`, line ~2682
- **Description**: The `error` field from `PostToolUseFailure` payloads originates from Claude Code and may reflect attacker-controlled content (e.g., a path like `../../etc/passwd` in a "file not found" message, or shell stderr injection artifacts from Bash failures). The error string is stored in `response_snippet`. The risk is already well-documented in RISK-TEST-STRATEGY.md under "Security Risks" and was explicitly evaluated by the design pipeline.
  - Mitigation is in place: `as_str()` rejects non-string types; `truncate_at_utf8_boundary(error_str, 500)` enforces a hard 500-byte cap; SQLite storage uses prepared statement parameters (no SQL injection path). The value is stored only for diagnostic display in retrospective evidence records.
  - No JSON depth limit guard specific to this payload was found in `hook.rs` or `listener.rs`. However, `payload["error"]` is accessed as a plain string via `.as_str()` — serde_json's `as_str()` on a JSON Value performs a single-field type check without recursive traversal, so deep nesting in `error` is not a concern. The `tool_input` field accessed via `serde_json::to_string()` could theoretically be a deeply nested object, but this is the same code path used for `PreToolUse` and `PostToolUse` with no incidents.
- **Recommendation**: No change required. The 500-byte truncation and `as_str()` type guard are sufficient for the current threat model. If the retrospective evidence display layer ever renders snippets as HTML, an escaping pass should be added at that layer (not at storage time).
- **Blocking**: no

### Finding 3: settings.json command argument not shell-escaped (shared with all existing hooks)
- **Severity**: low
- **Location**: `.claude/settings.json` line 64
- **Description**: The hook command passes `PostToolUseFailure` as a positional argument to the `unimatrix` binary. The payload data is passed via stdin (not as a shell argument), so there is no shell-injection risk from the event name or payload content. The event name `PostToolUseFailure` is a compile-time literal, not user-controlled. This is not a new risk.
- **Recommendation**: No change required.
- **Blocking**: no

---

## OWASP Check Summary

| OWASP Category | Assessment |
|----------------|-----------|
| A03 Injection | No SQL injection: all DB writes use prepared statements. No command injection: payload is passed via stdin, not as shell arguments. No path traversal: `payload["error"]` is stored as a string, not used in file operations. |
| A01 Broken Access Control | No access control changes. `PostToolUseFailure` events follow the same fire-and-forget UDS path as all other observation events. No new privilege paths introduced. |
| A08 Software/Data Integrity | Event type `"PostToolUseFailure"` stored verbatim (ADR-003 — no normalization). This is intentional and correct: detection rules filter by exact string equality. No integrity concern. |
| A04 Insecure Design | Two-site differential fix (`friction.rs` and `metrics.rs`) is atomic in a single commit. ADR-004 required this. Verified: both changes are present in the same diff. |
| A05 Security Misconfiguration | Hook registration key `"PostToolUseFailure"` uses correct exact casing matching Claude Code documentation. Matcher `"*"` is consistent with `PreToolUse` and `PostToolUse` entries. |
| Deserialization | `parse_hook_input()` uses `serde_json::from_str::<HookInput>()` with an error fallback (ADR-006 defensive parsing). Malformed JSON returns a default `HookInput` — hook exits 0. No panic path on deserialization failure. |
| Input Validation | `as_str()` type guard on `payload["error"]`. Empty string guard (`!s.is_empty()`). 500-byte truncation. All present and correct. |
| Secrets | No hardcoded secrets, API keys, tokens, or credentials found in the diff. |

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

- `ToolFailureRule` produces a false finding: a tool appears to have failed > 3 times when it did not. Impact: a misleading retrospective finding for a feature cycle. No data corruption, no data loss, no privilege escalation. The finding would be surfaced in `context_retrospective` output and could prompt an unnecessary allowlist change recommendation. Severity: low operational impact.

- `PermissionRetriesRule` still fires for balanced failure sessions: the `terminal_counts` change is correct and tested extensively. If a regression existed, existing test T-FM-01/02 would catch it. The worst case is the status quo ante — false `permission_retries` findings continue, which is the bug being fixed.

- `compute_universal()` returns wrong `permission_friction_events`: the `terminal_counts` change mirrors `friction.rs`. The two-site integration tests (T-FM-08/09/10) run both sites on the same fixture, so a divergence would be caught at test time. No production data corruption path.

- Hook binary exits non-zero on malformed payload: explicit tests (T-HD-02, T-HD-03, T-HD-04, T-HD-extra) verify empty, null, and missing fields produce `RecordEvent` without panic. The fire-and-forget path means a transient error only loses one observation record, not the session.

**No blast radius path leads to data corruption, privilege escalation, or service disruption.**

---

## Regression Risk

**Targeted regressions checked:**

1. `PermissionRetriesRule` — Variable renamed from `post_counts` to `terminal_counts`; additional `elif` branch added. All existing `PermissionRetriesRule` tests pass (verified: `cargo test` green). The rename is purely internal; the rule's external name, finding category, and output format are unchanged.

2. `compute_universal()` — Same pattern: `post_counts` → `terminal_counts`, additional elif branch. Existing metrics tests pass. The `pre_counts` HashMap key type changed from `&str` (implicit `tool.as_str()`) to `tool.as_str()` explicit — this is functionally identical.

3. `default_rules()` count — Updated from 21 to 22. Tests assert the new count explicitly. Any future addition to `default_rules()` will need to update this count again, which is a minor maintenance burden but not a regression risk for this PR.

4. Detection rules not in this PR — The architecture doc confirms all 21 existing rules were audited. Rules that filter on `"PostToolUse"` for non-differential purposes (search miss rate, context loaded, edit bloat) will naturally ignore `"PostToolUseFailure"` because the strings are distinct. No silent widening of existing rules.

5. Hook binary exit behavior — `PostToolUseFailure` arm always produces `HookRequest::RecordEvent` and never panics. Existing hooks for `PreToolUse`, `PostToolUse`, etc. are untouched.

**Overall regression risk: low.** The change is additive. No existing control paths are altered except the two intended differential fix sites, both of which are covered by cross-site integration tests.

---

## Prior Reviewer Findings (from earlier review on PR #388)

A prior security review on this PR identified two additional minor findings. Verification below:

**Finding A: Dead `tool_name` variable in `hook.rs` `PostToolUseFailure` arm** — NOT PRESENT in the current code. The `PostToolUseFailure` arm in `hook.rs` (lines 488-512) does not extract a `tool_name` binding at all. `tool_name` reaches `listener.rs` through `input.extra.clone()` as intended. This finding from the prior review appears to have been addressed or was based on an earlier draft.

**Finding B: Doc comment says "500-char limit" but code truncates at 500 bytes** — PARTIALLY ACCURATE. `extract_response_fields` uses `.chars().take(500)` (character-based); `extract_error_field` uses `truncate_at_utf8_boundary(error_str, 500)` (byte-based). The `extract_error_field` doc at line 2675 correctly says "500 bytes". However, line 2676 says "consistent with extract_response_fields snippet budget" — this is imprecise for multi-byte UTF-8 content. No security risk; minor doc quality issue only. Non-blocking.

## PR Comments
- Posted 1 comment on PR #388 (approval blocked — cannot self-approve own PR)
- Blocking findings: no

---

## Knowledge Stewardship

Nothing novel to store — the untrusted-input-in-snippet pattern is already captured in the project's existing ADR-007 col-023 (ingest security bounds, entry #2909). The defensive-parsing-on-hook-stdin pattern is in entry #247 (ADR-006). No new cross-feature anti-pattern emerged from this review.
