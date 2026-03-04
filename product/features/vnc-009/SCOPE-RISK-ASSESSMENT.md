# Scope Risk Assessment: vnc-009

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `AccessSource` routing inside `UsageService::record_access` combines fundamentally different operations (MCP access counts/votes vs UDS injection logs/co-access) in one method — logic complexity and subtle behavior divergence if routing is wrong | Med | Med | Exhaustive match on `AccessSource` with no fallthrough. Each variant calls distinct internal methods. Unit tests per variant verifying exact store operations triggered. |
| SR-02 | Sliding window rate limiter with `Mutex<HashMap<CallerId, SlidingWindow>>` contention under high concurrent MCP load — all search/write paths acquire the lock | Med | Low | Keep the critical section minimal (timestamp append + expired entry eviction). Consider `parking_lot::Mutex` if std Mutex proves too slow. Benchmark under 100 concurrent requests. |
| SR-03 | `#[derive(Serialize)]` on `StatusReport` requires `Serialize` derives on types owned by `infra/contradiction.rs` (`ContradictionPair`, `EmbeddingInconsistency`) — cross-module serde dependency propagation | Low | High | Add `Serialize` derives to the infra types. Verify `serde` is already a dependency of the server crate (it is via rmcp). No new external dependencies needed. |
| SR-04 | `serde_json::to_value()` field ordering differs from manual `json!` macro ordering — JSON consumers that depend on key order (even though JSON spec says order is undefined) could break | Low | Low | Run a snapshot test comparing old manual JSON output against new `to_value()` output. Document that JSON key order is not guaranteed. Use `#[serde(rename)]` only for naming mismatches, not ordering. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | UsageService absorbs `record_usage_for_entries()` from `server.rs` — this is a 80-line method with vote correction logic (NewVote, CorrectedVote, NoOp) and confidence recomputation coupling. Moving it must preserve exact vote semantics. | High | Med | Extract the method body into UsageService with zero logic changes. The internal function signature and behavior should be identical; only the call site changes. Add vote correction integration tests (new vote, changed vote, duplicate vote). |
| SR-06 | UDS session ID prefixing (`uds::`) is a behavioral change for existing injection logs and co-access pairs — entries written with raw session IDs will not match queries using prefixed session IDs | High | Med | Architect must decide: (a) prefix only new sessions, (b) prefix all and accept the break, or (c) prefix at service boundary but store unprefixed. Recommendation: prefix at service boundary, strip prefix before storage writes. Services see prefixed IDs; storage sees raw IDs. |
| SR-07 | MCP `session_id` sourced from hooks introduces a dependency on hook augmentation of MCP requests — if hooks are not active, `session_id` will always be `None`, reducing the feature to a no-op | Low | Med | Accept this as expected behavior. Document that `session_id` requires active hook integration. The backward-compatible `None` path is the correct default. |
| SR-08 | BriefingService rate limiting (AC-24: `check_search_rate` when `include_semantic=true`) means briefing calls count against the same 300/hr search budget as explicit `context_search` calls — agents doing frequent searches AND briefings could hit limits unexpectedly | Med | Med | Architect should consider whether briefing should have a separate rate bucket or share the search bucket. Document the interaction in the specification. Consider the rate impact: typical agent sessions issue 1 briefing + many searches. |
| SR-09 | `CallerId::ApiKey(String)` variant is future-proofing for HTTP transport that does not yet exist — adding unused enum variants affects match exhaustiveness and increases surface area for no current benefit | Low | Low | Include the variant as a forward declaration but mark with `#[allow(dead_code)]` or document as future. Alternatively, defer the ApiKey variant entirely and add it when HTTP transport ships. Architect decides. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-10 | UsageService must be accessible from both `mcp/tools.rs` and `uds/listener.rs` — the `ServiceLayer` struct must hold it alongside existing services, but adding it changes the constructor signature and all test setups that create `ServiceLayer` | Med | High | Follow the existing pattern (SearchService, StoreService, etc. are already in ServiceLayer). Add `UsageService` to the ServiceLayer struct. Test helpers that construct ServiceLayer must be updated — plan for this in implementation. |
| SR-11 | `AuditLog` must be passed to UDS connection handler for auth failure audit (AC-38) — currently the handler receives individual Arcs for store, embed, vector, etc. Adding another Arc parameter increases handler function signature complexity | Low | High | Consider bundling handler dependencies into a context struct (similar to ToolContext for MCP). Alternatively, just add `Arc<AuditLog>` as another parameter — the handler already takes 8+ parameters. |
| SR-12 | Rate limiter interacts with SecurityGateway which is shared across services via `Arc` — the `Mutex` inside `RateLimiter` must not create a lock ordering issue with any other Mutex in the system (e.g., `UsageDedup`, `SessionRegistry`) | Med | Low | Ensure RateLimiter lock is acquired and released within a single synchronous call (no async across lock). Never hold RateLimiter lock while acquiring another lock. Document lock ordering if multiple locks are needed. |
| SR-13 | Fire-and-forget usage recording via `spawn_blocking` in UsageService — if UsageService spawns tasks the same way `record_usage_for_entries` does, the task must capture all necessary Arcs (store, usage_dedup) without holding references to UsageService itself | Med | Med | Follow existing `spawn_blocking` pattern exactly: clone Arcs before spawn, move owned data into closure. Do not capture `&self` across spawn boundary. |

## Assumptions

1. **serde is already available in the server crate** — rmcp and other dependencies bring in serde. No new Cargo.toml dependency entries needed for `#[derive(Serialize)]` on StatusReport. (Ref: Constraint 2)
2. **Hooks augmenting MCP requests can inject `session_id`** — the mechanism for hooks to modify MCP tool parameters exists or will exist by the time vnc-009 ships. If not, `session_id` is always `None` and the feature degrades gracefully. (Ref: Resolved Question 2)
3. **ToolContext already carries AuditContext** — ToolContext from vnc-008 includes AuditContext construction. Adding `session_id` to AuditContext requires updating ToolContext construction, not creating a new mechanism. (Ref: SCOPE.md, Current Codebase State)
4. **Rate limiting exempt paths (UDS, Internal) are determined by CallerID variant matching** — no configuration or runtime policy needed. Exemption is structural. (Ref: AC-26, AC-27)

## Design Recommendations

1. **(SR-05)** Architect should preserve the exact `record_usage_for_entries` logic inside UsageService. This is a move-and-wrap, not a rewrite. The vote correction semantics (NewVote, CorrectedVote, NoOp) are subtle and must not change.
2. **(SR-06)** Architect must resolve session ID storage strategy: prefix at service boundary but strip before storage writes is the safest approach. This keeps internal storage consistent while preventing cross-transport session confusion at the API level.
3. **(SR-08)** Specification should document that briefing semantic search and explicit search share the same rate bucket. If this proves problematic in practice, a separate briefing rate bucket can be added later without breaking changes.
4. **(SR-10, SR-11)** Implementation brief should include a wave plan: StatusReport Serialize and UDS auth audit are independent and low-risk — do them first. UsageService and rate limiting have more integration points — do them second. Session-aware MCP depends on both — do it last.
5. **(SR-12)** Architect should specify that RateLimiter uses its own internal `Mutex` with no nesting. Rate limit checks are synchronous, fast (<1us), and never cross an async boundary while holding the lock.
