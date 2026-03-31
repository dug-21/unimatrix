# Security Review: bugfix-batch-security-reviewer

Agent ID: bugfix-batch-security-reviewer
PR: #464 (bugfix/batch-small-fixes)
GH Issues: #337, #345, #346, #378, #379, #380

## Risk Level: low

## Summary

Six hardening fixes across config validation, integer overflow guards, session input sanitization, and markdown output escaping. No new attack surface introduced. The fixes close or harden existing gaps. One low-severity observation on `escape_md_text` leading-whitespace stripping; no blocking findings.

---

## Findings

### Finding 1 — escape_md_text strips leading whitespace as a side effect

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/response/retrospective.rs:194-201`
- **Description**: When a string has leading whitespace followed by `#` (e.g., `"  ## heading"`), `escape_md_text` calls `s.trim_start()` on both the detection check and the format branch. The newline replacement earlier in the function cannot produce a string starting with `#` but preceded by spaces (spaces are not stripped by the earlier `replace` calls). However, if a goal or outcome text begins with literal spaces followed by `#`, the leading whitespace is silently dropped from the output. The `format!("\\{}", s.trim_start())` path produces `\## heading` instead of `\  ## heading`. This is cosmetically incorrect but cannot produce a Markdown heading (the `\` prefix is sufficient to escape it) and cannot cause injection. No test currently covers the leading-whitespace case.
- **Recommendation**: Add a test case for `escape_md_text("  ## heading")` to document the current behavior. If preserving the whitespace prefix is desired, replace `format!("\\{}", s.trim_start())` with `format!("\\{}", s.trim_start())` replaced by a version that inserts `\` only at the `#` character position rather than at the trimmed boundary.
- **Blocking**: no

### Finding 2 — RecordEvent / RecordEvents session_id arrives unsanitized in observation table

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:716-917` (RecordEvent and RecordEvents arms)
- **Description**: `HookRequest::RecordEvent` and `HookRequest::RecordEvents` do not call `sanitize_session_id` before using `event.session_id` in observation inserts (`insert_observation`, `insert_observations_batch`) and registry calls (`record_topic_signal`, `set_feature_if_absent`). The session_id value from these events is passed directly into `ObservationRow.session_id` and therefore into the SQLite OBSERVATIONS table. This is a pre-existing condition — it predates this PR — but it is worth noting since the PR fixes the same gap for CompactPayload (#346) and the pattern is inconsistent. An adversary with UDS access who can craft a RecordEvent packet with a malformed session_id could write a row with a non-conforming session_id into the observations table. The UDS socket is protected by peer UID verification (Layer 2), limiting the realistic attack surface to a process running as the same user or root.
- **Recommendation**: Apply `sanitize_session_id(&event.session_id)` at the top of both RecordEvent arms, consistent with the pattern established by SessionRegister, SessionClose, ContextSearch, and now CompactPayload. This is a follow-up hardening item, not a blocker for this PR (which fixes the targeted issue #346 correctly).
- **Blocking**: no

### Finding 3 — escape_md_text `\\|` ordering: pipe escape precedes # check (correct behavior confirmed)

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/response/retrospective.rs:195-197`
- **Description**: The function first replaces `|` with `\|`, then checks `trim_start().starts_with('#')`. A string like `"|# heading"` would have its pipe escaped to `\|# heading`, and then `trim_start()` on `\|# heading` does not start with `#`, so the heading escape is skipped. This is correct: a string starting with `|` cannot render as a Markdown heading. No issue.
- **Recommendation**: None.
- **Blocking**: no

---

## Special Attention Responses

### #346 (sanitize_session_id on CompactPayload)

The guard is placed correctly — before `handle_compact_payload` is called — and returns `ERR_INVALID_PAYLOAD` with a descriptive message. The `handle_compact_payload` function uses `session_id` directly in `AuditContext.source` (written to audit log) and passes it to `session_registry.get_state`, `get_category_histogram`, and `increment_compaction`. Without this guard, a malformed session_id (e.g., containing `../` or newlines) could have entered the audit log unsanitized. The early-return path is correct: it logs a warn-level trace event including the raw `session_id` value, which is safe because tracing fields are not interpreted by the rendering layer. The guard fully closes the reported gap for CompactPayload.

Note: the same gap still exists on `RecordEvent`/`RecordEvents` (see Finding 2 above).

### #378/#379 (escape_md_text leading-# rule)

The implementation does exactly what it describes: it checks `trim_start().starts_with('#')` (not a global replace). An embedded `#378` in a goal string like `"Fix for #378 and #379"` will not trigger the branch because `trim_start()` on that string produces `"Fix for #378..."` which does not start with `#`. The test `test_escape_md_text_heading_embedded_reference_and_pipe` explicitly verifies the non-escaping of embedded references. This is correct.

