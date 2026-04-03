# Agent Report: crt-043-agent-1-pseudocode

## Task

Produce per-component pseudocode files for crt-043: Behavioral Signal Infrastructure.
Three components: schema-migration, goal-embedding, phase-capture.

## Output

| File | Status |
|------|--------|
| `product/features/crt-043/pseudocode/OVERVIEW.md` | Created |
| `product/features/crt-043/pseudocode/schema-migration.md` | Created |
| `product/features/crt-043/pseudocode/goal-embedding.md` | Created |
| `product/features/crt-043/pseudocode/phase-capture.md` | Created |

## Components Covered

1. **schema-migration** — v20→v21 migration block, `CURRENT_SCHEMA_VERSION` bump 20→21,
   `encode_goal_embedding` / `decode_goal_embedding` helpers in new `embedding.rs`,
   `update_cycle_start_goal_embedding` store method, `phase` bind in observation INSERT
   functions, composite index `idx_observations_topic_phase`.

2. **goal-embedding** — `handle_cycle_event` signature extension with `embed_service`,
   Step 6 fire-and-forget embedding spawn after INSERT spawn for `CycleLifecycle::Start`
   with non-empty (non-whitespace) goal, three `dispatch_request` call site updates.

3. **phase-capture** — `phase: Option<String>` added to `ObservationRow`, pre-spawn capture
   at all four observation write sites (RecordEvent, rework-candidate, RecordEvents batch,
   ContextSearch), `phase` bound at position ?9 in both INSERT functions.

## Resolved Decisions Documented in OVERVIEW.md

- **WARN-2**: `decode_goal_embedding` stays `pub(crate)` within `unimatrix-store`; Group 6
  consumes via a store query method. `encode_goal_embedding` must be `pub` (called from
  `unimatrix-server`). Both helpers re-exported from `lib.rs` as `pub`.

- **FR-C-07**: Composite index `idx_observations_topic_phase ON observations (topic_signal, phase)`
  added in v21 migration. Justified: Group 6 phase-stratification queries will filter by both
  columns; full-table scan at scale is unacceptable. Index cost at migration time is negligible.

- **Whitespace-only goal**: Trim before check. Whitespace-only goal treated as absent (no spawn,
  no warn). The verbatim goal value is still passed to `insert_cycle_event` (col-025 verbatim
  UDS storage preserved).

## Open Questions / Gaps Found

1. **`encode_goal_embedding` visibility**: ADR-001 specifies `pub(crate)` but the write
   path in `unimatrix-server` needs cross-crate access. The pseudocode resolves this by
   promoting both helpers to `pub` and re-exporting from `lib.rs`. The implementation agent
   must verify no other ADR-001 constraint is violated by this promotion.

2. **`create_tables_if_needed` sync**: The `cycle_events` and `observations` CREATE TABLE
   statements in `db.rs` must include the new columns for fresh databases. The pseudocode
   documents this requirement; the implementation agent must locate and update these DDL
   statements (they are in `create_tables_if_needed`, around line 440 of db.rs).

3. **`set_current_phase("")` edge case**: The pseudocode documents the risk of empty-string
   phase being stored as `''` instead of NULL. The implementation agent must inspect
   `session.rs` to confirm whether `set_current_phase` normalizes or rejects empty strings.
   If it does not, add a note to the PR description.

4. **`session_registry.get_state` API**: The pseudocode assumes `get_state(session_id)` returns
   an `Option<SessionState>` or similar that exposes `.current_phase`. The implementation agent
   must verify the exact return type and field access path in `session.rs` before writing code.

5. **NFR-03 re-evaluation**: `update_cycle_start_goal_embedding` acquires a connection from
   the write pool independently from `insert_cycle_event`. These are two separate async tasks
   making independent pool acquisitions. The pseudocode notes this is consistent with other
   fire-and-forget writes in the same file (feature_cycle persist, eager attribution). The
   implementation agent must confirm this is acceptable per NFR-03 or flag if a different
   approach is needed.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — server unavailable in agent context; proceeded
  using architecture documents, ADR files, SPECIFICATION.md, RISK-TEST-STRATEGY.md, and direct
  code inspection of migration.rs and listener.rs.
- Deviations from established patterns: none. All patterns followed as specified:
  - `pragma_table_info` pre-check (entry #1264) applied to both ALTER TABLE statements
  - `enrich_topic_signal` pre-capture timing contract (entry #3374) replicated for phase
  - fire-and-forget spawn pattern with warn! on all error paths (entry #735)
  - bincode `standard()` config for both encode and decode (ADR-001)
  - Outer transaction atomicity (ADR-003) — no new BEGIN/COMMIT needed
