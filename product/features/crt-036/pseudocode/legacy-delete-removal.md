# crt-036: Legacy DELETE Removal — Pseudocode

**Files:**
- `crates/unimatrix-server/src/services/status.rs` (lines ~1372–1384)
- `crates/unimatrix-server/src/mcp/tools.rs` (lines ~1630–1642)

---

## Purpose

Two independent sites currently perform a 60-day wall-clock `DELETE FROM observations`.
Both must be removed unconditionally (not guarded, not conditionalized). Running both
the old time-based policy and the new cycle-based GC concurrently is explicitly not
supported (architecture constraint 7).

These removals happen in the same delivery pass as the GC block insertion in
`run-maintenance-gc-block.md`. They are not independent patches.

---

## Site 1: status.rs lines ~1372–1384

This block is the old step 4 `run_maintenance()`. It is replaced entirely by the
cycle-based GC block documented in `run-maintenance-gc-block.md`.

Identify by the comment `// 4. Observation retention cleanup (col-012: SQL DELETE)`
or the literal string `"DELETE FROM observations WHERE ts_millis < ?1"`.

```
// REMOVE THIS BLOCK (status.rs ~1372-1384):

        // 4. Observation retention cleanup (col-012: SQL DELETE)
        {
            let now_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
            let cutoff = now_millis - sixty_days_millis;
            let _ = sqlx::query("DELETE FROM observations WHERE ts_millis < ?1")
                .bind(cutoff)
                .execute(self.store.write_pool_server())
                .await;
        }
```

The step-4 comment becomes the new cycle-based GC block comment:
`// 4. Cycle-based activity GC (crt-036: replaces 60-day DELETE)`

---

## Site 2: tools.rs lines ~1630–1642

This block appears inside the MCP tool handler for `context_cycle_review` (or a
nearby handler). Identify by the comment `// 9. Cleanup expired observations (FR-07:
60-day retention via SQL DELETE)` or the literal string
`"DELETE FROM observations WHERE ts_millis < ?1"`.

```
// REMOVE THIS BLOCK (tools.rs ~1630-1642):

        // 9. Cleanup expired observations (FR-07: 60-day retention via SQL DELETE)
        {
            let now_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
            let cutoff = now_millis - sixty_days_millis;
            let _ = sqlx::query("DELETE FROM observations WHERE ts_millis < ?1")
                .bind(cutoff)
                .execute(store.write_pool_server())
                .await;
        }
```

After removal, the comment block and the surrounding code continue normally.
Step numbering in the tool handler may need renumbering (step 10a and beyond become
step 9a, 9b etc.) — or the comment for step 9 is simply removed and existing downstream
step numbers are left as-is. Prefer minimal churn: just remove the block and its comment.

---

## Verification Requirements

After both removals, two assertions must pass (AC-01a, AC-01b, R-01):

1. `grep -r "DELETE FROM observations WHERE ts_millis" crates/unimatrix-server/src/services/status.rs`
   must produce zero matches.

2. `grep -r "DELETE FROM observations WHERE ts_millis" crates/unimatrix-server/src/mcp/tools.rs`
   must produce zero matches.

These assertions are independently verified — a single combined grep across both files
is insufficient because it passes if one site is removed but the other remains.

The integration test suite should include two grep-style assertions (cargo test with
string pattern search, or a `compile_assertions` macro, or a dedicated `#[test]` that
reads the file as bytes and asserts the pattern is absent). Whichever mechanism is used,
both files must be checked independently.

---

## No Replacement Needed for Site 2

Site 2 in `tools.rs` is purely removed. It is not replaced. The cycle-based GC that
now handles observation cleanup runs in the background maintenance tick via `status.rs`.
The `tools.rs` handler does not need to call any GC function.

---

## Error Handling

No error handling needed — this is a deletion-only change. The removed blocks used
`let _ = ...` (result discarded) so there is no error propagation to maintain.

---

## Key Test Scenarios

- `grep` assertion: `"DELETE FROM observations WHERE ts_millis"` absent in `status.rs` (AC-01a).
- `grep` assertion: `"DELETE FROM observations WHERE ts_millis"` absent in `tools.rs` (AC-01b).
- After removal: `context_cycle_review` tool call still succeeds end-to-end with no
  observations error (regression test — the removed block was fire-and-forget, its absence
  must not cause a panic or compile error in the surrounding code).
- `cargo build` passes cleanly after both removals — no dead variable warnings from
  `now_millis` or `cutoff` bindings being removed.
