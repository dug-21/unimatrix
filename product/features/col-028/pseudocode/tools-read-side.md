# col-028: Component 2 — Phase Helper + Four Read-Side Call Sites + query_log Write Site

**File**: `crates/unimatrix-server/src/mcp/tools.rs`

## Purpose

1. Add free function `current_phase_for_session` at module scope (ADR-001) so all four
   read-side handlers share a single, testable phase extraction path.
2. Update `context_search`, `context_lookup`, `context_get`, `context_briefing` to:
   - Snapshot phase as the first statement before any `.await` (C-01).
   - Pass phase into `UsageContext.current_phase`.
   - Apply corrected access weights: `context_get` 1→2, `context_briefing` 1→0.
   - Call `record_confirmed_entry` at the appropriate sites.
3. Pass `phase` to `QueryLogRecord::new` in `context_search` using the same snapshot
   variable (C-04, FR-18).
4. Update `UsageContext.current_phase` doc comment (ADR-006).

## Pre-condition Check (NFR-05)

Before making any changes, verify `mcp/tools.rs` line count is below 500 (workspace
rule: max 500 lines per file). If at or near the limit, split first per the existing
module split pattern. The specification calls this out explicitly as NFR-05.

## New Free Function: current_phase_for_session

**Location**: Module scope in `mcp/tools.rs`, NOT inside the `impl UnimatrixServer` block.

**Exact signature** (FR-02, C-10 — do not use the `?` pseudocode from FR-02 body; use
the `and_then` chaining form from the Exact Signatures section of SPECIFICATION.md):

```rust
pub(crate) fn current_phase_for_session(
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> Option<String> {
    session_id.and_then(|sid| registry.get_state(sid))
              .and_then(|s| s.current_phase.clone())
}
```

Properties:
- `pub(crate)`: visible for unit tests without handler construction (ADR-001).
- No lock is held after the function returns — `get_state` returns a `Clone`.
- Returns `None` when: `session_id` is `None`; session not in registry; session has no
  active phase.
- Called at most once per handler invocation (NFR-01, C-04).

## Updated Doc Comment: UsageContext.current_phase (ADR-006)

In `services/usage.rs`, the `current_phase` field doc comment currently reads:
```
/// `None` for all non-store operations (search, lookup, get, correct, deprecate, etc.)
/// and for store calls with no active phase.
```

The delivery agent must update this comment in `services/usage.rs` to reflect that
read-side tools now populate the field. Suggested wording (exact text may vary, but the
meaning must be accurate):

```
/// Workflow phase active at the moment the MCP tool was called.
///
/// Snapshotted from `SessionState.current_phase` at call time — never re-read from
/// live state during drain or spawn.
/// - Populated for: `context_search`, `context_lookup`, `context_get`, `context_briefing`,
///   `context_store`.
/// - `None` for: mutation tools (correct, deprecate, quarantine), tools with no session,
///   and any call in a session where no `context_cycle(start)` has been emitted.
```

## Handler Changes

### context_search

**Current state**: `current_phase: None` in `UsageContext`; `QueryLogRecord::new` called
with six arguments (no phase).

**Change summary**:
1. Phase snapshot — first statement before any `.await`.
2. Use same snapshot for both `UsageContext.current_phase` and `QueryLogRecord::new`.
3. Pass `phase.clone()` to `QueryLogRecord::new` as the final argument.
4. `access_weight` remains 1 (unchanged).
5. No `confirmed_entries` recording (search is not explicit retrieval).

