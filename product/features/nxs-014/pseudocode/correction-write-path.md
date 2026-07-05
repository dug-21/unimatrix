# Component 2 — correction-write-path

**File:** `crates/unimatrix-store/src/write_ext.rs`, `correct_entry` (lines 439-629).
**Source of truth:** ADR-003 (entry #5504), BRIEF §Data Flow / Carry-Item R-01, SPEC FR-01/FR-02/FR-03/FR-04.
**Traces:** FR-01, FR-02, FR-03, FR-04; AC-01, AC-02, AC-08; R-01/SR-06, R-08; C-03, C-04.

## Purpose

Populate the cross-version chain link on every correction: set the new entry's `previous_hash` to the
superseded original's `content_hash` and `version` to `original.version + 1`, at **both** independent literal
sites (struct + INSERT bind). Add a defensive guard (FR-04) rejecting a correction whose `original` has an
empty `content_hash`.

## Preconditions (already true in the existing code — do not re-add)

- `original` is loaded at `:462` via `entry_from_row` and carries a populated `content_hash` and `version`.
- Status guards for Deprecated/Quarantined originals already exist at `:471-482`.
- `content_hash` for the NEW record is computed at `:516-517` from `correction.title`/`content` — unchanged (C-01).

## Change 1 — FR-04 guard (reject empty predecessor content_hash)

Insert AFTER the existing status validation (after `:482`), BEFORE building `new_rec` (`:518`). This fails the
correction loud and early — nothing is persisted (the txn has only read + not yet inserted the new row; the
`UPDATE ... Deprecate` at `:489` has NOT run yet at this point, so returning here leaves the DB untouched).

```
// FR-04 / A-1: a correction must chain onto a real predecessor hash. An empty original.content_hash
// would otherwise silently produce an empty previous_hash that the verify core tolerates as "legacy",
// laundering a real bad-state into an unverifiable skip (R-08). Reject it, naming original_id.
if original.content_hash.is_empty():
    return Err(StoreError::InvalidInput {
        field:  "original.content_hash".to_string(),
        reason: format!("cannot correct entry {original_id}: predecessor has empty content_hash \
                         (chain link would be malformed)"),
    })
```

**Placement rationale:** must run before the `Deprecate` UPDATE (`:489`) so no partial mutation occurs on
rejection (R-08 test: "failure occurs at correction time, before any row is persisted"). Since it only reads
`original` (already in hand at `:462`), placing it anywhere in `:483..:488` satisfies this. Recommend
immediately after the Quarantined check at `:482`.

## Change 2 — struct literal site (FR-01), currently `write_ext.rs:539-540`

Inside the `let new_rec = EntryRecord { ... }` literal:

```
    // was: previous_hash: String::new(),
    previous_hash: original.content_hash.clone(),   // FR-01: link to superseded predecessor's content hash
    // was: version: 1,
    version: original.version + 1,                  // FR-01/FR-03: monotonic per-chain counter
```

## Change 3 — INSERT bind site (FR-02), currently `write_ext.rs:582-583`

The INSERT (`:549-590`) binds columns `previous_hash` (`?20`, currently `.bind("")` at `:582`) and `version`
(`?21`, currently `.bind(1_i64)` at `:583`). Bind FROM the record fields — never inline literals:

```
    // was: .bind("")            // previous_hash (?20)
    .bind(&new_rec.previous_hash)          // FR-02: persist the SAME value the struct holds
    // was: .bind(1_i64)         // version (?21)
    .bind(new_rec.version as i64)          // FR-02: u32 -> i64 bind, matches column type
```

**Why both (C-03 / R-01 — the headline half-fix):** the struct literal and the INSERT binds are independent.
Fixing only Change 2 compiles clean and the returned in-memory `new_rec` looks correct, but the persisted row
still has `previous_hash=""`, `version=1` because Change 3's inline literals win at write time. Fixing only
Change 3 (without Change 2) persists correctly but returns a stale in-memory record. **Both are mandatory.**

## Data flow

```
in:  original: EntryRecord (loaded :462, content_hash + version populated), correction: NewEntry
     guard: original.content_hash non-empty (else Err, persist nothing)
transform: new_rec.previous_hash = original.content_hash; new_rec.version = original.version + 1
out: persisted row (INSERT binds from new_rec) AND returned (original*, new_rec) tuple agree on the link
```

`u32 -> i64` bind is lossless for `version` (R-07 boundary): `version as i64` widens; SQLite stores i64;
`entry_from_row` reads it back into `u32`. No truncation for any realistic `version`.

## Error handling

- FR-04 guard returns `StoreError::InvalidInput { field, reason }` naming `original_id` (AC-08). Consistent
  with the existing status-guard errors at `:471-482`.
- No other new error paths; the existing `map_err(StoreError::Database)` on the INSERT is unchanged.
- No `.unwrap()`; both new field reads (`original.content_hash`, `original.version`) are infallible struct
  accesses.

## Key test scenarios (hints)

1. **AC-01 DB read-back (the false-green killer, R-01).** Drive `correct_entry`; then issue a FRESH
   `SELECT previous_hash, version FROM entries WHERE id = <new_id>`; assert `previous_hash == original.content_hash`
   and `version == original.version + 1`. This test FAILS on a struct-only (Change-2-only) fix — it is the
   required authority; an assertion on the returned in-memory `new_rec` alone is INSUFFICIENT (C-04).
2. **AC-02 multi-hop (N=3), DB read-back.** A→B→C corrections; read all three rows back; assert each hop's
   persisted `previous_hash == predecessor.content_hash` and versions are `1,2,3` in supersession order.
3. **AC-08 empty-predecessor reject (R-08).** Construct/inject an active `original` with empty `content_hash`;
   attempt correction; assert `Err(InvalidInput)` naming `original_id`, and assert NO new row + NO Deprecate
   mutation occurred (post-attempt row count and original.status unchanged).
4. **Returned record agrees with DB.** The returned `new_rec.previous_hash`/`version` match the DB read-back
   (proves Change 2 and Change 3 are consistent, not just individually present).
