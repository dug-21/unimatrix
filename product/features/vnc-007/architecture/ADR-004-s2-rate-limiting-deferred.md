## ADR-004: S2 Rate Limiting Deferred to vnc-009

### Context

The SCOPE.md includes S2 rate limiting on knowledge writes (60/hour per caller, closing F-09) as a conditional item — the architect has authority to include or defer.

Rate limiting requires:
- A `RateLimiter` struct in SecurityGateway (in-memory HashMap of caller_id -> timestamps)
- A new `ServiceError::RateLimited` variant
- StoreService calling `gateway.check_write_rate()` before insert/correct
- Internal caller exemption (AuditSource::Internal skips rate check)
- Tests for the rate limiter, exemption logic, and error propagation

This is ~80-100 lines of self-contained code. It touches SecurityGateway and StoreService (both vnc-006 components) but has zero interaction with BriefingService.

### Decision

Defer S2 rate limiting to vnc-009 (Cross-Path Convergence), which already plans rate limiting on search (300/hour per caller). Implementing both write and search rate limiting together allows a single RateLimiter abstraction with configurable per-operation limits.

Rationale:
1. **No overlap with BriefingService**: Rate limiting is a write-path concern. BriefingService is a read-path concern. Including both in vnc-007 creates a PR that touches unrelated code paths.
2. **vnc-009 is the natural home**: vnc-009 scopes both write rate limiting (F-09) and search rate limiting in a single feature. A unified RateLimiter is simpler than building it in two phases.
3. **Current risk is low**: Only MCP agents with Write capability can perform knowledge writes, and they are authenticated via the agent registry. The capability check provides some protection against abuse.

### Consequences

- **Easier**: vnc-007 scope is smaller and focused entirely on briefing unification.
- **Easier**: vnc-009 can design a unified RateLimiter for both writes and searches.
- **Harder**: F-09 remains open until vnc-009 ships. If an agent malfunctions and floods writes, there is no throttle.
- **Accepted risk**: The Write capability check limits the blast radius. A misbehaving agent would need Write permission to flood, and the audit log (S5) records all writes for forensic analysis.
