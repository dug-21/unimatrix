# Agent Report: crt-043-agent-1-architect

## Files Produced

- `/workspaces/unimatrix/product/features/crt-043/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-043/architecture/ADR-001-bincode-embedding-blob.md`
- `/workspaces/unimatrix/product/features/crt-043/architecture/ADR-002-insert-update-race-resolution.md`
- `/workspaces/unimatrix/product/features/crt-043/architecture/ADR-003-v20-migration-atomicity.md`

## ADR Unimatrix Entry IDs

| ADR | Entry ID |
|-----|----------|
| ADR-001: SQLite embedding blob serialization — bincode Vec<f32> | #4067 |
| ADR-002: goal_embedding INSERT/UPDATE race — Option 1 | #4068 |
| ADR-003: v19→v20 migration atomicity | #4069 |

## INSERT/UPDATE Race Resolution

**Option 1 selected: embed task spawned from within `handle_cycle_event`, after the INSERT spawn.**

Options 2 and 3 are architecturally unavailable. The MCP `context_cycle` handler does not call
`handle_cycle_event` or dispatch into the UDS listener. The hook fires a UDS `RecordEvent`
independently. There is no point in the MCP handler where the INSERT is triggered and an
embedding task can be co-located with it.

Option 1 requires extending `handle_cycle_event` with a fifth parameter:
`embed_service: &Arc<EmbedServiceHandle>`. This handle is already in scope at all three call
sites in `dispatch_request` — only the pass-through needs adding.

The residual race (UPDATE before INSERT under multi-threaded tokio) degrades to NULL
`goal_embedding` — identical to the embed-service-unavailable path. No corruption.

## Key Design Decisions Summary

1. **Bincode for embedding blobs** — `bincode::serde::encode_to_vec(vec, config::standard())`.
   First SQLite embedding blob in codebase. `encode_goal_embedding` / `decode_goal_embedding`
   helpers ship in the same PR. Group 6 cites this pattern for goal_cluster embeddings.

2. **Race resolution: Option 1** — embed spawn fires inside `handle_cycle_event` after INSERT
   spawn. `handle_cycle_event` gains `embed_service` parameter; three call sites in
   `dispatch_request` updated. All error paths degrade to NULL (warn, never block).

3. **Migration atomicity** — both ADD COLUMN statements run inside the existing outer
   transaction from `migrate_if_needed`. No additional transaction needed. Both
   `pragma_table_info` pre-checks execute before either ALTER.

4. **Phase capture** — `ObservationRow` gains `phase: Option<String>`. Captured at all four
   write sites using the same pre-`spawn_blocking` session registry read as `topic_signal`.
   No `extract_observation_fields` changes needed — phase is appended at the call site.

## Open Questions

None. All critical decisions resolved. The `(topic_signal, phase)` composite index decision
is delegated to the delivery agent as stated in SCOPE.md.