```
FUNCTION context_search(params):
    // [C-01] Phase snapshot FIRST — before build_context await
    let phase = current_phase_for_session(
        &self.session_registry,
        params.session_id.as_deref(),
    )

    // 1. Identity + format + audit context (unchanged)
    let ctx = self.build_context(...).await?
    self.require_cap(...).await?

    // 2-4. Validation, k parsing, ServiceSearchParams, search (all unchanged)
    let search_results = self.services.search.search(...).await?

    // 5. Format response (unchanged)

    // 6. Usage recording — now includes current_phase
    self.services.usage.record_access(
        &target_ids,
        AccessSource::McpTool,
        UsageContext {
            ...
            access_weight: 1,                    // unchanged
            current_phase: phase.clone(),         // WAS: None
        },
    )

    // 7. Query log — phase shared from same snapshot (C-04, SR-06 mitigation)
    let record = QueryLogRecord::new(
        session_id_for_log,
        params.query.clone(),
        &entry_ids,
        &scores,
        "flexible",
        "mcp",
        phase,                                    // NEW 7th argument (C-05: as ?9)
    )
    spawn_blocking(move || { store_clone.insert_query_log(&record) })

    Ok(result)
```

Critical ordering note: the `let phase = ...` line must physically appear before
`self.build_context(...).await?` in the source file. `build_context` contains the first
`.await` in the handler. The phase snapshot must precede it.

### context_lookup

**Current state**: `current_phase: None` in `UsageContext`; no `record_confirmed_entry`.
`access_weight: 2` (unchanged).

**Change summary**:
1. Phase snapshot — first statement.
2. `UsageContext.current_phase` receives phase.
3. `record_confirmed_entry` called when `target_ids.len() == 1` (ADR-004).
4. `access_weight` remains 2.

```
FUNCTION context_lookup(params):
    // [C-01] Phase snapshot FIRST — before build_context await
    let phase = current_phase_for_session(
        &self.session_registry,
        params.session_id.as_deref(),
    )

    // 1. Identity + capability check (unchanged)
    let ctx = self.build_context(...).await?
    self.require_cap(...).await?

    // 2-3. Validation, limit parsing (unchanged)

    // 4. Branch: ID-based vs filter-based (unchanged; produces target_ids)
    let (result, target_ids) = ...

    // 5. Audit (unchanged)

    // 6. Usage recording — now includes current_phase
    self.services.usage.record_access(
        &target_ids,
        AccessSource::McpTool,
        UsageContext {
            ...
            access_weight: 2,                    // unchanged (R-14: must NOT change)
            current_phase: phase,                 // WAS: None
        },
    )

    // NEW: confirmed_entries recording (ADR-004 — request-side cardinality)
    // Check params.id (or equivalently target_ids from params), NOT target_ids.len()
    // from the response. See note below.
    if params.id.is_some():
        // Single-ID lookup — the params.id path always produces exactly one result
        // or returns an error. If we reach here, target_ids has exactly one entry.
        IF let Some(sid) = ctx.audit_ctx.session_id.as_deref():
            IF let Some(&entry_id) = target_ids.first():
                self.session_registry.record_confirmed_entry(sid, entry_id)

    Ok(result)
```

**Cardinality note (ADR-004)**: The trigger is request-side, not response-side.
`params.id.is_some()` exactly identifies single-ID lookup requests. This is equivalent
to `target_ids.len() == 1` for the ID path (which always yields one result or errors
before reaching usage recording), but clearer in intent. The filter-based branch
(no `params.id`) always has `len() != 1` unless exactly one entry happened to match —
that is response-side cardinality and must NOT trigger `record_confirmed_entry`.

Alternative equivalent check: `target_ids.len() == 1 && params.id.is_some()`. Either
form satisfies ADR-004.

### context_get

**Current state**: `current_phase: None`, `access_weight: 1`, no `record_confirmed_entry`.

**Change summary**:
1. Phase snapshot — first statement.
2. `access_weight: 1 → 2` (FR-03, R-07).
3. `UsageContext.current_phase` receives phase.
4. `record_confirmed_entry` called unconditionally after successful retrieval (FR-08).

