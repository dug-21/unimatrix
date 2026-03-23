# Gate 3b Report: crt-027

> Gate: 3b (Code Review)
> Date: 2026-03-23
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All components match validated pseudocode |
| Architecture compliance | PASS | ADR-001 through ADR-006 all implemented correctly |
| Interface implementation | PASS | Signatures match, data types correct, error handling follows project patterns |
| Test case alignment | PASS | All planned test scenarios implemented |
| Code quality | WARN | Build succeeds; `max_tokens` field dead-code warning (new, intentional); pre-existing `services/mod.rs` 532 lines (pre-existed before crt-027); no stubs, no `todo!()`, no `.unwrap()` in non-test code |
| Security | PASS | No hardcoded secrets; input validated; no path traversal or injection vectors introduced |
| Knowledge stewardship | PASS | Not applicable — gate agent (read-only); queried patterns below |

---

## Detailed Findings

### 1. Pseudocode Fidelity

**Status**: PASS

**Evidence**:

- `wire.rs` ContextSearch variant: `source: Option<String>` added at line 121 with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Matches pseudocode `wire-source-field.md` exactly.
- `hook.rs` SubagentStart arm: added before `_ => generic_record_event` fallthrough at line 401-426. Uses `input.session_id.clone()` (not ppid fallback), sets `source: Some("SubagentStart")`, guards with `.trim().is_empty()`. Matches `hook-routing.md`.
- `hook.rs` UserPromptSubmit word-count guard: implemented at lines 282-288. Uses `query.split_whitespace().count()` instead of `query.trim().split_whitespace().count()` — functionally equivalent because `str::split_whitespace()` in Rust already skips leading/trailing whitespace; the comment at line 283-284 documents this explicitly. FR-05 intent is preserved.
- `write_stdout_subagent_inject` and `write_stdout_subagent_inject_response` implemented at lines 667-708. JSON envelope matches ADR-006 specification exactly.
- `index_briefing.rs` (new file): `IndexBriefingService`, `IndexBriefingParams`, `derive_briefing_query`, `extract_top_topic_signals` all match pseudocode. `default_k: 20` hardcoded. `effective_k` clamps `k=0` to `default_k`. Post-filter for `Status::Active` present.
- `mcp/response/briefing.rs`: `IndexEntry` struct, `format_index_table`, `SNIPPET_CHARS = 150` all match `index-entry-formatter.md`. `Briefing` struct and `format_briefing` deleted. `format_retrospective_report` retained.
- `listener.rs` `dispatch_request`: `source.as_deref().unwrap_or("UserPromptSubmit")` at line 813 replaces hardcoded literal. `handle_compact_payload` uses `IndexBriefingService`, `derive_briefing_query`, and `format_compaction_payload(Vec<IndexEntry>, ...)`. Matches `listener-dispatch.md`.
- `services/mod.rs`: `briefing: IndexBriefingService` field, deprecation comment for `UNIMATRIX_BRIEFING_K`, `IndexBriefingService::new()` call with `Arc::clone(&effectiveness_state)`. Matches `service-layer-wiring.md`.
- `mcp/tools.rs` `context_briefing` handler: uses `derive_briefing_query`, `IndexBriefingParams`, `IndexBriefingService::index()`, `format_index_table`. Three-step query derivation present. `max_tokens: 1000` respected. Matches `context-briefing-handler.md`.

### 2. Architecture Compliance

**Status**: PASS

**Evidence**:

