# Component: `context_correct` handler — step 8b′ insertion + `edges_carried` ack

**Crate / location:** `unimatrix-server/src/mcp/tools.rs::context_correct` (~:1015). Two edits:
1. Insert step **8b′** between the existing 8b (`validate_and_write_edges`, ~:1126-1142) and
   8c (`run_redirect_loop`, ~:1144-1153).
2. Thread `edges_carried` into the response ack at step 10 (~:1161-1185), mirroring the existing
   `format_redirect_summary` append.

No other handler logic changes. Phase A pre-correction validation (~:1054), `store_ops.correct()`
(~:1108), confidence recompute (~:1155), and the redirect loop (~:1148) are all UNCHANGED.

## Purpose

Wire the new carry-forward loop into the live correction pipeline at the load-bearing position
(8b′, ADR-001) and surface the carried count in the response (AC-11) without disturbing the
existing post-correction steps.

## Edit 1 — insert step 8b′

Current code (~:1142) ends step 8b with the closing brace of the `if !correct_edges_slice.is_empty()`
block, then step 8c (`let redirect_summary = run_redirect_loop(...)`) begins at ~:1144. Insert
between them:

```
    // ...existing 8b block ends here (validate_and_write_edges, ~:1142)...

    // 8b′. Carry-forward A's eligible outgoing edges onto the new entry (vnc-035).
    //
    // Runs AFTER 8b so re-passed params.edges already on B become UNIQUE conflicts here
    // (not double-counted), and BEFORE 8c so outgoing-carry and incoming-redirect act on
    // disjoint row sets (A-outgoing vs A-incoming) and never both touch one Contradicts pair
    // (ADR-001, ADR-005). The correction has already committed (step 8); carry never rolls it
    // back — run_carry_forward_loop returns CarrySummary by value and cannot error (ADR-002).
    let carry_summary = run_carry_forward_loop(
        &self.entry_store,
        original_id,
        correct_result.corrected_entry.id,
    )
    .await;

    // 8c. Auto-redirect incoming edges (vnc-017) — UNCHANGED below this point.
    let redirect_summary = run_redirect_loop(...)   // existing
```

Notes:
- Use `&self.entry_store` (same store handle 8b/8c use).
- `original_id` is already in scope (~:1037). `correct_result.corrected_entry.id` is B (same
  value 8b and 8c pass).
- If the developer chose the `created_at: u64` parameter shape for the loop (run_carry_forward_loop.md
  Note A (b)), compute `now` once before 8b (or hoist the existing 8b `created_at` at ~:1128 out of
  the `if` block) and pass the same `now` to both 8b and 8b′ so carried + passed edges share one
  correction timestamp. The 8b `created_at` is currently computed INSIDE the `if !empty` block
  (~:1128); hoisting it is a small, safe refactor if (b) is chosen.

## Edit 2 — thread `edges_carried` into the ack (step 10, ~:1161)

The ack is currently built by `format_correct_success(...)` (~:1162) with an optional
`format_redirect_summary` line appended to the first content item's text (~:1167-1184). Append the
`edges_carried` line the same way — **count only, omitted when zero** (AC-11 / ADR-003).

```
    // 10. Format response.
    let mut result = format_correct_success(
        &correct_result.deprecated_original,
        &correct_result.corrected_entry,
        ctx.format,
    );

    // 10a. Append edges_carried ack (vnc-035, AC-11) — count only, OMITTED when zero.
    //      carried counts actual inserts (write_graph_edge `true`); a re-passed edge that
    //      conflicted in 8b′ is NOT counted (SR-02). One logical Contradicts = counted once.
    if carry_summary.carried > 0 {
        append_to_first_text(&mut result, format_edges_carried(carry_summary.carried));
    }

    // 10b. Append redirect summary (vnc-017) — EXISTING, unchanged.
    if let Some(rs) = redirect_summary { ...existing append... }

    Ok(result)
```