```
FUNCTION context_get(params):
    // [C-01] Phase snapshot FIRST — before build_context await
    let phase = current_phase_for_session(
        &self.session_registry,
        params.session_id.as_deref(),
    )

    // 1. Identity + capability check (unchanged)
    let ctx = self.build_context(...).await?
    self.require_cap(...).await?

    // 2. Validation (unchanged)

    // 3. Get entry (unchanged — returns error on not-found)
    let id = validated_id(params.id)?
    let entry = self.entry_store.get(id).await?
    // Execution reaches here only on successful retrieval (EC-05 contract)

    // 4. Format response (unchanged)

    // 5. Audit (unchanged)

    // 6. Usage recording — weight corrected to 2, phase added
    self.services.usage.record_access(
        &[id],
        AccessSource::McpTool,
        UsageContext {
            ...
            helpful: params.helpful.or(Some(true)),   // unchanged
            access_weight: 2,                          // WAS: 1 — FR-03
            current_phase: phase,                      // WAS: None
        },
    )

    // NEW: confirmed_entries recording (FR-08 — always on successful retrieval)
    IF let Some(sid) = ctx.audit_ctx.session_id.as_deref():
        self.session_registry.record_confirmed_entry(sid, id)

    Ok(result)
```

EC-05 contract: `record_confirmed_entry` is only reached if `entry_store.get(id)` succeeded
(returned `Ok`). The `?` after the get call propagates errors before reaching the usage
recording and confirmed_entries sections.

### context_briefing

**Current state**: `current_phase: None`, `access_weight: 1`, `AccessSource::Briefing`.

**Change summary**:
1. Phase snapshot — first statement within the `#[cfg(feature = "mcp-briefing")]` block.
2. `access_weight: 1 → 0` (FR-04, R-08).
3. `UsageContext.current_phase` receives phase.
4. No `record_confirmed_entry` (briefing is not explicit retrieval per ADR-005 contract).

```
FUNCTION context_briefing(params):
    // Feature-flag guard (existing — unchanged)
    #[cfg(not(feature = "mcp-briefing"))]:
        return error result

    #[cfg(feature = "mcp-briefing")]:
        // [C-01] Phase snapshot FIRST — before build_context await
        // Note: inside the cfg block, this is still the first statement before any .await
        let phase = current_phase_for_session(
            &self.session_registry,
            params.session_id.as_deref(),
        )

        // 1. Identity + capability check (unchanged)
        let ctx = self.build_context(...).await?
        self.require_cap(...).await?

        // 2-8. Validation, session_state resolution, query derivation,
        //      IndexBriefingParams, BriefingService.index (all unchanged)
        //
        //      Note: step 4 already calls self.session_registry.get_state(sid)
        //      for query derivation. That is a SEPARATE purpose (query synthesis
        //      from topic_signals). It is NOT the phase snapshot. The phase
        //      snapshot uses current_phase_for_session, not session_state.current_phase
        //      directly, to satisfy the single-call discipline of C-04 per handler.
        //      (context_briefing is not context_search — it does not share the
        //      session_state variable between UsageContext and another consumer.
        //      The existing session_state lookup at step 4 serves query derivation only.)

        // 9. Collect entry IDs (unchanged)
        // 10. Format response (unchanged)
        // 11. Audit (unchanged)

        // 12. Usage recording — weight corrected to 0, phase added
        self.services.usage.record_access(
            &entry_ids,
            AccessSource::Briefing,
            UsageContext {
                ...
                access_weight: 0,                // WAS: 1 — FR-04
                current_phase: phase,             // WAS: None
            },
        )
        // D-01 guard fires in record_briefing_usage (Component 3) — not here.

        Ok(table_result)
```

**context_briefing and C-04 clarification**: The context_briefing handler already calls
`self.session_registry.get_state(sid)` at step 4 for query derivation (not for phase).
This is a different purpose and is acceptable. C-04 ("single get_state call per handler")
applies specifically to cases where the same phase value is read twice — at context_search
where phase serves both UsageContext and QueryLogRecord. In context_briefing there is
only one phase consumer (UsageContext), so one call from `current_phase_for_session` is
sufficient. The step-4 `session_state` variable is used only for query derivation and
must not be reused to extract phase (separation of concerns, and the step-4 call happens
after `build_context().await` — too late for C-01).

