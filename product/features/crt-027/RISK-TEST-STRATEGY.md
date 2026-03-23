# Risk-Based Test Strategy: crt-027 (WA-4 Proactive Knowledge Delivery)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `source` field addition to `HookRequest::ContextSearch` — existing struct-literal test constructors that omit `source` fail to compile, or silently pass `source: None` through without asserting correct tagging | High | High | Critical |
| R-02 | `BriefingService` deletion causes silent ranking degradation — `EffectivenessStateHandle` wiring is a compile-time guarantee, but if `IndexBriefingService` initializes its `cached_snapshot` with a stale or zero-generation snapshot it silently scores all entries as equally effective | High | Med | High |
| R-03 | `format_compaction_payload` test loss — 10+ tests are rewritten; if any invariant (budget enforcement, UTF-8 safety, histogram block, session context header, confidence sort) is omitted from the new test set, the regression is invisible until production misuse | High | High | Critical |
| R-04 | `MIN_QUERY_WORDS = 5` boundary — an off-by-one means 4-word prompts route to `ContextSearch` (injection noise) or 5-word prompts route to `RecordEvent` (missed injection). Exact boundary must be confirmed by dedicated tests | High | Med | High |
| R-05 | CompactPayload flat index format contract broken for WA-5 — `format_index_table` column layout, separator character, or `SNIPPET_CHARS` constant shifts during implementation; WA-5 prepend breaks silently because both sides compile without knowing about the change | High | Low | High |
| R-06 | Query derivation three-step fallback diverges between MCP and UDS call sites — if `derive_briefing_query` is not actually a shared helper (or if one call site bypasses it), session-synthesized queries may work for MCP but not CompactPayload or vice versa | High | Med | High |
| R-07 | SubagentStart stdout injection unverified (SR-01) — if Claude Code ignores SubagentStart hook stdout, WA-4a delivers zero injection value while the system reports no error; the feature silently underdelivers | High | Med | High |
| R-08 | `mcp-briefing` feature-flag split — `IndexBriefingService` compiles unconditionally, but MCP-path tests inside `#[cfg(feature = "mcp-briefing")]` blocks are skipped in `cargo test` without the flag; briefing tool behaviour untested in that build profile | Med | Med | Medium |
| R-09 | `UNIMATRIX_BRIEFING_K` env var silently reduces k — if `IndexBriefingService` inadvertently reads the env var (e.g., inherited from a shared `parse_semantic_k()` call site that was not fully removed), k=3 in production defeats the k=20 design | Med | Low | Medium |
| R-10 | Cold-state query derivation (step 3 topic fallback) returns zero results — feature ID string `"crt-027"` as the sole query may match no knowledge-base entries if none are tagged with that topic, causing an empty briefing at session start | Med | Med | Medium |
| R-11 | SM delivery protocol update incomplete — if `context_briefing` is added at some but not all six phase-boundary points, or if `max_tokens: 1000` is omitted at any site, SM agents at certain phases start without knowledge packages | Med | Med | Medium |
| R-12 | Observation `hook` column mismatch — `dispatch_request` uses `source.as_deref().unwrap_or("UserPromptSubmit")` but if the destructuring omits `source` from the match arm (easy oversight in a large match), all SubagentStart observations are permanently tagged as UserPromptSubmit | Med | Med | Medium |
| R-13 | `HookRequest::Briefing` wire variant inadvertently removed — spec explicitly forbids removal of this variant in C-04; a misread during `briefing.rs` deletion could extend to `wire.rs` and silently break any future code path that matches on this variant | Low | Low | Low |
| R-14 | Empty CompactPayload result when histogram is also empty — `format_compaction_payload` should return `None`; if it returns `Some("")` or an empty table header, WA-5 receives a malformed payload | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: `source` Field Addition — Wire Protocol Backward Compat
**Severity**: High
**Likelihood**: High
**Impact**: Compile failures in existing tests or, worse, tests that compile but silently pass wrong `source` values into observations, corrupting retrospective tagging data. Lesson #885 confirms serde-heavy types routinely hide coverage gaps at gate reviews.

