## ADR-003: Deprecation Idempotency

### Context

When `context_deprecate` is called on an already-deprecated entry, should it return an error or succeed silently? Agents may retry failed operations, or multiple agents may independently decide to deprecate the same entry.

### Decision

Deprecating an already-deprecated entry is a no-op that returns success. The response indicates the entry is deprecated (same format as a successful deprecation). No audit event is logged for the no-op case -- the original deprecation audit event is sufficient.

The idempotency check happens before entering the write transaction: read the entry via `entry_store.get(id)`, check status, and return early if already deprecated. This avoids unnecessary write transaction overhead.

### Consequences

**Easier:**
- Retry-safe: agents can safely retry deprecation without error handling
- Multiple agents can independently deprecate without coordination
- Simpler agent code: no need to check entry status before deprecating

**Harder:**
- Silent success may mask unintended double-deprecations (mitigated: the audit log records the original deprecation)
- The check-then-act pattern has a TOCTOU window (two agents deprecating simultaneously), but this is harmless: the write transaction serializes the actual status change, and the second write would be a no-op within the transaction