The only behavioral gap is the leading-whitespace side effect described in Finding 1 (low severity, non-blocking).

### #380 (try_from unwrap_or(i64::MAX))

The behavioral change from `obs.ts as i64` (wrapping cast: `u64::MAX as i64 = -1`) to `i64::try_from(obs.ts).unwrap_or(i64::MAX)` (saturating: values above `i64::MAX` become `i64::MAX`) is safe for all downstream consumers.

The consumer is the window filter: `ts >= window.start_ms && ts < window_end`. All realistic observation timestamps (epoch milliseconds in the year ~2024) fit within i64 and are unaffected. For pathological out-of-range values (years > 292 billion), the old code produced negative timestamps that caused the observation to be silently excluded from every window. The new code produces `i64::MAX`, which still does not satisfy `ts < window_end` when `window_end = i64::MAX` (open window), but correctly avoids the false negative-timestamp classification. The test confirms `u64::MAX as i64 = -1` (old behavior) vs. `i64::try_from(u64::MAX).unwrap_or(i64::MAX) = i64::MAX` (new behavior). No downstream consumer depends on the wrap-to-negative behavior.

---

## Blast Radius Assessment

- **config.rs (#337)**: If `validate_config` has a false positive, server startup fails with `ConfigError::FusionWeightSumExceeded`. Failure mode is safe: error returned at startup, no data mutation. Worst case: misconfigured users cannot start the server after upgrade until they fix their config.

- **session.rs / search.rs (#345)**: Counters saturate at `u32::MAX` instead of wrapping to 0. If a counter reaches `u32::MAX` (practically impossible in normal use: ~4 billion increments per session), the histogram affinity boost and topic majority vote are computed on a saturated value. Saturated totals produce conservatively correct scoring behavior (score slightly off but not injected/corrupted). No data loss or persistence impact.

- **listener.rs (#346)**: Early return on invalid session_id prevents `handle_compact_payload` from executing. If the guard has a false positive on a valid session_id format, the client gets `ERR_INVALID_PAYLOAD` and the compaction is skipped. The session registry increment does not happen. This is a safe degradation: the hook client retries or falls back to default behavior. The `sanitize_session_id` allowlist (`[a-zA-Z0-9-_]`, max 128) is consistent with the existing guards on other request types, so a false positive would affect all session operations equally.

- **retrospective.rs (#378/#379)**: Markdown output is cosmetic (no data mutation, no persistence). Worst case if `escape_md_cell` or `escape_md_text` introduces a regression: a retrospective report renders incorrectly. No security consequence.

- **tools.rs (#380)**: Affects retrospective phase window attribution only. If the saturation behavior silently changes window membership for edge-case timestamps, phase duration statistics in the retrospective report are incorrect. No data mutation, no persistence of the computed values. The realistic blast radius is limited to cosmetic retrospective report errors.

---

## Regression Risk

- **config.rs**: Medium regression risk. The added `validate_config` call runs on the merged config with access to both global and project values. If any existing user's combined config inadvertently violates a constraint that was previously only checked per-file, the server will refuse to start. The constraint in question (`FusionWeightSumExceeded`) is new and only catches the post-merge case, so existing single-file configs are unaffected. The test validates the specific scenario.

- **session.rs / search.rs**: Minimal. `saturating_add` is a strict improvement over wrapping addition; no existing behavior changes for values in the normal operating range.

- **listener.rs**: Minimal. The `sanitize_session_id` function is pre-existing and already in use for SessionRegister, SessionClose, and ContextSearch. The new call uses the identical code path.

- **retrospective.rs**: Low. The escaping helpers are additive. The updated test for `test_baseline_outlier_metric_name_with_pipes` (which previously tested for unescaped pipes) now correctly requires escaped output.

- **tools.rs**: Minimal. The `try_from().unwrap_or(i64::MAX)` replacement produces identical results for all timestamps in the realistic range (values <= `i64::MAX` as u64 convert cleanly via `try_from`).

---

## Dependency Safety

No new dependencies introduced. No existing dependencies modified.

---

## Secrets Check

No hardcoded secrets, API keys, credentials, or tokens found in the diff.

---

## PR Comments

- Posted 1 comment on PR #464 (summary of findings, non-blocking).
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the RecordEvent session_id gap is a pre-existing condition predating this PR; the finding is documented in the PR comment for follow-up tracking. The escape_md_text whitespace side effect is a low-severity observation with no generalizable anti-pattern beyond what is already covered by the existing Markdown injection lesson entries.