- **ADR-001**: `source: Option<String>` with `#[serde(default)]` on `HookRequest::ContextSearch`. Backward-compatible — all existing callers without `source` deserialize to `None`. Confirmed by wire.rs test at line 1172 (JSON without `source` key deserializes to `source == None`).
- **ADR-002**: SubagentStart arm placed before `_ =>` fallthrough. `MIN_QUERY_WORDS: usize = 5` defined at line 34. SubagentStart not subject to word-count guard. Both guards use `.trim()` semantics.
- **ADR-003**: `UNIMATRIX_BRIEFING_K` not read. Deprecation comment at `services/mod.rs` line 424. `default_k: 20` hardcoded in constructor. `parse_semantic_k()` deleted.
- **ADR-004**: `CompactionCategories` struct deleted. `format_compaction_payload` signature accepts `Vec<IndexEntry>`. Budget enforcement via row-count reduction loop (cleaner than string truncation, matches pseudocode preferred approach).
- **ADR-005**: `IndexEntry` typed struct in `mcp/response/briefing.rs`. `format_index_table` as canonical formatter. `SNIPPET_CHARS = 150`. Both re-exported unconditionally from `mcp/response/mod.rs` line 48 — NFR-05 compliance confirmed.
- **ADR-006**: `write_stdout_subagent_inject` writes `hookSpecificOutput` JSON envelope. `run()` branches on `req_source.as_deref() == Some("SubagentStart")` at line 101.

Component boundary compliance:
- Server returns `HookResponse::Entries` unchanged for SubagentStart. JSON wrapping is hook-process-only concern. Server has no awareness of stdout format.
- `IndexBriefingService` does not hold `SessionRegistry`. Callers pre-resolve histograms (consistent with `handle_context_search` pattern). `IndexBriefingParams.category_histogram` carries pre-resolved value.
- `IndexBriefingService` compiles unconditionally (not gated by `mcp-briefing`). UDS path always active.

### 3. Interface Implementation

**Status**: PASS

**Evidence**:

- `IndexBriefingService::new(Arc<Store>, SearchService, Arc<SecurityGateway>, EffectivenessStateHandle)` — required non-optional `EffectivenessStateHandle` is compile-time enforced. Missing wiring = compile error.
- `IndexBriefingService::index(params: IndexBriefingParams, audit_ctx: &AuditContext, caller_id: Option<&CallerId>) -> Result<Vec<IndexEntry>, ServiceError>` — matches ARCHITECTURE.md integration surface table exactly.
- `format_index_table(entries: &[IndexEntry]) -> String` — public, at `mcp/response/briefing.rs` line 50.
- `derive_briefing_query(task: Option<&str>, session_state: Option<&SessionState>, topic: &str) -> String` — public(crate), both MCP and UDS callers use it.
- `format_compaction_payload` updated signature: `(entries: &[IndexEntry], role, feature, compaction_count, max_bytes, category_histogram)` matches ARCHITECTURE.md.
- `write_stdout_subagent_inject(entries_text: &str) -> std::io::Result<()>` and wrapper `write_stdout_subagent_inject_response` — correct signatures per ADR-006.
- Error handling: all `ServiceError` errors propagate via `?`; `handle_compact_payload` uses graceful degradation on `Err` (returns empty `BriefingContent`, not panic); hook errors log via `eprintln!` and exit 0 (FR-06).

### 4. Test Case Alignment

**Status**: PASS

**Evidence** (test names from key plan scenarios all present):

- `hook.rs` tests (lines 2707-3025): SubagentStart routing (T-HR-01 through T-HR-05), UserPromptSubmit word-count (T-HR-06 through T-HR-11), `write_stdout_subagent_inject` JSON envelope (T-HR-12, T-HR-13 equivalents), `MIN_QUERY_WORDS` constant check.
- `wire.rs` tests (lines 1167-1266): source field absent defaults to `None`, source `"SubagentStart"` deserializes correctly, round-trip `source: None`.
- `index_briefing.rs` tests (lines 289-481): `derive_briefing_query` all three steps, empty task falls through, fewer-than-3 signals, no trailing spaces, absent `feature_cycle` falls to topic.
- `mcp/response/briefing.rs` tests (lines 103-422): empty slice returns empty string, column presence, multibyte UTF-8 safe, confidence formatting, section header absence, snippet truncation.
- `listener.rs` tests (lines 2855-3267): `format_compaction_payload` — empty entries returns `None`, header present, budget enforcement, UTF-8 safety, session context, active-only, entry ID present, token override, histogram present/absent.
- `tools.rs` tests (lines 3010-3200+): `context_briefing_active_only_filter`, `context_briefing_default_k_20`, `context_briefing_k_override`, `context_briefing_flat_table_format`.

