# Scope Risk Assessment: crt-043

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | INSERT/UPDATE race for `goal_embedding`: the MCP-handler UPDATE may execute before the UDS-spawned INSERT completes, producing a silent no-op | High | High | Architect must choose and ADR one of the three resolution options in SCOPE.md; Option 1 or 2 are preferred — Option 3 (retry polling) adds hidden complexity |
| SR-02 | bincode blob is the first SQLite embedding blob in the codebase — no existing deserialization path exists; read sites in Group 6/7 must independently implement the inverse | Med | High | ADR must specify exact bincode API call and config so read sites cannot diverge; ship a `decode_goal_embedding()` helper alongside the write path |
| SR-03 | Fire-and-forget `tokio::spawn` adds another blocking-pool task per cycle start; under concurrent MCP load this can contribute to pool saturation (see entry #735) | Med | Med | Batch the embed write with any other fire-and-forget work at cycle start; do not open a separate Store Mutex acquisition just for this one UPDATE |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Two ADD COLUMN statements in a single v20→v21 migration step: if the migration partially applies (one column added, second fails), the schema version bump may still occur, leaving the DB in an inconsistent state | Med | Low | Wrap both ADD COLUMN statements in one `BEGIN`/`COMMIT` block; the `pragma_table_info` idempotency checks must cover both columns before either ALTER runs |
| SR-05 | `phase TEXT` has no allowlist — free-text from `context_cycle` events. If callers pass non-canonical values (e.g. "Design" vs "design"), Group 6 phase-stratification queries will silently produce sparse results | Med | Med | Spec should document canonical phase values and recommend a LOWER() normalization at write or query time |
| SR-06 | Scope explicitly excludes `(topic_signal, phase)` composite index from AC — it is flagged as "evaluate". If Group 6 is delivered without the index, full-table scans on `observations` will be a performance regression | Low | Med | Delivery agent must evaluate and decide at implementation time; do not defer to Group 6 |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | UDS listener access to `EmbedServiceHandle` is unverified — Option 1 (fire from UDS listener) depends on this being injectable. If the handle is not accessible there, Option 1 is unavailable and the architect must use Option 2 or 3 | High | Med | Architect must inspect the UDS listener construction path and confirm before committing to Option 1 |
| SR-08 | `goal_embedding` is NULL for all pre-v21 `cycle_events` rows. Group 6/7 features that aggregate goal embeddings will have sparse data for any historical cycle. Cold-start degradation is accepted in scope but downstream features may not account for it | Med | High | Group 6/7 spec must explicitly handle NULL `goal_embedding`; architect should note the cold-start gap in the ADR |

## Assumptions

- **SCOPE.md §Item B**: Assumes `cycle_id` (the `topic` string) is stable and unique at the time the MCP handler fires the embedding spawn. If `topic` can be absent or reused, the UPDATE target is ambiguous.
- **SCOPE.md §Proposed Approach, Item B step 2**: Assumes `EmbedServiceHandle` is accessible in the MCP handler context. The scope does not verify whether a handle clone is available at that call site.
- **SCOPE.md §Item C**: Assumes `SessionState.current_phase` is always set before the first observation write in a cycle. If a session writes observations before any `context_cycle` event, `phase` will be NULL — this is documented as acceptable but downstream Group 6 queries must treat NULL as a valid state.

## Design Recommendations

- **SR-01, SR-07**: The architect's first task is to verify `EmbedServiceHandle` accessibility in the UDS listener, then choose and ADR the race resolution option. This is the single highest-risk decision in the feature.
- **SR-02**: Ship a `decode_goal_embedding(blob: &[u8]) -> Result<Vec<f32>>` helper alongside the write path in the same PR. Group 6 will need it; establishing it now prevents format drift.
- **SR-04**: Ensure the v21 migration wraps both ALTER statements in an explicit transaction and checks both columns via `pragma_table_info` before either runs. Validate against entry #378 lesson: test migration against a real v20 database through the full `Store::open()` path, not just a fresh schema.