**Test Scenarios**:
1. Deserialize a `HookRequest::ContextSearch` JSON blob that omits the `source` key entirely; assert the resulting struct has `source == None` and `dispatch_request` tags the observation as `"UserPromptSubmit"`.
2. Deserialize a JSON blob with `"source": "SubagentStart"`; assert `source == Some("SubagentStart")` and observation is tagged `"SubagentStart"`.
3. Round-trip: serialize a `HookRequest::ContextSearch` with `source: None`, re-deserialize; assert field survives round-trip as `None` (not absent key causing parse error).
4. All existing `hook.rs` and `wire.rs` tests that construct `HookRequest::ContextSearch` via struct literal compile with `source: None` or `..` spread — `cargo build --release` emits no `non_exhaustive` errors.
5. `listener.rs` integration test: `ContextSearch` with `source: None` writes `hook = "UserPromptSubmit"` to observations table; `source: Some("SubagentStart")` writes `hook = "SubagentStart"`.

**Coverage Requirement**: Deserialization default, explicit set, round-trip, observation column tagging, and compile-time struct-literal update — all five scenarios required before gate.

---

### R-02: `EffectivenessStateHandle` Wiring — Silent Ranking Degradation
**Severity**: High
**Likelihood**: Med
**Impact**: Effectiveness-based ranking silently reverts to a zero-generation snapshot, causing `IndexBriefingService` to score all entries as equally effective and ignore the crt-018b ranking signal. This would be undetectable without a specific test. ADR-004 crt-018b (#1546) established the non-optional constructor requirement precisely because silent degradation occurred before.

**Test Scenarios**:
1. Construct `IndexBriefingService` with a mock `EffectivenessStateHandle`; call `index()` with two entries that differ only in effectiveness snapshot; assert the higher-effectiveness entry ranks before the lower one.
2. Confirm `ServiceLayer::with_rate_config()` passes `Arc::clone(&effectiveness_state)` (not a fresh default) to `IndexBriefingService::new()` — code inspection test or integration test comparing `IndexBriefingService` results before and after an effectiveness update tick.
3. `IndexBriefingService::new()` must be a compile error if `effectiveness_state` is omitted — verified by the Rust type system (no runtime test needed, but must be confirmed by attempting omission in a negative compile test or code review gate).

**Coverage Requirement**: At minimum one integration test showing ranking is influenced by effectiveness state; one compile-time verification that the parameter is non-optional.

---

### R-03: `format_compaction_payload` Test Invariant Coverage
**Severity**: High
**Likelihood**: High
**Impact**: Any omitted invariant (budget enforcement, UTF-8 boundary, histogram block, session context header, confidence sort, entry ID presence) becomes a silent regression. The ADR-004 table lists 11 test replacements; if the implementer deletes the old tests without writing matching new ones, `cargo test` passes on a broken formatter. Lesson #2758 (gate-3c non-negotiable test names) is directly applicable — these 11 new test names must be verified to exist.

**Test Scenarios**:
1. `format_payload_empty_entries_returns_none` — `format_compaction_payload(&[], ...)` returns `None`.
2. `format_payload_header_present` — output starts with `"--- Unimatrix Compaction Context ---\n"`.
3. `format_payload_sorted_by_confidence` — two entries with scores 0.90 and 0.30 produce row 1 = 0.90, row 2 = 0.30.
4. `format_payload_budget_enforcement` — output with large entries and small `max_bytes` cap: `result.len() <= max_bytes`.
5. `format_payload_multibyte_utf8` — CJK content (`"\u{4e16}\u{754c}".repeat(200)`) truncates at a valid char boundary; `snippet.is_char_boundary(snippet.len())` holds.
6. `format_payload_session_context` — Role/Feature/Compaction# lines present when provided.
7. `format_payload_active_entries_only` — output contains only Active-status entries; no Deprecated marker appears.
8. `format_payload_entry_id_metadata` — entry ID appears in flat table `id` column.
9. `format_payload_token_limit_override` — `max_bytes = 400`, large content: `output.len() <= 400`.
10. `test_compact_payload_histogram_block_present` — non-empty histogram produces `"Recent session activity:"` block.
11. `test_compact_payload_histogram_block_absent` — empty histogram produces no histogram block.

**Coverage Requirement**: All 11 test names must exist in the final implementation and must pass. Gate reviewer must grep for each name — not accept a PASS claim without name verification.

---

### R-04: `MIN_QUERY_WORDS = 5` Boundary
**Severity**: High
**Likelihood**: Med
**Impact**: Off-by-one means either short prompts inject noise (4-word route to ContextSearch) or 5-word prompts are silently dropped (route to RecordEvent). Neither failure is observable at runtime without a specific boundary test.

**Test Scenarios**:
1. `UserPromptSubmit` with exactly 4 whitespace-delimited words → `build_request` returns `RecordEvent` (not `ContextSearch`).
2. `UserPromptSubmit` with exactly 5 whitespace-delimited words → `build_request` returns `ContextSearch`.
3. `UserPromptSubmit` with 1 word → `RecordEvent`.
4. `UserPromptSubmit` with 6 words → `ContextSearch`.
5. `SubagentStart` with 1-word non-empty `prompt_snippet` → `ContextSearch` (word guard does not apply to SubagentStart).
6. `SubagentStart` with empty `prompt_snippet` → `RecordEvent` (empty-string guard still applies).

**Coverage Requirement**: Tests 1 and 2 are the non-negotiable boundary cases. Tests 3–6 confirm the guard is scoped to `UserPromptSubmit` only.

---

### R-05: CompactPayload Format Contract for WA-5
**Severity**: High
**Likelihood**: Low
**Impact**: WA-5 is a downstream feature that prepends transcript content before the flat index table. If `format_index_table` output format shifts (column widths, separator character, row field width) after WA-5 is designed against it, WA-5 will need rework. The typed `IndexEntry` struct (ADR-005) mitigates compile-time signature changes but does not protect against formatting-level changes.

**Test Scenarios**:
1. `format_index_table` on a single `IndexEntry` produces a string containing the header line `"#    id   topic"` (or equivalent column headers) followed by a separator line and one data row.
2. The data row for an entry with `id=2, topic="product-vision", category="decision", confidence=0.60, snippet="Unimatrix is..."` matches the exact column layout specified in FR-12 — row number right-justified, confidence formatted as `"0.60"`.
3. `SNIPPET_CHARS` constant exists in `mcp/response/briefing.rs` and equals 150.
4. `format_index_table(&[])` returns an empty string (not a header-only string that confuses a caller testing for emptiness).

**Coverage Requirement**: Format contract test (scenario 2) must assert the literal output string for the documented example row — any column shift breaks this test and flags a WA-5 integration risk before it becomes a downstream problem.

---

### R-06: Query Derivation Three-Step Fallback — Both Call Sites
**Severity**: High
**Likelihood**: Med
**Impact**: If the MCP path and UDS path use slightly different derivation logic (e.g., one checks `task.is_empty()` and the other checks `task.len() > 0`), the two paths diverge silently. Different queries produce different results for semantically identical inputs.

**Test Scenarios**:
1. `derive_briefing_query(task: Some("implement spec writer"), session_state: None, topic: "crt-027")` → returns `"implement spec writer"` (step 1).
2. `derive_briefing_query(task: None, session_state: Some(state_with_signals), topic: "crt-027")` where `state.feature_cycle = "crt-027/spec"` and top 3 signals are `["briefing", "hook", "compaction"]` → returns `"crt-027/spec briefing hook compaction"` (step 2).
3. `derive_briefing_query(task: None, session_state: Some(state_with_empty_signals), topic: "crt-027")` → returns `"crt-027"` (step 3 fallback).
4. `derive_briefing_query(task: None, session_state: None, topic: "crt-027")` → returns `"crt-027"` (step 3 fallback, no session).
5. `derive_briefing_query(task: Some(""), ...)` — empty string task is treated as absent; falls to step 2 or 3.
6. Code inspection confirms a single shared helper function is called by both the MCP tool handler and `handle_compact_payload` — not two independently written derivation blocks.

**Coverage Requirement**: All four derivation paths (task present, synthesized, empty-signals, no-session) tested against the shared helper directly. Code review gate: confirm a single function call site in both callers.

---

### R-07: SubagentStart Stdout Injection — SR-01 Manual Smoke Test
**Severity**: High
**Likelihood**: Med
**Impact**: If Claude Code does not inject SubagentStart hook stdout into the subagent context, WA-4a's entire injection value proposition fails silently. The server-side changes (observation recording, topic_signal extraction) still work, so no error surfaces. The feature ships but subagents still receive zero knowledge at spawn. Graceful degradation is designed in (ARCHITECTURE.md SR-01 section), but the primary value path is unconfirmed.

**Test Scenarios**:
1. **Manual smoke test (non-automated)**: Spawn a subagent with a `prompt_snippet` that should match a known Unimatrix entry (e.g., `"implement the specification writer agent for crt-027"`). After the subagent starts, verify whether the Unimatrix injection text appears in its initial context. If it does not, file a spike issue against Claude Code SubagentStart hook behavior.
2. Unit test (automated): `build_request("SubagentStart", input_with_prompt_snippet)` returns `HookRequest::ContextSearch` (not `RecordEvent`) — confirming the server-side routing is correct regardless of stdout behavior.
3. Integration test (automated): `dispatch_request` with a `ContextSearch { source: "SubagentStart" }` request writes a response to the socket; `write_stdout()` is called with non-empty content when search results exist.
4. Verify the hook exits 0 even when the UDS server is unavailable — the graceful degradation path must be exercised in a test where the server returns an error.

**Coverage Requirement**: Scenario 1 is a MANUAL gate item. Scenarios 2–4 are automated. AC-SR01 in the spec must be explicitly marked OPEN or CONFIRMED before Gate 3c.

---

### R-08: `mcp-briefing` Feature Flag Split
**Severity**: Med
**Likelihood**: Med
**Impact**: `IndexBriefingService` compiles unconditionally (correct), but MCP-layer briefing tests inside `#[cfg(feature = "mcp-briefing")]` blocks do not run in the default `cargo test` invocation. If CI only runs `cargo test` without the feature flag, the MCP `context_briefing` tool handler changes are never exercised in CI.

**Test Scenarios**:
1. `cargo test` (without `--features mcp-briefing`) passes all `handle_compact_payload` tests, confirming the always-compiled path works.
2. `cargo test --features mcp-briefing` passes the `context_briefing` MCP tool tests (AC-06, AC-07, AC-08, AC-09, AC-11).
3. CI pipeline invokes at minimum one test run with `--features mcp-briefing` before merge.

**Coverage Requirement**: Both build profiles must be exercised in CI. The delivery agent must confirm CI configuration in the PR.

---

### R-09: `UNIMATRIX_BRIEFING_K` Env Var — Silent k Reduction
**Severity**: Med
**Likelihood**: Low
**Impact**: If `parse_semantic_k()` is not fully deleted and is inadvertently called somewhere in the new code path (e.g., a leftover reference in `services/mod.rs`), a production deployment with `UNIMATRIX_BRIEFING_K=3` returns 3 entries instead of 20, silently defeating the high-k design.

**Test Scenarios**:
1. Set `UNIMATRIX_BRIEFING_K=3` in the test environment; call `IndexBriefingService::index()` with a k=20 default; assert the result contains up to 20 entries (not capped at 3).
2. Code inspection: `grep -r "UNIMATRIX_BRIEFING_K" crates/unimatrix-server/src/` returns zero matches in production code paths (only the deprecation comment and this test).
3. `parse_semantic_k()` function no longer exists — `grep -r "parse_semantic_k" crates/` returns no results.

**Coverage Requirement**: Scenario 1 is the runtime guard. Scenarios 2 and 3 are static code-inspection gates that the gate reviewer must run.

---

### R-10: Cold-State Query Derivation — Topic Fallback Quality
**Severity**: Med
**Likelihood**: Med
**Impact**: At session start (no topic_signals yet accumulated), `derive_briefing_query` falls to step 3 and uses the `topic` parameter string (e.g., `"crt-027"`). If no knowledge-base entries are tagged with that topic, `IndexBriefingService::index()` returns zero results — an empty briefing at the most important moment (session start). This is the lowest-confidence path with the highest need.

**Test Scenarios**:
1. Call `IndexBriefingService::index()` with `topic = "nonexistent-feature-id-xyz"` and an empty session state; assert the result is an empty `Vec<IndexEntry>` and the function returns `Ok(vec![])` without error (not a panic or `Err`).
2. Call `context_briefing` MCP tool with `topic = "crt-027"` and no `session_id` against a populated test database; verify the result is non-empty (at least one active entry matches the topic or its semantic neighborhood).
3. Verify that an empty result on step 3 does not cause `format_index_table` to produce malformed output — it should return an empty string, and `format_compaction_payload` should return `None` when both entries and histogram are empty (AC-18).

**Coverage Requirement**: Step 3 empty-result path must be exercised; graceful empty-result handling confirmed.

---

### R-11: SM Delivery Protocol Update — Completeness
**Severity**: Med
**Likelihood**: Med
**Impact**: If `context_briefing` is missing at one of the six phase-boundary points, agents starting that phase receive no knowledge package. The SM protocol is a text file — missing an insertion point is a human error that `cargo test` cannot catch.

**Test Scenarios**:
1. Static verification: `grep -c "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md` returns at least 6.
2. Visual diff of the protocol file confirms insertions immediately after: (a) `context_cycle(type: "start", ...)`, (b) `context_cycle(type: "phase-end", phase: "spec", ...)`, (c) `context_cycle(type: "phase-end", phase: "spec-review", ...)`, (d) `context_cycle(type: "phase-end", phase: "develop", ...)`, (e) `context_cycle(type: "phase-end", phase: "test", ...)`, (f) `context_cycle(type: "phase-end", phase: "pr-review", ...)`.
3. Every `context_briefing` call in the protocol specifies `max_tokens: 1000`.

**Coverage Requirement**: All six insertion points must be verified. A static grep count of 6 is the minimum gate. AC-14 requires a diff showing all six sites.

---

### R-12: Observation `hook` Column Mismatch
**Severity**: Med
**Likelihood**: Med
**Impact**: If the `source` field is not correctly destructured from `HookRequest::ContextSearch` in the `dispatch_request` match arm, the `source` variable will be uninitialized or shadowed by a default, and all SubagentStart observations will be permanently mislabeled as `"UserPromptSubmit"`. Lesson #699 (silent data orphaning from hardcoded None in hook pipeline) is the exact historical analogue.

**Test Scenarios**:
1. Integration test: submit `HookRequest::ContextSearch { source: Some("SubagentStart"), ... }` to `dispatch_request`; query the observations table; assert `hook = "SubagentStart"`.
2. Integration test: submit `HookRequest::ContextSearch { source: None, ... }` to `dispatch_request`; assert `hook = "UserPromptSubmit"`.
3. Integration test: submit a JSON blob with `source` key omitted entirely; assert `hook = "UserPromptSubmit"` (backward compat path through serde default).

**Coverage Requirement**: All three observation tagging paths must be tested (explicit SubagentStart, explicit None, deserialized-absent).

---

## Integration Risks

**IR-01: SubagentStart fires in parent session context.** The `session_id` passed to `ContextSearch` is the parent session's ID, not a new session. `handle_context_search` must resolve the histogram from the parent session registry entry. If the parent session has been partially torn down by the time SubagentStart fires (e.g., during a fast spawn), the histogram lookup may return empty and WA-2 boost is silently skipped. Test: send `ContextSearch { source: "SubagentStart", session_id: "known_parent_session" }` after the parent session is registered; verify WA-2 histogram boost applies.

**IR-02: `SearchService` status filter.** `IndexBriefingService` passes `status = Active` to `SearchService`. If `SearchService` does not honor a status filter parameter, deprecated entries leak through. This is not a new SearchService behaviour — the filter was present in `BriefingService` — but it must be explicitly verified after the refactor. Test: AC-06 (one active + one deprecated entry; briefing returns only the active one).

**IR-03: `ServiceLayer` field rename compile surface.** Pattern #2938 documents that adding a new `Arc` parameter to `ServiceLayer::new()` requires updating 5+ call sites. The field rename from `briefing: BriefingService` to `briefing: IndexBriefingService` is a type change, not an addition, but it touches the same construction block. Test: `cargo build --release` passes with zero type errors; integration test harness construction sites compile.

**IR-04: `handle_compact_payload` session state access.** The UDS path already holds `session_state` directly; it must NOT make a `SessionRegistry` lookup for step 2 of query derivation. If the shared `derive_briefing_query` helper is designed for the MCP path (with a registry lookup parameter) and incorrectly used on the UDS path, the UDS path performs an unnecessary and potentially incorrect registry lookup. Test: AC-10 (code inspection confirming no registry lookup in the UDS path).

---

## Edge Cases

**EC-01: `prompt_snippet` contains only whitespace.** `SubagentStart` with `prompt_snippet = "   "` — `input.extra["prompt_snippet"]` is non-empty but after `unwrap_or("")` the query string is `"   "`. The empty-string guard (`if query.is_empty()`) may not catch this. If `"   "` is passed to `SearchService`, it may embed as a near-zero vector and return arbitrary entries. The spec's empty-string guard should use `query.trim().is_empty()` or the existing guard should be confirmed to handle whitespace-only strings.

**EC-02: `prompt_snippet` is a JSON `null` value.** `input.extra["prompt_snippet"]` may be `serde_json::Value::Null`. The `and_then(|v| v.as_str())` chain in `build_request` returns `None` for `Null`, so `unwrap_or("")` produces an empty query and `generic_record_event` is called. This is correct behavior — verify it explicitly.

**EC-03: `k` parameter of 0 passed to `IndexBriefingService`.** If a caller passes `k=0`, `SearchService` may panic or return an error. The service should clamp `k` to a minimum of 1, or the caller should validate the parameter before dispatch.

**EC-04: `IndexEntry.snippet` with exactly 150 chars of multi-byte content.** If the content has a 3-byte CJK character at position 148–150, `chars().take(150)` is safe (operates on char boundaries) but the byte length of the snippet may be up to 450 bytes. Tests must confirm the 150-char limit is chars, not bytes, and budget calculations account for the byte expansion.

**EC-05: `format_index_table` with entries whose `topic` or `category` is longer than column width.** Long topic strings must be truncated (or wrapped) without corrupting adjacent columns. The current prose spec does not specify a truncation rule for topic/category columns in the table — the implementation must choose one and the test must assert it.

**EC-06: `derive_briefing_query` step 2 — `topic_signals` contains only one or two entries.** When fewer than 3 topic signals are available, the synthesized query must still be well-formed. Test: `state.topic_signals = [("briefing", 5)]` → query is `"crt-027/spec briefing"` (feature_cycle + 1 signal, not `"crt-027/spec briefing  "` with trailing spaces).

---

## Security Risks

**SR-A: `prompt_snippet` is attacker-controlled input.** The `prompt_snippet` field in `SubagentStart` input comes from the spawning prompt — which an attacker-crafted task could control. If `prompt_snippet` contains SQL-injection or embedding-manipulation payloads, they pass through `build_request` into `SearchService`'s embedding pipeline. The existing `SecurityGateway` wraps search calls and must be confirmed to sanitize or rate-limit this input. Blast radius: malformed query could cause embedding panics or excessive database queries. Test: send a `SubagentStart` event with `prompt_snippet` containing SQL metacharacters (`'; DROP TABLE ENTRIES; --`) and verify the server returns a normal (possibly empty) result, not an error or panic.

**SR-B: Untrusted topic strings in `derive_briefing_query`.** The `topic` parameter (step 3 fallback) and `task` parameter (step 1) arrive from MCP tool callers. These strings are embedded into search queries. They must not bypass `SecurityGateway` authorization checks. Confirm that `IndexBriefingService::index()` routes through `SecurityGateway` for all three query derivation paths, not only when `session_id` is present.

**SR-C: `source` field injection.** A caller could set `source: Some("arbitrary string")` to inject arbitrary values into the observations table `hook` column. The observations table `hook` column should have a defined domain (e.g., `VARCHAR` with a max length). An unbounded string insert from an untrusted caller could cause storage issues or analytics corruption. The existing DB schema should be confirmed to have reasonable constraints on the `hook` column length.

---

## Failure Modes

**FM-01: UDS server unavailable during SubagentStart.** `transport.request()` returns an error. Hook process must catch the error, degrade to `generic_record_event` behavior (topic_signal recorded locally if possible, or silently dropped), and exit 0. Must not write a non-zero exit code or produce stderr output that causes Claude Code to fail the spawn. Test: AC-07 from hook.rs, plus a test simulating server unavailability.

**FM-02: `IndexBriefingService::index()` returns an error.** `handle_compact_payload` and `context_briefing` handler must handle a `ServiceError` gracefully — return an empty result or a partial result, not panic. `context_briefing` should return a `CallToolResult::error` message, not an MCP protocol error. `handle_compact_payload` should return `HookResponse::BriefingContent("")` or `None` payload.

**FM-03: `format_compaction_payload` exceeds byte budget with a single row.** If the first row of the flat table (header + row 1) already exceeds `max_bytes`, the function must truncate the row (not emit a partial table) and still return `Some(...)` with at least the header block. The current spec says "rows are truncated from the end (lowest-ranked dropped first)" — this assumes header + row 1 fits within the budget. A test should verify behavior when even row 1 exceeds the budget.

**FM-04: `SessionRegistry` lookup failure in MCP query derivation step 2.** If `SessionRegistry.get_session_state(session_id)` returns `None` (session expired or never registered), `derive_briefing_query` must fall to step 3 without error. The failure must be silent degradation to the topic fallback.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (SubagentStart stdout injection unverified) | R-07 | Graceful degradation designed (ARCHITECTURE.md §SR-01). Observation + topic_signal recorded regardless. Manual smoke test required at AC-SR01 before Gate 3c. |
| SR-02 (`mcp-briefing` flag test coverage) | R-08 | `IndexBriefingService` is unconditionally compiled (ADR-003). CompactPayload tests run without flag. MCP tests require `--features mcp-briefing` run in CI (AC-24). |
| SR-03 (`EffectivenessStateHandle` wiring loss) | R-02 | `effectiveness_state` is a required, non-optional constructor parameter on `IndexBriefingService::new()` — missing wiring is a compile error (ADR-003, spec FR-15). |
| SR-04 (`format_compaction_payload` test loss) | R-03 | ADR-004 enumerates 11 test replacements by name. Spec AC-16 through AC-21 enumerate surviving invariants. Gate reviewer must grep for test names. |
| SR-05 (`UNIMATRIX_BRIEFING_K` undefined fate) | R-09 | Env var explicitly deprecated and not read by `IndexBriefingService` (ADR-003, spec FR-13, C-08). `parse_semantic_k()` deleted. Runtime test with env var set confirms no effect. |
| SR-06 (WA-5 format contract under-specified) | R-05 | Resolved by typed `IndexEntry` struct + `format_index_table` function + `SNIPPET_CHARS` constant (ADR-005). Compile-time stable. Format contract test (R-05 scenario 2) guards against formatting drift. |
| SR-07 (Wire protocol struct literal breakage) | R-01 | `#[serde(default)]` on `source: Option<String>` — all existing deserializers unaffected (ADR-001). Struct-literal tests require `source: None` addition — compile error if omitted (visible, not silent). |
| SR-08 (Cold-state query derivation) | R-10 | Step 3 fallback to `topic` param always available. Empty result is graceful (`Ok(vec![])`). Test confirms no panic and no malformed output (R-10, AC-18). |
| SR-09 (SM context budget) | R-11 | `max_tokens: 1000` cap specified in all six protocol call sites (spec NFR-07, AC-14). Static grep verification gate. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-03) | 5 + 11 = 16 scenarios minimum |
| High | 5 (R-02, R-04, R-05, R-06, R-07) | 3 + 6 + 4 + 6 + 4 = 23 scenarios |
| Medium | 5 (R-08, R-09, R-10, R-11, R-12) | 3 + 3 + 3 + 3 + 3 = 15 scenarios |
| Low | 2 (R-13, R-14) | 1 + 1 = 2 scenarios minimum |

