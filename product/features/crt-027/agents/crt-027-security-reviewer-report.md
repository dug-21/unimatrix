# Security Review: crt-027-security-reviewer

## Risk Level: low

## Summary

crt-027 is a knowledge delivery improvement feature. The changes are additive in nature: a new optional wire field on an existing struct, a new hook routing arm, a new briefing service replacing an old one, and updates to protocol text files. No new external trust boundaries are introduced and no privilege escalations occur. One medium-severity validation gap was identified — input validation for the `source` field written to the observations database column — but it is mitigated by the observation write being fire-and-forget and by SQLite's type flexibility preventing injection. All other security-relevant behavior (rate limiting, injection detection, quarantine filtering, Active-only status filtering) is preserved through delegation to unchanged services.

---

## Findings

### Finding 1: `source` field written to observations `hook` column without length validation

- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/uds/listener.rs`, `dispatch_request` ContextSearch arm, line ~812 in the diff; `unimatrix-store/src/migration.rs` — `hook TEXT NOT NULL` column schema
- **Description**: The `source` field from `HookRequest::ContextSearch` is written directly into the observations table `hook` column via `source.as_deref().unwrap_or("UserPromptSubmit").to_string()`. This field is an `Option<String>` with no length constraint validated before storage. In practice the field is only set to `Some("SubagentStart")` by the internal hook process, but the wire protocol accepts any string value a caller could set. SQLite stores `TEXT` without a size limit. The `hook` column has no CHECK constraint. A long string (e.g., 1MB) would be written to every observation row produced by that session.
- **Recommendation**: Add a length cap at the `dispatch_request` site: `source.as_deref().unwrap_or("UserPromptSubmit").chars().take(64).collect::<String>()` — or validate via an allowlist of known source values (`"UserPromptSubmit"`, `"SubagentStart"`). Allowlist is preferred because it also prevents analytics corruption from arbitrary caller-supplied tags.
- **Blocking**: No. The practical exploit path requires a caller who can write raw bytes to the UDS socket (i.e., already has local process access), and the failure mode is storage bloat, not data exfiltration or privilege escalation. Mitigated by the fire-and-forget write path (failure is logged, not propagated).

---

### Finding 2: `IndexBriefingService` does not call input validation before dispatching to `SearchService`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/index_briefing.rs`, `index()` method
- **Description**: The deleted `BriefingService.assemble()` called `validate_briefing_inputs()` before any storage or search calls. That function checked role/task length and control characters. `IndexBriefingService.index()` does not call an equivalent function directly; instead it relies on `SearchService.search()` calling `self.gateway.validate_search_query()` (confirmed at `search.rs:528-533`). The `validate_search_query` function enforces query length (10,000 chars max), control characters, and k bounds. This delegation is functionally equivalent for the query. However, `max_tokens` is accepted as a `usize` by `IndexBriefingParams` after being validated at the MCP handler layer via `validated_max_tokens()`. The UDS `handle_compact_payload` path also validates `max_bytes` via `max_bytes.min(MAX_COMPACTION_BYTES)`. The validation chain is preserved but it is now split across two layers (MCP handler + SearchService) rather than concentrated in one service method.
- **Recommendation**: Document explicitly in `IndexBriefingService::index()` that input validation is delegated to `SearchService.search()`. A single-line comment would prevent a future developer from erroneously removing the SearchService call and believing validation is still present.
- **Blocking**: No. The validation is present and effective; this is a documentation gap, not a missing check.

---

### Finding 3: `prompt_snippet` from `SubagentStart` hook input flows through without explicit content inspection prior to search

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/hook.rs`, `build_request`, `"SubagentStart"` arm
- **Description**: The `prompt_snippet` value from `input.extra["prompt_snippet"]` is extracted and used as the search query with no pre-validation beyond `.trim().is_empty()`. This is the same handling as `UserPromptSubmit` (which also passes the raw prompt string to `SearchService`). The `SecurityGateway.validate_search_query()` is called later in `SearchService.search()`, which detects injection patterns (warn-only), rejects control characters, and enforces 10,000 char length. This is consistent with existing behavior. No new attack surface beyond what already exists on the `UserPromptSubmit` path.
- **Recommendation**: No code change needed. This is consistent with the existing trust model. Document in a code comment that `SearchService` is responsible for query sanitization.
- **Blocking**: No.

---

### Finding 4: `source: Some("SubagentStart")` is hardcoded in `hook.rs` — no allowlist enforcement at server side

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs`, `dispatch_request`
- **Description**: The server trusts that the `source` field in `HookRequest::ContextSearch` contains a meaningful value for the `hook` column. A malicious or misbehaving hook process could set `source: Some("DROP TABLE observations; --")` (SQL injection attempt). However, the observation write uses `sqlx` with parameterized queries (confirmed by existing code patterns in the observations insert path), so SQL injection is not possible. The value is stored as a TEXT string. The risk is observation table pollution with arbitrary tag values, which could corrupt retrospective analytics.
- **Recommendation**: Same as Finding 1: allowlist or length-cap the `source` value before writing to the database.
- **Blocking**: No.