Where:
- `append_to_first_text` is the existing inline pattern at ~:1177-1182 (push `'\n'` + text onto
  `result.content.first_mut()`'s `RawContent::Text`). Either factor it into a tiny local helper
  reused by both 10a and 10b, or inline it twice — developer's call (DRY vs minimal diff).
- `format_edges_carried(n)` is a new formatter (see below). It returns the bare line; the
  zero-omission is enforced by the `if carry_summary.carried > 0` guard at the call site, mirroring
  `format_redirect_summary`'s `found == 0 → None` contract.

### Ordering of the two appended lines

Append `edges_carried` (10a) BEFORE the redirect summary (10b) — carry is step 8b′ (before 8c), so
ordering the ack lines carry-then-redirect reads in pipeline order. This is cosmetic; the ACs only
require the `edges_carried` field/line to be present (>0) or absent (==0).

## New formatter — `format_edges_carried`

**Location:** `unimatrix-server/src/mcp/response/entries.rs` (alongside `format_redirect_summary`
at :265 and `format_correct_success` at :301), so both correction ack formatters live together.

```rust
/// Format the vnc-035 carry-forward ack line. COUNT ONLY — no edge identities/content (AC-11).
/// The caller guards on `carried > 0`; this returns the line unconditionally for `carried >= 1`.
pub fn format_edges_carried(carried: usize) -> String {
    // e.g. "Carried 3 outgoing edges forward"
    format!("Carried {carried} outgoing edges forward")
}
```

For `ResponseFormat::Json` the ack is currently text-appended like the redirect summary (the
existing code appends to the first content item's text regardless of format). Keep parity: append
the same line. If a future change wants a structured JSON `edges_carried` field, that is out of
scope here — match the existing redirect-summary rendering exactly (SR-05: the ack is the awareness
channel; its presence, not its shape, is the contract).

## Data flow

- **In:** `carry_summary: CarrySummary` from 8b′.
- **Read:** `carry_summary.carried` only (the ack value). `found`/`failed` are logged inside the
  loop, not surfaced in the ack (count-only, no failure detail in the response — ADR-003).
- **Out:** `CallToolResult` with the `edges_carried` line appended when `carried > 0`.

## Error handling

- The handler does NOT add any new error path. `run_carry_forward_loop` returns `CarrySummary` by
  value (no `Err`), so 8b′ cannot fail the handler or roll back the correction (NFR-01). This is
  structurally stronger than 8b (`validate_and_write_edges` returns `Result` and the handler
  early-returns on its `Err` at ~:1140 — but 8b runs its validation BEFORE writes; carry has no
  such error surface by design).
- No change to the existing 8b early-return, 8c, or confidence steps.

## Key test scenarios (hints — not the test plan)

- **Ack present, N>0 (AC-11 #a):** correction carrying N edges → response contains
  `"Carried N outgoing edges forward"`; the integer equals actual inserts (NFR-03).
- **Ack omitted, zero (AC-11 #b / R-02 #3):** correction with no eligible outgoing edges → the
  `edges_carried` line is ABSENT (not "Carried 0 ...").
- **No edge content in ack (AC-11 #c / R-02 #4):** the line carries the integer only — no target
  ids, no relation types.
- **Pipeline order (R-04 #1):** assert the handler runs 8 → 8b → 8b′ → 8c → 9 → 10 (carry between
  `params.edges` write and incoming redirect). A re-passed edge written by 8b is a UNIQUE conflict
  in 8b′ → not counted (validates 8b BEFORE 8b′, R-04 #2).
- **AC-07 integration (MANDATORY):** the mandatory carry-failure test asserts the handler returns
  success and B Active / A Deprecated while one carry write fails mid-loop — this exercises the
  handler's no-rollback guarantee through the real 8b′ call.
- **Back-compat (NFR-07):** a caller re-passing A's full edge list → 8b writes them, 8b′ hits
  UNIQUE conflicts → no double-write, no error, `edges_carried` reflects only non-re-passed carries.
