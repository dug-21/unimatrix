# Agent Report — vnc-047-agent-1-pseudocode (Stage 3a)

**Feature:** vnc-047 — `context_cycle` whole-set-once run-identity tags (GH #940)
**Role:** Pseudocode specialist (per-component algorithm design)
**Session:** Session 2, Stage 3a. **Read-only knowledge tier.**

## Deliverables produced

Per-component pseudocode under `product/features/vnc-047/pseudocode/`:
OVERVIEW.md + C1 cycle_tags-migration.md, C2 store-write-primitive.md, C3 store-read-getter.md,
C4 hook-extraction.md, C5 listener-persistence.md, C6 cycle-params.md, C7 report-field.md,
C8 review-handler.md, C9 markdown-render.md, C10 gc-protection.md, C11 deferred-seam.md,
C12 ack-echo.md, C13 freeze-trace.md.

All 13 components mapped 1:1 to ARCHITECTURE C1–C13 and grounded against HEAD line references.

## Key design decisions encoded

- **C2 whole-set-once:** EXISTS-guard inside a `BEGIN IMMEDIATE` txn on a dedicated connection
  (precedent import/mod.rs:197), NOT `pool.begin()` (DEFERRED). Guard reads row existence only
  (value-opacity). Cycle_start row + guard + per-tag inserts are one atomic unit. Duplicate-within-set
  → `ON CONFLICT DO NOTHING`. Parameterized binds only (sole SQLi defense).
- **C2 signature verified against HEAD** (db.rs:320): 8-arg `insert_cycle_event` WITH `next_phase`,
  `goal_embedding` NOT written by the INSERT (nullable BLOB populated by the separate Step-6
  `update_cycle_start_goal_embedding` UPDATE, db.rs:438). Corrected a Stage 3a tester note that
  claimed otherwise; Gate 3a confirmed the pseudocode was right and routed the sweep to the tester.
- **Two independent cascades kept separate:** C1 v31 (real DB migration, 3 paths + DDL parity +
  pinned) vs C7 SUMMARY v6 (fidelity stamp + `#[serde(default)]` v5-blob backward-read + pinned).
- **C8 rmcp seam:** extracted `pub(crate) populate_review_tags(&Store, &str, &mut RetrospectiveReport)`
  so the AC-05 assembled test drives REAL `get_cycle_tags` + render without the non-constructible
  `#[tool]` handler (#5389 pattern).
- **C10 GC by omission:** no DELETE-path edits; regression test extended across BOTH
  `gc_cycle_activity` and `gc_unattributed_activity` with a `sessions` positive control.
- **C9 empty-render (Gate 3a rework):** PINNED to render NOTHING for a tag-less cycle (AC-05d "no
  spurious section") — an intentional divergence from `render_goal_section`, which always emits an
  empty `## Goal` section with a fallback (verified HEAD retrospective.rs:203-217).
- **C11 comment-only** (no stub); **C12 ack echo + C13 freeze trace** best-effort, NON-GATING.

## Gate 3a rework addressed (this iteration)

1. C9 empty-render pinned to "no section when tag-less" across purpose, pseudocode, notes, and test
   hints; documented the one-line divergence from goal parity.
2. This stewardship report emitted with the block below.
3. (Opportunistic, gate WARN) Stripped stray `</content>` EOF template tags from all pseudocode files.

The two test-plan signature sweeps (test-plan/OVERVIEW.md §7 OQ-1, test-plan/store-write-primitive.md)
are the tester's rework items, not mine.

## Open questions / flags

- C13 trace physically emitted inside C2 (to preserve the fixed `Result<()>` signature); the
  Component-Map "C13 = listener step-5" placement is approximate. Non-gating (Gate 3a: note only).
- C8 assembled test bypasses the rmcp `#[tool]` wrapper (identity/cap/format/memoization); that shell
  is covered by AC-06 registry/auth tests + crt-033 memoization tests, not AC-05. Flagged, non-blocking.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (category=pattern) — surfaced #4153 (schema bump = 3 paths +
  `>= N` test discipline), #836 (add-new-table procedure: `<` guard, single end-of-txn version stamp),
  #373 (junction-vs-JSON), #681 (create-new-then-swap). Applied #4153/#836 to C1.
- Queried: mcp__unimatrix__context_search (category=decision, topic=vnc-047) — surfaced ADR entries
  #5655 (GC by omission), #5659 (ack echo), #5653 (fire-and-forget durability); cross-read with the
  on-disk ADR-001/002/007 for C1/C2/C7/C10/C12.
- Queried: mcp__unimatrix__context_get — #4153, #836 (full detail).
- Referenced (via code, not stored): #5389 (rmcp `#[tool]` non-constructible → `pub(crate)` seam
  extraction) applied to C8; #5599 (entry-tag junction primitive port anchor) applied to C1/C2.
- Deviations from established patterns: one sanctioned divergence — C2 freeze is a whole-set EXISTS
  guard under BEGIN IMMEDIATE, NOT vnc-045's per-row `ON CONFLICT` accumulate (ADR-002, to preserve
  value-opacity / avoid namespace parsing). C9 empty-render intentionally diverges from
  `render_goal_section` to honor AC-05d. Both documented in the pseudocode. No unsanctioned deviations.
- Stored: nothing novel to store — read-only tier; no new reusable pattern beyond what the ADRs
  already capture. Interface-signature correction was a code defect in a peer artifact (routed to the
  tester + gate), not a lesson (bugs are GH issues, not Unimatrix lessons).