## compile-fix sites (no semantic change)

### uds/listener.rs line 1324

`QueryLogRecord::new(...)` currently has six arguments. After Component 4 adds `phase`
as the seventh, pass `None`:

```
QueryLogRecord::new(
    session_id,
    query_text,
    &entry_ids,
    &scores,
    retrieval_mode,
    "uds",
    None,             // NEW — col-028 compile fix; no phase semantics for UDS (C-08)
)
```

### mcp/knowledge_reuse.rs — make_query_log struct literal

Add `phase: None` to the `QueryLogRecord` struct literal (FR-21):

```
QueryLogRecord {
    // ... existing fields ...
    phase: None,   // col-028
}
```

### eval/scenarios/tests.rs — insert_query_log_row helper

The raw SQL INSERT in `insert_query_log_row` must include the `phase` column. Update the
column list and add a NULL bind. All 15+ call sites use this shared helper so no call
site changes are needed (IR-03).

Before: 8 columns, 8 values.
After:
```sql
INSERT INTO query_log
    (session_id, query_text, ts, result_count,
     result_entry_ids, similarity_scores, retrieval_mode, source, phase)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
```
Bind `?9` as `None::<String>` (or bind `sqlx::types::Null`).

### server.rs — schema version cascade (SR-02)

Lines 2059 and 2084 currently have `assert_eq!(version, 16)`. Update both to
`assert_eq!(version, 17)`.

## Error Handling

- `current_phase_for_session`: infallible. Returns `Option<String>`.
- `record_confirmed_entry`: infallible (see Component 1).
- No new error conditions introduced by any handler change in this component.
- `QueryLogRecord::new`: infallible (same as before — no new failure path).

## Key Test Scenarios

**AC-01** — `context_search` passes `current_phase: Some("delivery")` when session has
  phase "delivery"; passes `None` when no session.
  - Unit test with a `SessionRegistry` seeded with a phased session.

**AC-02** — `context_lookup` passes correct phase in `UsageContext`.
  - Same pattern as AC-01.

**AC-03** — `context_get` passes correct phase; uses `access_weight: 2`.

**AC-04** — `context_briefing` passes correct phase; uses `access_weight: 0`.

**AC-05** — `context_get` produces `access_count = 2` on first call (weight=2).
  - Unit test: insert entry, call context_get, read back access_count, assert 2.

**AC-06** — `context_briefing` produces no `access_count` increment.
  - Unit test: insert entry, call context_briefing, read back access_count, assert 0.

**AC-09** — After `context_get` for entry X, session's `confirmed_entries` contains X.

**AC-10 (single-ID)** — After `context_lookup` with a single ID, `confirmed_entries`
  contains that ID.

**AC-10 (multi-ID)** — After `context_lookup` with multiple IDs, `confirmed_entries`
  is NOT updated.

**AC-12 (code review gate)** — Phase snapshot is the first statement in each handler
  body. No `.await` appears before `current_phase_for_session(...)`.

**AC-16** — Integration test: `context_search` in a session with phase "delivery" writes
  `phase = "delivery"` to `query_log`. Requires real analytics drain and flush.

**Unit test for current_phase_for_session**:
  - Register session with `current_phase = Some("design")`.
  - Call `current_phase_for_session(&registry, Some("sid"))`.
  - Assert `Some("design")`.
  - Call with `None` session_id. Assert `None`.
  - Call with unknown session_id. Assert `None`.
  - Call with session that has `current_phase = None`. Assert `None`.

## Out of Scope

- Phase capture for `context_correct`, `context_deprecate`, `context_quarantine`.
- Any change to `context_store` (already has phase capture from crt-025).
- UDS phase semantics (uds/listener.rs is compile-fix only).
- Scoring pipeline (`w_phase_explicit` remains 0.0).
