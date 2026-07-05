# Component 1 — chain-verify-core

**File:** `crates/unimatrix-store/src/chain_verify.rs` (new, ~120 lines) + register in `crates/unimatrix-store/src/lib.rs`.
**Source of truth:** ARCHITECTURE §Verify algorithm, ADR-003 (entry #5504), BRIEF §Verify Algorithm, SPEC FR-05/FR-06.
**Traces:** FR-05, FR-06, NFR-05, NFR-06; AC-03, AC-04; R-03, R-09, R-12; C-02, C-07.

## Purpose

The single, transport-agnostic integrity oracle. Given a slice of `EntryRecord`, it recomputes each entry's
content hash and verifies each populated chain link against the authoritative `supersedes` predecessor. It
performs **no I/O** and takes **no CLI/MCP types** (C-07) — callers (import, CLI, future MCP) supply the
entries. Violations are returned as data (`ChainReport`), not errors — the report is always produced; callers
decide the failure surface.

## Module registration

In `crates/unimatrix-store/src/lib.rs`:
```
pub mod chain_verify;
```
Public items reachable as `unimatrix_store::chain_verify::{verify_entries, ChainReport, ChainViolation, ViolationKind}`.

## Types (the shared integration surface — see OVERVIEW)

```
pub struct ChainReport { pub checked: usize, pub skipped_legacy: usize, pub violations: Vec<ChainViolation> }
pub struct ChainViolation { pub entry_id: u64, pub kind: ViolationKind }
pub enum ViolationKind {
    ContentHashMismatch { computed: String, stored: String },
    ChainLinkMismatch   { predecessor_id: u64, expected: String, found: String },
    MissingPredecessor  { predecessor_id: u64 },
    DanglingPreviousHash,
}
```

Derive `Debug`, `Clone`, `PartialEq, Eq` on `ChainViolation`/`ViolationKind` so tests can assert exact
variants (R-09 requires each variant produced by a dedicated scenario). Derive `Debug` on `ChainReport`.

## Function: `verify_entries`

```
pub fn verify_entries(entries: &[EntryRecord]) -> ChainReport

  // O(entries): build id -> &EntryRecord map first, then a single linear pass (NFR-05).
  index : HashMap<u64, &EntryRecord> = entries.iter().map(|e| (e.id, e)).collect()
  report = ChainReport { checked: 0, skipped_legacy: 0, violations: [] }

  for e in entries:
      report.checked += 1                      // every entry examined (counter semantics per OVERVIEW; R-02)

      // ---- (1) Content-hash recompute (AC-04 half) --------------------------
      computed = compute_content_hash(&e.title, &e.content)   // FROZEN signature (C-01)
      if computed != e.content_hash:
          report.violations.push(ChainViolation { entry_id: e.id,
              kind: ContentHashMismatch { computed, stored: e.content_hash.clone() } })
          // NOTE: do NOT `continue` — an entry can have BOTH a content mismatch and a broken link;
          // report both. Fall through to the chain-link check.

      // ---- (2) Chain-link check (AC-03; C-02 legacy skip) -------------------
      if e.previous_hash.is_empty():
          report.skipped_legacy += 1           // unverifiable-legacy / genesis — NOT a break (FR-06, matches import:429)
          continue                             // no link to verify for this entry

      // previous_hash is populated -> the link MUST resolve via the authoritative supersedes edge
      match e.supersedes:
          None =>
              // a populated link with no chain edge should not occur by construction; fail loud, never ignore (R-09)
              report.violations.push(ChainViolation { entry_id: e.id, kind: DanglingPreviousHash })
          Some(pred_id) =>
              match index.get(pred_id):
                  None =>
                      report.violations.push(ChainViolation { entry_id: e.id,
                          kind: MissingPredecessor { predecessor_id: pred_id } })
                  Some(pred) =>
                      if pred.content_hash != e.previous_hash:
                          report.violations.push(ChainViolation { entry_id: e.id,
                              kind: ChainLinkMismatch {
                                  predecessor_id: pred_id,
                                  expected: pred.content_hash.clone(),   // what the link SHOULD be
                                  found:    e.previous_hash.clone(),      // what it IS
                              } })
                      // else: link verified — nothing to record

  return report
```

**Why key on `supersedes` (not "some known hash"):** the link check asserts
`successor.previous_hash == predecessor.content_hash` where predecessor is the *specific* entry named by
`supersedes`. This is strictly stronger than the old existence check (import/mod.rs:429 "previous_hash is in
the set of known hashes") and subsumes it (R-04, Non-Goal 6 / A-2: the supersedes topology is trusted; we
verify the hash along it, not the topology).

## `is_clean` and human-readable output (NFR-06, SR-05)

```
impl ChainReport:
    pub fn is_clean(&self) -> bool { self.violations.is_empty() }   // false whenever violations non-empty (R-12)

    pub fn describe(&self) -> String:
        if self.is_clean():
            return "chain OK: {checked} entries checked, {skipped_legacy} legacy (unverifiable) skipped"
        else:
            header = "chain integrity FAILED: {N} violation(s) over {checked} entries checked"
            lines  = for v in violations: "  entry {v.entry_id}: {describe kind}"
                       ContentHashMismatch -> "content hash mismatch (computed {computed}, stored {stored})"
                       ChainLinkMismatch   -> "chain link mismatch vs predecessor {predecessor_id} (expected {expected}, found {found})"
                       MissingPredecessor  -> "predecessor {predecessor_id} not found in corpus"
                       DanglingPreviousHash-> "previous_hash set but supersedes is None (dangling link)"
            return header + "\n" + lines.join("\n")

impl Display for ChainReport { write self.describe() }
```

Output names **every** offending `entry_id` (AC-04, SR-05, NFR-06). Emit ids + hashes only — never raw
`content` (security: avoids echoing untrusted content / terminal-escape injection).

## Error handling

- The core returns a value, never `Err` — violations are data. It cannot panic on any input slice: empty
  slice ⇒ `checked=0, skipped_legacy=0, violations=[]` (`is_clean()==true`); single entry, duplicate-id, and
  self-referential `supersedes` are all handled by map-lookup semantics (no recursion, no unwrap).
- `compute_content_hash` is total (no failure mode). No `.unwrap()` anywhere.

## Key test scenarios (hints for the tester — not the test plan)

1. **Clean multi-hop chain** A(v1,genesis)→B(v2)→C(v3): `is_clean()`, `checked==3`, `skipped_legacy==1`
   (A only), no violations. (AC-03)
2. **Deprecated predecessor counted (R-02).** A Deprecated (superseded_by set) + B Active with populated
   `previous_hash==hash(A)`: `is_clean()`, and A is included in `checked` (proves it was in the slice).
3. **Mixed legacy + chained (R-03).** Legacy entries (`previous_hash==""`) + chained entries verify clean;
   `skipped_legacy` counts every empty-hash entry.
4. **ContentHashMismatch (AC-04).** Mutate an entry's `content` without fixing `content_hash` ⇒ exactly
   `ContentHashMismatch`, names the id.
5. **ChainLinkMismatch.** Correct content but `previous_hash != predecessor.content_hash` ⇒ `ChainLinkMismatch`
   with `predecessor_id`/`expected`/`found`.
6. **DanglingPreviousHash (R-09).** Non-empty `previous_hash` with `supersedes==None` ⇒ `DanglingPreviousHash`.
7. **MissingPredecessor (R-09).** `supersedes==Some(id)` where id absent from slice ⇒ `MissingPredecessor{id}`.
8. **Both violations on one entry.** Content mutated AND link broken ⇒ two violations for that id (no early
   `continue` after content check).
9. **is_clean property (R-12).** For every non-empty `violations`, `is_clean()==false`.
10. **Empty / single-entry corpus.** No panic; counts sane.