**Non-negotiable tests** (must be verified by name at Gate 3c, per lesson #2758):
- `format_payload_empty_entries_returns_none`
- `format_payload_sorted_by_confidence`
- `format_payload_budget_enforcement`
- `format_payload_multibyte_utf8`
- `format_payload_active_entries_only`
- `test_compact_payload_histogram_block_present`
- `test_compact_payload_histogram_block_absent`
- `build_request_subagentstart_with_prompt_snippet` (AC-01)
- `build_request_subagentstart_empty_prompt_snippet` (AC-02)
- `build_request_userpromptsub_four_words_record_event` (AC-22)
- `build_request_userpromptsub_five_words_context_search` (AC-22)

**Manual gate item**: AC-SR01 (SubagentStart stdout injection confirmation) must be marked OPEN or CONFIRMED in the test plan before Gate 3c approval.

---

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned gate failures — found #2758 (gate-3c non-negotiable test name verification), #699 (silent hook pipeline data orphaning), #885 (serde-heavy type test coverage gaps) — all directly inform R-01, R-03, R-12 severity elevations.
- Queried: `/uni-knowledge-search` for risk patterns — found #2938 (ServiceLayer Arc call-site count), #3180 (SessionState struct-literal test helper updates), #646 (serde(default) backward compat) — used to calibrate R-01 and R-03 likelihoods.
- Queried: `/uni-knowledge-search` for EffectivenessStateHandle + BriefingService — found #1546 (ADR-004 crt-018b, non-optional constructor) — directly confirms R-02 scenario design.
- Stored: nothing novel to store — patterns involved are already captured in entries #699, #885, #2758, #3180.
