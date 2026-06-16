## ADR-007 (vnc-037): Symmetric-edge canonicalization — collapse `Contradicts`/`CoAccess`/`Informs` to one `↔` edge in SQL BEFORE ranking and BEFORE counting (SR-08 blocker)

### Context

Three relation types are stored as **two reciprocal rows** (A→B *and* B→A):
- `Contradicts` (authored) — the reverse row is written at `edge_write.rs:211-223`.
- `CoAccess` (behavioral, S8) — both directions at `graph_enrichment_tick.rs:442-478`.
- `Informs` (behavioral, S1/S2) — forward and reverse at `behavioral_signals.rs:244-308`.

All other types are **single-row / asymmetric**: `Prerequisite`, `Supports` (authored, one
row, direction meaningful), `Supersedes` (excluded at SQL, ADR #4461), and the newer
semantic types.

The depth-1 `Both` read does `outgoing.extend(incoming)` with **no dedup**. So a symmetric
edge between the read entry and a neighbor appears **twice** — once as outgoing (A→B), once
as incoming (B→A). Under the reframe this is a **High-severity blocker (SR-08)** on two
surfaces at once:
1. **Display.** Two rows for one relationship waste two of only three slots (D-05) and
   render the same neighbor twice — and `#4083` confirms `LIMIT` caps *rows*, not edges, so
   a symmetric type yields 2N rows for N relationships.
2. **Totals.** The split `COUNT(*)` (D-05) double-counts the relationship — once inbound,
   once outbound — corrupting the honest-totals invariant that the feedback loop and
   #744/#745 observability depend on (`#3618`: double-counts need explicit JOIN/CASE guards).

A canonicalization miss is a **silent** defect: a plausible-looking build with
double-counted totals and a duplicated neighbor.

### Decision

**Canonicalize the three symmetric relation types to one logical `↔` edge in SQL, BEFORE
`ORDER BY…LIMIT` (ADR-006) and BEFORE the split `COUNT(*)` (ADR-001).** Both the displayed
set and the totals operate on the canonicalized set, so each is deduped independently.

1. **Canonicalize in SQL, not Rust.** Within the ranked query and the count query, fold the
   reciprocal pair into a single row before ranking/counting. The canonical-pair predicate
   keeps exactly one row per `{relation_type, unordered endpoint pair}` for the symmetric
   types — e.g. keep the row where `source_id < target_id` (or the read-entry-anchored row)
   for symmetric `relation_type`s, while letting asymmetric types pass through unchanged. The
   choice of canonical anchor is an implementation detail for the spec/pseudocode; the
   **invariant** is: one row per symmetric relationship, before `ORDER BY` and before
   `COUNT(*)`. Doing it in SQL keeps the fan-out bound (SR-14) — Rust-side dedup would
   require materializing the full set first.
2. **Symmetric set is exactly these three types** (`Contradicts`, `CoAccess`, `Informs`) —
   A2. The canonicalization predicate enumerates them explicitly. Any future symmetric type
   added without updating this list will double-count (documented hazard, A2).
3. **Direction semantics (the D-02 fix).** A canonicalized symmetric edge carries the `↔`
   glyph and `direction = "both"` (ADR-002) — **no** spurious `→`/`←`. `→`/`←` remain
   meaningful **only** for asymmetric types (`Prerequisite`/`Supports`). Single-row
   asymmetric types are unaffected by canonicalization.
4. **Counting.** A `↔` edge counts **once** in the split totals (D-05), attributed to the
   direction of its canonical row (or a defined convention — spec decides which bucket; the
   invariant is once, not twice). Asymmetric edges count in their actual direction.
5. **Test both surfaces independently (SR-08, AC-10).** A `Contradicts` (and a `CoAccess`,
   an `Informs`) pair stored as both rows must (a) render as **one** `↔` edge, not two, and
   (b) contribute **one** to the totals, not two — asserted as **separate** tests on display
   and on totals (`#4083`).

### Consequences

- **Easier:** one relationship = one slot and one count, so the scarce three slots and the
  honest totals are both correct; the `↔` glyph reads as a genuinely bidirectional
  relationship rather than two confusing one-way arrows; doing it in SQL keeps the hub-node
  fan-out bound (SR-14) — no full-set materialization.
- **Harder:** the canonicalization predicate is non-trivial SQL that must run identically in
  *two* queries (ranked select + split count) — a drift between them re-introduces a
  double-count on one surface only, which is why both are tested independently; the symmetric
  type list is a hard-coded set (A2) that a future symmetric relation type can silently
  invalidate — the spec must flag "adding a symmetric type requires updating
  canonicalization"; the canonical-anchor choice (`source_id < target_id` vs read-anchored)
  affects which direction bucket a `↔` edge's count lands in, so the convention must be
  stated and tested.
- **Cross-ref:** ADR-001 (the two queries that both apply this first), ADR-006 (ranking runs
  on the canonicalized set), ADR-002 / ADR-005 (`↔` / `direction="both"` rendering),
  ADR #4461 (`Supersedes` already excluded — not part of canonicalization).
- **Grounded in:** `edge_write.rs` / `graph_enrichment_tick.rs` / `behavioral_signals.rs`
  (the three reverse-row writers), `#4083` (LIMIT caps rows not edges), `#3618` (double-count
  needs explicit guards).
