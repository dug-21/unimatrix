# Security Review: 536-security-reviewer

**Agent ID**: 536-security-reviewer
**PR**: #541 — `bugfix/536-phase-stats-tool-normalization`
**GH Issue**: #536
**Date**: 2026-04-07

## Risk Level: low

## Summary

The fix promotes an existing internal string-stripping function (`normalize_tool_name`) to `pub` and re-exports it from the crate root, then applies it at three match sites in `categorize_tool_for_phase` and `compute_phase_stats`. The change is read-only analytics code — it affects metric counting only, not access control, data storage, or MCP request routing. No new attack surface is introduced. No injection vectors, secrets, new dependencies, or unsafe code were found.

## Findings

### Finding 1: normalize_tool_name public promotion — no security risk

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-observe/src/session_metrics.rs:214`
- **Description**: `normalize_tool_name` changed from `fn` to `pub fn` and re-exported at `unimatrix_observe::normalize_tool_name`. The function is a pure string transform: `tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool)`. Making it public adds no trust-boundary exposure. It accepts a `&str` reference and returns a `&str` reference (lifetime-bound to input). It cannot panic, allocate, or mutate state. Its only effect is stripping a fixed ASCII prefix.
- **Recommendation**: No action required. The promotion is appropriate — the function now has two consumers (`session_metrics` internally, `tools.rs` externally) and a single canonical definition is correct.
- **Blocking**: no

### Finding 2: Input validation at the normalization boundary

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:3426`, `3619`, `3634`
- **Description**: `normalize_tool_name` is called on `ObservationRecord.tool`, which is a client-declared `Option<String>` from hook events. The function is used purely for categorization and counting. It cannot write to the database, route requests, or escalate privilege. Even if an attacker could inject a crafted `tool` value into an `ObservationRecord`, the worst outcome is miscategorized analytics counts — a data integrity issue, not a security issue. The `Option<String>` is handled correctly at all three sites: `None` falls through the chain via `.as_deref()` + `.map()` returning `None` before normalization is ever called.
- **Recommendation**: No change required. The fix does not widen the attack surface compared to pre-fix code (both old and new code read the same `tool` field).
- **Blocking**: no

### Finding 3: Inconsistent use of map_or(false) vs is_some_and in tools.rs

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:3620`, `3635`
- **Description**: The existing `session_metrics.rs` call sites were updated from `map_or(false, ...)` to `is_some_and(...)` (a clippy improvement). The two new filter chains in `tools.rs` (`knowledge_served` at line 3620 and `knowledge_stored` at line 3635) retain `map_or(false, ...)`. These are semantically equivalent; `is_some_and` is marginally more idiomatic in Rust 1.70+. This is a stylistic inconsistency between the two files, not a security or correctness issue.
- **Recommendation**: A follow-up clippy pass could unify the style. Not a security concern. Not blocking.
- **Blocking**: no

### Finding 4: Label-only change in retrospective.rs — no risk

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/mcp/response/retrospective.rs:1004`
- **Description**: `"**Total served**"` changed to `"**Distinct entries served**"`. This is a UI string in a Markdown report renderer. No computation, validation, or access control is affected. Both test assertions updated to match. No security implication.
- **Blocking**: no

### Finding 5: No missed match sites — verified independently

- **Severity**: n/a (verification note)
- **Location**: Codebase-wide grep
- **Description**: A codebase-wide search for bare `== "context_search"`, `== "context_lookup"`, `== "context_get"`, `== "context_store"` patterns confirms only two `== "context_store"` comparisons remain in production code — both inside the normalized filter chains (post-`normalize_tool_name` application), so they correctly operate on stripped names. No unprotected bare-name comparisons against MCP-prefixed values remain outside of already-normalized contexts.
- **Blocking**: no

## OWASP Evaluation

| OWASP Category | Assessment |
|---|---|
| A01 Broken Access Control | Not affected. Changed code is read-only analytics; no authorization logic modified. |
| A02 Cryptographic Failures | Not applicable. No crypto involved. |
| A03 Injection | Not affected. `strip_prefix` is a pure string operation; input never passes to a shell, SQL query, or format string. |
| A04 Insecure Design | Not affected. Fix restores intended design — normalization before matching. |
| A05 Security Misconfiguration | Not affected. No configuration changed. |
| A06 Vulnerable Components | Not affected. No new dependencies introduced. |
| A07 Identity & Auth Failures | Not affected. No auth path touched. |
| A08 Data Integrity Failures | Risk: pre-fix, phase stats were silently wrong (always zero for MCP-prefixed tools). Fix restores correct counting. Existing tests would have caught regressions. |
| A09 Logging Failures | Not applicable. |
| A10 SSRF | Not applicable. No network calls in changed code. |

## Blast Radius Assessment

**Worst case if this fix has a subtle bug**: Incorrect phase statistics in the retrospective report — wrong counts for `knowledge_served`, `knowledge_stored`, and `tool_distribution.search`. This is a data integrity failure in an analytics/reporting feature, not a security failure. No data is corrupted in the store, no access control is bypassed, and no MCP tool routing is affected. The blast radius is bounded entirely to the `context_retrospective` response rendering.

**Safe failure mode**: If `normalize_tool_name` were somehow called with a nil pointer (impossible in Rust — the type is `&str`), a compile error would result, not a runtime failure. All call sites handle `None` correctly before normalization via the `Option` chain.

## Regression Risk

**Low.** The change is additive in behavior: previously, MCP-prefixed tool names fell through to "other" or zero counts; now they correctly categorize. Existing tests that used bare names (`make_obs_at("...", "context_search")`) were updated to use `make_mcp_obs_at`, which prepends the production prefix. The new test `test_phase_stats_mcp_prefix_normalized_correctly` directly guards the fixed behavior. No previously-passing test was deleted — only call sites updated to match the production data format.

**Existing functionality at risk**: None. `classify_tool` in `session_metrics.rs` already called `normalize_tool_name` internally, so session summaries were already correct. The fix brings `compute_phase_stats` and `categorize_tool_for_phase` in `tools.rs` into alignment with `session_metrics.rs`.

## Dependency Safety

No new crate dependencies introduced. No `Cargo.toml` changes in the diff. The fix is purely within existing code.

## Secrets

No hardcoded secrets, API keys, credentials, or tokens found in the diff.

## PR Comments

- Posted 1 comment on PR #541 (informational, non-blocking)
- Blocking findings: no

## Knowledge Stewardship

- Nothing novel to store — the pattern (normalize before match; pub-promotion for a second consumer) was already stored as Unimatrix entry #4204 by the rust-dev agent. Storing the same pattern again would create a duplicate.
