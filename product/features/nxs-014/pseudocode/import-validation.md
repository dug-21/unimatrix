# Component 3 — import-validation

**File:** `crates/unimatrix-server/src/import/mod.rs`, `validate_hashes` (lines 396-442), called at `:212`.
**Source of truth:** ARCHITECTURE §Component Interactions (import), ADR-001 (single oracle), SPEC FR-07/FR-10.
**Traces:** FR-07, FR-10; AC-04, AC-05; R-04, R-05; C-06 (shrinks the near-ceiling file).
**Depends on:** Component 1 (`chain_verify::verify_entries`).

## Purpose

Refactor `validate_hashes` from a second, weaker integrity implementation into a **thin adapter** over the
shared store core (ADR-001 — one oracle, no drift). Load ALL entries from the in-flight import transaction
connection, call `verify_entries`, map a non-clean report to the existing `Err` so the caller ROLLBACKs
before COMMIT.

## What is REMOVED (the divergent second oracle — R-04)

Delete the current body (`:397-441`): the 5-column `SELECT id, title, content, content_hash, previous_hash`,
the `known_hashes: HashSet<String>` existence check (`:429`), and the hand-rolled `errors: Vec<String>`
content-hash loop (`:421`). These are subsumed by the core:
- Content-hash recompute (`:421`) → core step (1).
- Empty-`previous_hash` tolerance (`:429`) → core `skipped_legacy` skip (C-02, preserved).
- "previous_hash references some known hash" existence check (`:429`) → replaced by the strictly-stronger
  `supersedes`-keyed link check (R-04). This is a **documented, intentional** behavior strengthening, not a
  loosening — existing import tests are the tripwire (run them unchanged first).

## New body (thin adapter)

Signature UNCHANGED (`async fn validate_hashes(conn: &mut SqliteConnection) -> Result<(), Box<dyn Error>>`)
so the call site at `:212` and its ROLLBACK/`Err` control flow (`:211-218`) stay intact (R-05).

```
async fn validate_hashes(conn: &mut SqliteConnection) -> Result<(), Box<dyn std::error::Error>>:

    // Load ALL entries from the IN-FLIGHT transaction connection so uncommitted imported rows are visible
    // (must use `conn`, not the pool — same reason as the BEGIN IMMEDIATE note at :195-198).
    // NO status filter -> ALL statuses incl. Deprecated predecessors (R-02). ENTRY_COLUMNS = full row so
    // entry_from_row can reconstruct EntryRecord (supersedes/version/status needed by the core).
    let sql  = format!("SELECT {} FROM entries", unimatrix_store::read::ENTRY_COLUMNS)
    let rows = sqlx::query(&sql).fetch_all(&mut *conn).await?

    let entries: Vec<EntryRecord> = rows.iter()
        .map(unimatrix_store::read::entry_from_row)
        .collect::<Result<Vec<_>, _>>()?          // map_err into Box<dyn Error> via `?`

    // SINGLE ORACLE — content-hash AND chain-link run together, no caller-side half-check (ADR-001)
    let report = unimatrix_store::chain_verify::verify_entries(&entries)

    if !report.is_clean():
        return Err(report.describe().into())      // names every offending id; caller ROLLBACKs (:213) before COMMIT

    Ok(())
```

Notes:
- `entry_from_row` does NOT load tags; the core does not use tags — fine. (Tag hydration is irrelevant to
  content-hash + link verification.)
- Ordering (`ORDER BY id`) is unnecessary — the core builds a map and is order-independent — but keep it if a
  deterministic error-message order aids test assertions. Optional.
- No behavior change to the surrounding transaction: call site `:211-218` still wraps this in
  `if let Err(e) = validate_hashes(&mut conn) { ROLLBACK; return Err(e) }` (R-05 atomicity preserved).

## Data flow

```
in:  &mut SqliteConnection (in-flight BEGIN IMMEDIATE txn, uncommitted rows visible)
load: SELECT {ENTRY_COLUMNS} FROM entries (all statuses) -> Vec<EntryRecord> via entry_from_row
core: verify_entries(&entries) -> ChainReport
out: Ok(()) if clean; Err(describe) if not -> caller ROLLBACKs before COMMIT (:213)
```

## Error handling

- Row load / `entry_from_row` failures propagate via `?` into `Box<dyn Error>` (unchanged behavior).
- Non-clean report → `Err(report.describe().into())`: the message names every offending `entry_id` + kind
  (AC-04, NFR-06). Caller maps to ROLLBACK + `Err` at `:213` — no COMMIT of a tampered corpus (R-05).
- `skip_hash_validation` branch (`:216`) is untouched — the `--skip-hash-validation` escape hatch still bypasses.

## File-size (C-06)

`import/mod.rs` is near the 500-line ceiling; this refactor DELETES ~35 lines (the HashSet + dual-loop body)
and adds ~12, net reducing the file. Confirm post-edit line count stays under 500. No new module needed.

## Key test scenarios (hints)

1. **R-04 regression tripwire.** Run ALL pre-existing `validate_hashes`/import tests UNCHANGED first. Any
   newly-failing case must be justified by the stronger `supersedes`-keyed link check, never by loosened
   tolerance. Document each diff.
2. **AND-halves via import (AC-04).** (a) Import a corpus with correct content-hash but a broken
   `previous_hash` link ⇒ import fails. (b) Import a corpus with a correct link but mutated content ⇒ import
   fails. Both proven on the IMPORT path (not only CLI) — the single oracle enforces the AND.
3. **Deprecated predecessor via import (R-02).** Import an export whose predecessors are Deprecated; assert
   import chain-verify PASSES (they load via the all-status query on the in-flight txn).
4. **Atomic ROLLBACK on tamper (R-05).** Import a tampered export; assert `Err` returned, transaction
   ROLLBACKs, and post-failure entry count == pre-import count (no rows from that import remain).
5. **Clean COMMIT (R-05).** Import a clean corrected export; assert COMMIT succeeds, rows present with intact
   `previous_hash`/`version`.
6. **Mixed legacy round-trip (AC-05, FR-10).** Export→import a corpus mixing legacy (`previous_hash==""`) and
   chained entries; assert values byte-identical after re-import AND import re-verify clean.
