# Scope Risk Assessment: vnc-006

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Rust module visibility (`pub(crate)`) is not sufficient to prevent service bypass at compile time in vnc-006 — enforcement is deferred to vnc-008 module reorg | Med | Med | Document bypass prevention rules clearly. Add `#[cfg(test)]` integration tests that verify transports call services, not foundation directly. |
| SR-02 | `Store::insert_in_txn` exposes redb `WriteTransaction` in the service layer interface, coupling services to storage implementation | Med | Low | Keep `insert_in_txn` as `pub(crate)` and document that it is the sole write path. Avoid leaking `WriteTransaction` into service public API — wrap it. |
| SR-03 | `SecurityGateway` struct injection adds an `Arc<SecurityGateway>` to every service, increasing constructor complexity and test setup | Low | High | Provide a `SecurityGateway::new_for_test()` constructor with permissive defaults. Keep the struct lightweight. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | BriefingService is explicitly out of scope, but both MCP `context_briefing` and UDS `CompactPayload` use search internally — changes to SearchService could alter briefing behavior indirectly | High | Med | Verify briefing output is unchanged by running existing briefing tests against the new SearchService. Add regression tests if none exist. |
| SR-05 | "Like-for-like behavior" (AC-13, AC-14) is hard to verify for search ranking — floating-point re-ranking order could change with pipeline restructuring | Med | Med | Use snapshot tests with fixed seed data. Compare result ordering and scores, not just result sets. |
| SR-06 | Rate limiting (S2) is defined but not enforced in vnc-006 — the interface could lock in a design that vnc-009 needs to change | Low | Low | Define the `RateLimiter` trait/interface but do not implement. Mark as `#[allow(dead_code)]` or feature-gate. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | UDS fire-and-forget pattern means audit writes from SecurityGateway S5 must not block the UDS response path | High | Med | Audit writes must use the existing `spawn_blocking` fire-and-forget pattern. Never await audit completion in UDS handlers. |
| SR-08 | `ContentScanner` singleton (`OnceLock`) is currently initialized by MCP path — UDS path calling it for the first time could introduce initialization races | Low | Low | Verify `OnceLock::get_or_init` is thread-safe (it is by design). Add a test that initializes from multiple threads. |
| SR-09 | Moving search logic into SearchService changes the call graph for ~680 existing tests — tests that mock or stub search at the transport level will break | Med | High | Audit test structure before implementation. Identify tests that inline search logic vs. tests that call through handlers. |

## Assumptions

1. **Both transports use identical re-ranking weights** (0.85*similarity + 0.15*confidence) — if any divergence exists in the current code, unification will change one path's behavior. (Ref: SCOPE.md, Search Pipeline Duplication Analysis)
2. **Existing AUDIT_LOG table schema is sufficient** for AuditContext with session_id and feature_cycle — no schema version bump needed. (Ref: SCOPE.md, Constraint 8)
3. **`spawn_blocking` availability** — all service calls happen within a tokio runtime context, including UDS handlers. (Ref: SCOPE.md, Constraint 5)

## Design Recommendations

1. **(SR-04, SR-05)** Architect should define SearchService with exact parameter parity to both existing paths. Build a comparison test harness that runs the old and new paths side-by-side before ripping out the old code.
2. **(SR-07)** Architect should ensure AuditContext and SecurityGateway S5 writes are always fire-and-forget. Define this as a service-level invariant, not a per-call decision.
3. **(SR-09)** Spec writer should include an AC for test migration strategy — tests move with their code, no test deletions allowed.
