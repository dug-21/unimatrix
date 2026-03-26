# col-028: Component 3 — D-01 Guard in record_briefing_usage

**File**: `crates/unimatrix-server/src/services/usage.rs`

## Purpose

Prevent briefing calls from consuming a `UsageDedup` access slot when
`access_weight == 0`. Without this guard, a briefing event burns
`UsageDedup.access_counted[(agent_id, entry_id)]`, causing a subsequent
`context_get` on the same entry to produce zero `access_count` increment — silencing
the highest-signal read event in the pipeline.

The guard also enforces EC-04: `access_count` must not be incremented for
`context_briefing` (which is an "offer" event, not a "retrieval" event).

## Current State of record_briefing_usage

```
fn record_briefing_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
    let agent_id = ctx.agent_id.clone().unwrap_or_default();

    // Dedup access count only
    let access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids);

    if access_ids.is_empty() {
        return;
    }
    // ... spawn async task with record_usage_with_confidence ...
}
```

The `filter_access` call at line 2 is the dedup slot consumer. If `access_weight == 0`
but `filter_access` is called, the slot is consumed and the entry is marked as seen for
this agent — blocking any future `access_count` increment for the same entry in this
session.

## Change: Add D-01 Early-Return Guard

Insert the guard as the **very first statement** in the function body, before
`let agent_id = ...` and before `filter_access`. Exact text (load-bearing, AC-07):

```rust
fn record_briefing_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
    // D-01 guard (col-028): weight-0 is an offer-only event.
    // Must appear before filter_access to avoid burning the dedup slot.
    // EC-04 contract enforcement: access_count is NOT incremented for briefing.
    if ctx.access_weight == 0 {
        return;
    }

    let agent_id = ctx.agent_id.clone().unwrap_or_default();
    // ... rest of existing body unchanged ...
}
```

## Why Here, Not at the AccessSource Dispatch Level (ADR-003)

`record_access` routes `AccessSource::Briefing` to `record_briefing_usage`. An
architecturally cleaner guard location would be in `record_access` itself, before the
`match source` dispatch. However:

1. Placing the guard in `record_access` would affect ALL `AccessSource` variants —
   including future variants that might legitimately use `access_weight: 0` for different
   semantics.
2. Moving the guard to the dispatch level is a structural refactor that requires a
   separate ADR (ADR-003 SR-07 acknowledgment).
3. For this feature, the guard in `record_briefing_usage` is sufficient because all
   briefing flows go through this method.

This placement is documented as a structural limitation (R-16 future bypass risk).

## UsageContext.current_phase Doc Comment Update (ADR-006)

The `current_phase` field in `UsageContext` has a doc comment that currently ends with:
```
/// `None` for all non-store operations (search, lookup, get, correct, deprecate, etc.)
/// and for store calls with no active phase.
```

This becomes inaccurate once Component 2 (tools-read-side.md) populates `current_phase`
for read-side tools. Update the doc comment to reflect the new truth (required deliverable
per ADR-006, AC-24 gate item for the struct field in session.rs; doc comment here for
`UsageContext.current_phase`).

Updated doc comment for `current_phase` in `UsageContext`:

```rust
/// Workflow phase active at the moment the MCP tool was called.
///
/// Snapshotted from `SessionState.current_phase` at call time — never re-read from
/// live state during drain or spawn.
/// - Populated for: `context_search`, `context_lookup`, `context_get`,
///   `context_briefing`, `context_store`.
/// - `None` for: mutation tools (correct, deprecate, quarantine), tools with no
///   session, and any call in a session where no `context_cycle(start)` has been
///   emitted.
```

## Logic Flow After Change

```
record_briefing_usage(entry_ids, ctx):
    IF ctx.access_weight == 0:           // D-01 guard
        return                           // no dedup slot consumed, no DB write

    agent_id = ctx.agent_id or ""
    access_ids = usage_dedup.filter_access(agent_id, entry_ids)

    IF access_ids.is_empty():
        return                           // all entries already seen this session

    // spawn async: record_usage_with_confidence for access_ids
    // (existing body, unchanged)
```

## Error Handling

No new error conditions. The guard is a pure early-return on a `u32` comparison.
All existing error handling in the function body is unchanged.

## Key Test Scenarios

**AC-07 (Critical)** — Briefing-then-get sequence, dedup slot NOT consumed:
  1. Insert entry X with `access_count = 0`.
  2. Call `record_briefing_usage([X], UsageContext { access_weight: 0, ... })`.
  3. Assert `UsageDedup.access_counted` does NOT contain `(agent_id, X)`.
  4. Call `record_mcp_usage([X], UsageContext { access_weight: 2, ... })`.
  5. Assert `access_count` for X = 2 (not 0).

**AC-07 negative (guard load-bearing verification)**:
  - Simulate guard absent: call `record_briefing_usage` with `access_weight: 0` but
    comment out the guard.
  - Assert `UsageDedup.access_counted` DOES contain `(agent_id, X)`.
  - Assert subsequent context_get produces 0 increment.
  - This confirms the guard is not redundant.

**AC-06** — No access_count increment for any entry after briefing at weight=0:
  1. Insert entries X, Y.
  2. Call `record_briefing_usage([X, Y], UsageContext { access_weight: 0, ... })`.
  3. Assert `access_count` for both X and Y remains 0.
  4. Assert `UsageDedup.access_counted` is empty.

**Double briefing idempotency** — Call `record_briefing_usage` with weight=0 twice:
  - Dedup slot still absent both times (guard fires before filter_access).
  - Subsequent context_get still increments by 2.

**EC-03** — Briefing with empty entry list (`[]`) at weight=0:
  - `record_access` has an existing early-return for `entry_ids.is_empty()` BEFORE
    dispatching to `record_briefing_usage`. The D-01 guard in `record_briefing_usage`
    would never be reached in this case, but the overall behavior is correct: no
    dedup slot consumed, no panic.

## Interaction With Existing access_ids.is_empty() Guard

After the D-01 guard, the existing `if access_ids.is_empty() { return; }` guard remains.
These two guards serve different purposes:
- D-01: fires before `filter_access` — prevents dedup slot from being consumed at all.
- `access_ids.is_empty()`: fires after `filter_access` — prevents a no-op async spawn.
Both must remain present in the function.

## Out of Scope

- Moving the guard to the `AccessSource` dispatch level in `record_access` (SR-07,
  requires separate ADR).
- Any change to `record_mcp_usage` or `record_hook_injection`.
- Any change to the confidence computation or access_count increment logic.
