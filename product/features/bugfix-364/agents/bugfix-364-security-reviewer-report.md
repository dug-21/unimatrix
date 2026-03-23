# Security Review: bugfix-364-security-reviewer

## Risk Level: low

## Summary

PR #365 makes `BriefingParams.role` optional (`String` -> `Option<String>`) in the MCP
tool parameter struct. The change is minimal, correctly typed, and updates all validation
and handler call sites. No new trust boundaries, no new inputs from external sources, and
no dependency changes. All changed code is in input validation and parameter deserialization
layers that were already present.

---

## Findings

### F-1: Validation bypass when role is None
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/validation.rs:281-283`
- **Description**: When `role` is `None`, the `validate_string_field` call for `role` is
  skipped entirely. This is the intended behavior for the fix. The field was previously
  required and validated; now it is optional and validation is conditionally skipped.
  No validation gap exists — an absent field has no content to validate. If `role` is
  present (`Some`), the original length and control-character checks still apply.
- **Recommendation**: No action needed. The guard `if let Some(role)` is the correct
  pattern for optional field validation.
- **Blocking**: no

### F-2: "unknown" static fallback in query derivation
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:945-948`
- **Description**: When both `feature` and `role` are `None`, the handler uses the static
  string `"unknown"` as the `topic` fallback that feeds into `derive_briefing_query`. This
  string becomes the search query only if both `task` is empty AND no session state is
  available (i.e., steps 1 and 2 of the three-step derivation both fail). The `"unknown"`
  string is a hardcoded safe value — it is embedded into a vector search query (passed to
  the embedding pipeline as a float vector, never interpolated into SQL or a shell command).
  Worst case: the query returns low-relevance entries. No injection vector exists.
- **Recommendation**: No action needed. The value is safe in the search pipeline.
- **Blocking**: no

### F-3: No new injection surface
- **Severity**: informational
- **Location**: entire diff
- **Description**: The `role` value (when present) is validated with the same
  `validate_string_field` guard used by all other string fields — length check (max 100
  chars) and control-character rejection. After validation it is used only as a last-resort
  search query string passed to the embedding model. It is never interpolated into SQL,
  filesystem paths, shell commands, or format strings with untrusted data. OWASP injection
  (A03:2021) risk is not introduced.
- **Recommendation**: None.
- **Blocking**: no

### F-4: No new deserialization risk
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:204-222`
- **Description**: Changing `role: String` to `role: Option<String>` is a narrowing of the
  deserialization contract (a previously required field becomes optional). This is
  backward-compatible for callers that supply `role` and forward-compatible for callers
  that omit it. `serde_json` handles `Option<String>` absent-key deserialization safely.
  No malformed-input deserialization path was introduced.
- **Recommendation**: None.
- **Blocking**: no

### F-5: UDS path unaffected
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs`
- **Description**: The UDS code path does not use `BriefingParams` — it constructs
  `IndexBriefingParams` directly. The `role` field change has no effect on the UDS
  briefing path. Confirmed by zero diff lines touching `listener.rs` in `main...HEAD`.
- **Recommendation**: None.
- **Blocking**: no

---

## Blast Radius Assessment

If the fix has a subtle bug, the worst case is:

- A caller sends `role: None` (no role, no feature, empty task) — the fallback chain
  resolves to `"unknown"` as the search query, which produces a low-relevance briefing.
  The failure mode is **degraded quality results**, not data corruption, information
  disclosure, or privilege escalation.
- A caller sends a malicious `role` value — it is still subject to length (max 100 chars)
  and control-character validation. The only use of the value is as a search query string
  passed to the embedding model.
- The blast radius is confined to a single MCP tool handler (`context_briefing`). No
  other tools, storage writes, or trust checks are affected.

---

## Regression Risk

Low. The fix:

1. Relaxes a previously required field to optional — a backward-compatible change.
2. Updates exactly two files (`mcp/tools.rs`, `infra/validation.rs`) with no changes to
   shared infrastructure, the storage layer, or any other tool handler.
3. All 8 briefing-specific integration tests pass. The `test_briefing_params_missing_role`
   test was inverted to directly catch this class of bug in future.
4. No clippy warnings introduced in changed files.

The only plausible regression: a caller that depended on the error response when `role` was
absent and `task` was missing would no longer receive a deserialization error for the `role`
field specifically. However, `task` remains required (`String`, not `Option<String>`), so
the missing-required-field error path is still exercised when `task` is absent. The
`test_briefing_params_missing_task` test confirms this (asserts `is_err()` for
`{"role":"architect"}`).

---

## Dependency Safety

No new dependencies were introduced. `Cargo.toml` and `Cargo.lock` are unchanged in the
PR diff.

---

## Secrets / Hardcoded Credentials

None. The diff contains no secrets, API keys, tokens, or credentials. The static string
`"unknown"` used as a fallback is not a credential.

---

## PR Comments

- Posted 1 comment on PR #365 (general findings summary, non-blocking).
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this is a standard optional-field relaxation with clean
  validation guard pattern. The `if let Some(field)` pattern for optional MCP tool
  parameter validation is already established practice in this codebase and does not
  warrant a new lesson-learned entry.
