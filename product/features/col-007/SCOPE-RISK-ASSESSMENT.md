# Scope Risk Assessment: col-007

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Search pipeline extraction breaks existing MCP tools. Moving ~170 lines from `tools.rs` to `unimatrix-engine/src/search.rs` changes import paths, async context, and service access patterns. Any subtle behavioral difference causes regression in the MCP `context_search` tool. | High | Medium | Architect should define the extraction boundary as an ADR. The refactored MCP tool must call the exact same function as the UDS handler. Existing integration tests are the regression gate -- no test modification allowed. |
| SR-02 | UDS shared state expansion introduces tight coupling. `start_uds_listener()` currently takes `Arc<Store>` only. col-007 needs embed service, vector store, entry store, adapt service, and co-access infrastructure. Passing 6+ Arc parameters creates a fragile signature; bundling into a struct risks leaking MCP-server internals into `unimatrix-engine`. | High | Medium | Architect should design the shared state boundary carefully. The key tension: the search pipeline lives in `unimatrix-engine` but the services it needs are initialized in `unimatrix-server`. Consider a trait-based abstraction or context struct that lives in engine. |
| SR-03 | Token budget heuristic (4 bytes/token) systematically over- or under-estimates. Real tokenizer ratios vary by content type (code ~3 bytes/token, English prose ~4-5 bytes/token, structured markdown with IDs/paths ~3.5 bytes/token). A 350-token target at 4 bytes/token = 1400 bytes, but actual token count could be 400-470 tokens for the same content. | Medium | High | Define the constant as `MAX_INJECTION_BYTES` (not tokens) to avoid confusion. Document the heuristic ratio. Accept that v1 overshoots; a real tokenizer is a future enhancement. |
| SR-04 | Cold ONNX pre-warming on SessionStart adds latency to session startup. The embedding call takes ~200ms. If SessionStart is fire-and-forget (hook exits immediately), the server warms in the background -- but if the first UserPromptSubmit fires before warming completes, it still hits cold path. | Medium | Medium | Server-side warming must be synchronous (block until model loaded) OR the UserPromptSubmit handler must check model readiness and skip injection if not ready. Architect should specify which approach. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Injection recording via RecordEvent diverges from col-010's INJECTION_LOG design. ASS-014 specifies typed InjectionRecord fields (hook_type, prompt_context, injection_reason) while RecordEvent uses unstructured `serde_json::Value` payload. The migration path (parse JSON into typed record) works but creates technical debt. | Low | High | **Flag for human**: this is a known minor divergence. The unstructured payload means col-010 must deserialize and re-store. Acceptable if the alternative (adding INJECTION_LOG now) pulls col-010 scope into col-007. The human explicitly chose RecordEvent -- flagging as requested. |
| SR-06 | Scope includes co-access pair generation from injected entries but the injection path has no session-scoped dedup. The MCP tool generates co-access pairs per-call. Hook injection fires on every prompt -- a 50-prompt session injects the same 3 entries 50 times, generating 150 redundant co-access pair writes. | Medium | High | Spec writer should require session-scoped co-access dedup (max 1 co-access recording per unique entry set per session). The server needs minimal session state even in col-007 (just a set of already-paired entry IDs). |
| SR-07 | Similarity floor (0.5) and confidence floor (0.3) are arbitrary constants with no empirical basis. The knowledge base has 53 active entries -- search quality at this scale may not benefit from aggressive filtering. Too-high thresholds could suppress all results; too-low thresholds inject noise. | Low | Medium | Make both constants configurable. Start with the proposed values. Plan for tuning after real-world observation. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | The `dispatch_request()` function must become async for ContextSearch (embedding requires tokio spawn_blocking). This changes the dispatcher from sync to async, affecting all existing request handlers (Ping, SessionRegister, SessionClose, RecordEvent). | Medium | High | Architect should assess whether to make the entire dispatcher async or use a hybrid approach (sync dispatch with async fallback for specific request types). The change is mechanical but touches all existing handlers. |
| SR-09 | `HookInput.prompt` field addition requires updating the defensive parsing tests (ADR-006). The field must work correctly when absent (non-UserPromptSubmit events) and when present but empty. | Low | Low | Straightforward -- `#[serde(default)]` handles this. Ensure test coverage for missing and empty prompt fields. |

## Assumptions

1. **Claude Code provides `prompt` field in UserPromptSubmit stdin** (SCOPE.md "Background Research"). Verified against [hooks reference](https://code.claude.com/docs/en/hooks). Low risk of invalidation.

2. **Plain text stdout injection is sufficient** (SCOPE.md "Key Design Choices"). The alternative `additionalContext` JSON format offers more control but adds complexity. If Claude Code changes how plain text is surfaced (e.g., hides it from agents), this assumption fails. Medium risk.

3. **ONNX embedding model warms in <300ms** (SCOPE.md "Latency Budget Analysis"). If model loading is slower (e.g., on resource-constrained CI environments), SessionStart pre-warming may not help fast enough. Low risk in production, medium risk in CI.

4. **Existing integration tests cover search pipeline behavior completely** (AC-12 zero regression). If the tests have gaps in coverage of the re-ranking or co-access boost steps, extraction could introduce subtle behavioral changes undetected by the test suite. Medium risk.

## Design Recommendations

1. **(SR-01, SR-02)** Architect should design the search pipeline extraction and UDS shared state as a single cohesive decision. The extraction boundary determines what services the UDS listener needs access to.

2. **(SR-04)** Architect should specify the warming strategy: background-async (risk of first-prompt miss) or blocking-with-readiness-check (simpler correctness, slightly slower SessionStart). Recommend blocking-with-readiness-check.

3. **(SR-06)** Spec writer should include session-scoped co-access dedup as a constraint. Without it, co-access data becomes noisy proportional to session length.

4. **(SR-08)** Architect should make the dispatcher fully async. The sync-to-async transition is mechanical and avoids a hybrid that would be harder to maintain.

5. **(SR-05)** Flag for human: RecordEvent injection recording is workable but creates a known migration point for col-010. The human chose this path; architect should document the migration expectation in an ADR if the approach is adopted.
