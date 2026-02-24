## ADR-007: FEATURE_ENTRIES Trust-Level Gating

### Context

SCOPE.md Decision #11 requires that only agents with Internal or higher trust level can write to FEATURE_ENTRIES. Restricted agents (read-only, auto-enrolled unknowns) have their `feature` parameter silently ignored. This preserves the read-only trust model: Restricted agents should not create analytics associations even though they can read knowledge.

The gating could happen at:
1. **Store layer**: `record_feature_entries` accepts a trust level parameter and conditionally writes.
2. **Server layer**: The server checks trust level before calling `record_feature_entries`.
3. **Tool handler layer**: Each tool handler individually checks trust before passing the feature param.

### Decision

Use Option 2: trust-level gating at the server layer, in the `record_usage_for_entries` method.

Rationale:
- **Single enforcement point.** `record_usage_for_entries` is the only code path that calls `record_feature_entries`. Checking trust here means one check covers all four retrieval tools.
- **Store stays trust-unaware.** The store crate has no dependency on trust level concepts. It writes what it's told to write. Trust is a server-layer concern.
- **Consistent with existing security patterns.** Capability checks already happen at the server layer (tools.rs). Trust-level gating for FEATURE_ENTRIES follows the same pattern.
- **Silent skip, not error.** When a Restricted agent provides a `feature` parameter, the retrieval succeeds normally. The feature parameter is simply not acted upon for FEATURE_ENTRIES. The agent is not informed of the skip -- this is intentional. Restricted agents have no legitimate reason to know whether their feature association was recorded.

The trust level is already resolved during identity resolution (step 1 of the retrieval flow). The `record_usage_for_entries` method accepts the trust level as a parameter, avoiding a second lookup.

### Consequences

- **Restricted agents get the same retrieval results.** The gating affects only the FEATURE_ENTRIES write, not the retrieval itself. AC-17 confirms this.
- **No error response for Restricted agents.** The feature parameter is silently ignored. This prevents information leakage about trust-level enforcement.
- **Single enforcement point for auditing.** If we later need to log trust-gating decisions, there is one place to add the logging.
- **Internal, Privileged, and System trust levels all write to FEATURE_ENTRIES.** The check is `trust_level >= Internal`, which includes all three higher levels.
- **`record_usage_for_entries` signature changes.** Gains a `trust_level: TrustLevel` parameter. All call sites (4 tool handlers + context_briefing) must pass the resolved trust level.