All test runs pass: 1871 tests in unimatrix-server, 0 failures.

### 5. Code Quality

**Status**: WARN

**Evidence**:

- `cargo build --workspace` succeeds: `Finished dev profile` with 0 errors.
- `cargo test --workspace`: all test suites pass (0 failures across all crates).
- No `todo!()`, `unimplemented!()`, or placeholder functions. No `// TODO` or `// FIXME` in crt-027 code (pre-existing `W2-4` TODOs in `main.rs` and `services/mod.rs` are not crt-027 work).
- No `.unwrap()` in non-test production code in any crt-027 modified file.
- **WARN-1**: `max_tokens: Option<usize>` in `IndexBriefingParams` (index_briefing.rs line 46) is never read by `index()`. Compiler issues a dead-code warning. This is intentional per the spec ("for future ranked truncation; not enforced here") and does not block functionality, but the warning is new and originates from crt-027.
- **WARN-2**: Four `#[allow(dead_code)]` annotations on `IndexBriefingService` fields (`entry_store`, `gateway`, `effectiveness_state`, `cached_snapshot`) that are stored for the pattern but not yet used in `index()`. The attributes suppress what would otherwise be compiler warnings. AC-13 prohibits `#[allow(dead_code)]` on removed types, but these are on new fields of the new type. This is a minor code smell for a new service, not a deleted-type cleanup failure.
- **WARN-3**: `services/mod.rs` is 532 lines — exceeds the 500-line limit. Pre-existing condition: the file was 530 lines before crt-027 (confirmed by git history at commit `a5372ab`). crt-027 added 2 lines. Not a crt-027 regression.
- `hook.rs`, `listener.rs`, `tools.rs`, `wire.rs` all exceed 500 lines — all pre-existing large files.

### 6. Security

**Status**: PASS

**Evidence**:

- No hardcoded secrets, API keys, or credentials in any crt-027 modified file. `k=20` is a logic constant, not a credential.
- Input validation: `HookRequest::ContextSearch` validated by `sanitize_session_id` in `listener.rs` before query execution. SubagentStart `prompt_snippet` input validated (`.trim().is_empty()` guard). No raw user input passed to SQL without parameterization (delegated to `SearchService`/`Store`).
- No path traversal vulnerabilities introduced. No new file operations.
- No command injection. No new shell/process invocations.
- `serde_json` deserialization in `write_stdout_subagent_inject`: the envelope is constructed (not parsed) from Rust values — no deserialization of untrusted input here.
- `cargo audit` not installed in this environment; check skipped. Pre-existing dependency set unchanged by crt-027 (no new Cargo.toml dependencies added).

### 7. Knowledge Stewardship Compliance

**Status**: PASS

Evidence that implementation agents (rust-dev agents) produced reports is outside the scope of this gate check (Gate 3b validates code, not agent reports). This check is satisfied at the process level: the session is on branch `feature/crt-027`, and the implementation commits reference `(#349)`, confirming agents executed under the swarm protocol.

---

## Rework Required

None.

---

## Notable Observations (Non-blocking)

The following are WARN items that do not block Gate 3b but should be addressed in a follow-on cleanup:

1. **`max_tokens` dead-code warning** — `IndexBriefingParams.max_tokens` is set by callers but unused in `index()`. Either add `#[allow(dead_code)]` with a comment explaining the future intent, or remove the field until the "future ranked truncation" feature is scoped. The current state emits a compiler warning that may confuse future developers.

2. **`#[allow(dead_code)]` on `IndexBriefingService` fields** — `entry_store`, `gateway`, `effectiveness_state`, `cached_snapshot` are stored but the service currently delegates all work to `SearchService`. These fields exist for pattern consistency with `BriefingService` but add no current functionality. A comment per field explaining the retention rationale would clarify intent.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for gate 3b validation patterns and code quality standards before executing checks — found existing validation patterns in entries confirming project standards.
- Stored: nothing novel to store — this gate confirms standard implementation patterns; no recurring failure pattern observed that warrants a new lesson entry.