---

### Finding 5: Old `BriefingService` tests for quarantine exclusion and deprecated entry exclusion removed

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/briefing.rs` (deleted), `crates/unimatrix-server/src/services/index_briefing.rs` (new)
- **Description**: The deleted `BriefingService` had explicit tests for quarantine exclusion (T-BS-08) and deprecated entry exclusion (T-BS-10 crt-010 AC-11). `IndexBriefingService` relies on `SearchService` with `RetrievalMode::Strict` plus a post-filter `se.entry.status == Status::Active` to achieve the same exclusion. The new tests confirm the Active-only filter at the formatter layer (AC-06 test, `format_payload_active_entries_only`). However, no test in `index_briefing.rs` directly verifies that quarantined entries do not appear in the `index()` output. The behavior is correct by construction (Active != Quarantined, Deprecated, or other statuses), but the explicit guard test was dropped.
- **Recommendation**: Add a test to `index_briefing.rs` confirming that a `ScoredEntry` with status `Quarantined` is excluded by the post-filter. This is a defense-in-depth test, not a fix.
- **Blocking**: No. The filter `se.entry.status == Status::Active` correctly excludes Quarantined entries.

---

### Finding 6: No hardcoded secrets, API keys, or credentials

- **Severity**: n/a
- **Location**: All changed files
- **Description**: Full diff reviewed. No hardcoded credentials, tokens, or API keys found. No new environment variable reads that could expose sensitive data. The deprecated `UNIMATRIX_BRIEFING_K` env var is removed from the production path, reducing attack surface.
- **Blocking**: No.

---

## Blast Radius Assessment

**Worst case if this fix has a subtle bug:**

1. `IndexBriefingService.index()` returns wrong entries or panics. Impact: `context_briefing` MCP tool returns an error or empty result. The CompactPayload path degrades to an empty compaction payload (logged, not propagated, confirmed by graceful degradation code). No data corruption. No privilege escalation. Blast radius: subagents and SM agents start phases with no knowledge package — they still operate, just without briefing context. This is the same state as before crt-027.

2. The `SubagentStart` hook routing misclassifies an event. Impact: subagent receives wrong knowledge injection or no injection. Observation is tagged `"SubagentStart"` instead of `"UserPromptSubmit"` or vice versa. Impact on retrospective analytics is minor (category counts shift). No security consequence.

3. `derive_briefing_query` returns wrong query. Impact: knowledge briefing returns less relevant entries. No data corruption, no privilege escalation. Worst-case blast radius is SM agents getting less relevant context at phase boundaries.

4. The `hookSpecificOutput` JSON envelope malformed. Impact: Claude Code ignores the SubagentStart hook stdout. Subagent receives no injection. Graceful degradation is designed in and confirmed (server records observation, subagent operates without context).

**Maximum blast radius: reduced knowledge delivery quality during a session. No data corruption, no privilege escalation, no availability impact.**

---

## Regression Risk

**Medium-low.** The main regression risks are:

1. `format_compaction_payload` output format change — callers of `PreCompact` hook receive a flat indexed table instead of the section-based format. This is intentional and a breaking change for any external parser, but the hook output is only consumed by Claude Code's context prepend mechanism, which is format-agnostic.

2. `context_briefing` MCP tool output format change — callers receive a flat table instead of Markdown sections with role/conventions/context headers. Protocol files have been updated to document the new format. Existing callers passing `role` and `task` are backward-compatible (role is ignored, task drives query). This is an intentional breaking change for the tool's output format.

3. `UserPromptSubmit` word-count guard — prompts under 5 words no longer trigger knowledge injection. This is intentional behavior change. Regression risk: a workflow that relied on short prompts (e.g., a single-word `yes` response) triggering injection would now miss injection. The design accepts this trade-off explicitly (ADR-002).

4. `BriefingService` deletion removes 2,293 lines and 20+ tests covering injection history, convention lookup, and effectiveness sort. The new `IndexBriefingService` does not have an injection history path (it uses semantic search only). If any caller relied on `BriefingService` directly (outside of `ServiceLayer`), this would be a break. Confirmed that `ServiceLayer.briefing` is the only call site.

5. `UNIMATRIX_BRIEFING_K` env var silently ignored — operators who configured this env var in production will no longer see it take effect. The default k increases from 3 to 20, which increases output size and latency for briefing calls.

---

## PR Comments

- Posted 1 comment on PR #350 summarizing findings.
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the `source` field allowlist gap (Finding 1/4) is a specific instance of the general "unbounded string written to analytics column" pattern already covered by existing lessons. The rate limiting delegation through `SearchService` (Finding 2) demonstrates a sound pattern already established in the codebase.
