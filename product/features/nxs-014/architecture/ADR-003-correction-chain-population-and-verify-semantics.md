## ADR-003: Correction Chain Population (Both Literal Sites) + Chain-Verify Semantics

### Context

`correct_entry` (`unimatrix-store/src/write_ext.rs`) hardcodes the chain link in **two
independent** places, and the verify semantics must tolerate forward-only legacy data:

- **SR-06 (headline):** the successor's `previous_hash`/`version` are hardcoded at BOTH the
  `EntryRecord` struct literal (`:539-540`, `previous_hash: String::new()`, `version: 1`) AND the
  INSERT binds (`:582-583`, `.bind("")`, `.bind(1_i64)`). The INSERT does **not** read the record
  fields — it binds inline literals. Fixing only the struct compiles clean and still persists an
  empty link; a struct-level or in-memory test passes while the DB row is wrong.
- **SR-02:** the new chain check must skip empty `previous_hash` (D-2 forward-only legacy;
  `validate_hashes:429` already establishes this tolerance). Failing to skip raises a false-positive
  integrity alarm on every pre-existing corrected entry.
- The chain itself (`supersedes`/`superseded_by`) is authoritative and already correct
  (Non-Goal 6, A-2); verify walks it and trusts it.

`original` is already loaded at `write_ext.rs:462` with populated `content_hash` and `version`.

### Decision

**Write path — change both sites, reading from the loaded `original`:**

- Struct literal (`:539-540`): `previous_hash: original.content_hash.clone()`,
  `version: original.version + 1`.
- INSERT binds (`:582-583`): `.bind(&new_rec.previous_hash)` and `.bind(new_rec.version as i64)` —
  bind **from the record**, never inline literals again.

Both are non-negotiable; either alone leaves the bug half-fixed. The acceptance test MUST read
`previous_hash`/`version` **back from the DB** after correction (not from the in-memory record),
so the half-fix cannot pass green (SR-06).

**Verify semantics — implemented by the pure core `verify_entries` (ADR-001):** build an
`id -> EntryRecord` map; for each entry:

1. **Content hash:** recompute `compute_content_hash(title, content)`; mismatch →
   `ContentHashMismatch { computed, stored }`. (Catches content mutation — AC-04.)
2. **Chain link:** if `previous_hash` is empty → skip as **unverifiable-legacy** (increment
   `skipped_legacy`), NOT a break (SR-02, AC-03). Otherwise resolve the predecessor via the
   authoritative `supersedes` edge:
   - `supersedes == None` → `DanglingPreviousHash` (link with no chain edge — should not occur;
     fail loud, do not silently ignore).
   - predecessor not found → `MissingPredecessor { predecessor_id }`.
   - `predecessor.content_hash != previous_hash` → `ChainLinkMismatch { predecessor_id, expected,
     found }`. (AC-03.)

The report **names every offending entry id** and the break kind (fail-loud — AC-04, SR-05).
Keying the link check on `supersedes` (rather than the old "references some known hash" existence
check) makes it strictly stronger and subsumes the prior check.

Concrete example — a 3-hop chain `A(v1) → B(v2) → C(v3)`:
`B.previous_hash == hash(A)`, `C.previous_hash == hash(B)`, versions `1,2,3`. A legacy entry
`L` with `previous_hash == ""` is counted as `skipped_legacy`, not a break. Mutating `B`'s content
without perfectly rewriting `hash(B)` **and** `C.previous_hash` fails loud, naming `B` (content
mismatch) and/or `C` (chain-link mismatch).

### Consequences

- **Easier:** Architectural Principle 1 holds by construction on every correction; the tamper-record
  guarantee (ADR-002 boundary) is real. The DB-readback test closes the SR-06 false-green. Mixed
  legacy+new corpora verify without false alarms (SR-02).
- **Harder / required tests:** two non-negotiable tests — (a) read `previous_hash`/`version` back
  **from the DB** after an N-step correction chain and assert links + monotonic versions
  (AC-01/02); (b) a **mixed legacy(empty)+populated** corpus that verifies clean and, tampered,
  fails loud naming the entry (AC-03/04). Plus an export→import round-trip over that corpus (AC-05,
  SR-07) — columns already serialize, so no export code change.
- **Defensive note (A-1):** if a corrected `original` ever has an empty `content_hash`, the successor
  inherits an empty `previous_hash` (tolerated as legacy — degrades safely, no crash). Spec should
  assert `original.content_hash` non-empty at correction time.

Related: ADR-001 (where the verify core lives), ADR-002 (the frozen-hash boundary this operates within).
