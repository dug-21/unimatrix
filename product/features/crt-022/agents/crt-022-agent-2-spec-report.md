# Agent Report: crt-022-agent-2-spec

**Agent ID**: crt-022-agent-2-spec
**Role**: Specification Writer
**Feature**: crt-022 (W1-2: Rayon Thread Pool + Embedding Migration)
**Date**: 2026-03-19

---

## Deliverable

`product/features/crt-022/specification/SPECIFICATION.md`

---

## Key Decisions Made

**D-01 (Pool naming)**: Field/variable name `ml_inference_pool` used throughout, per
human approval resolving OQ-1. Applies to struct field name in `RayonPool`, variable
names at call sites, and `ServiceLayer` field.

**D-02 (AsyncEmbedService treatment)**: Specified as removal (dead code), not
migration. The researcher confirmed zero consumers in `unimatrix-server`. The spec
reflects this distinction explicitly — no "migration" language for this item.

**D-03 (Call site count)**: 7 server call sites migrate to rayon + 1 dead code
removal from `unimatrix-core`. The SCOPE.md lists 8 total (including the dead code
item); the spec clarifies the distinction in the Call Site Inventory table.

**D-04 (CI enforcement for AC-07)**: Converted from a post-ship grep audit (SCOPE.md
language) to a CI enforcement gate (C-09), per SR-05 recommendation. The spec
requires a CI step — not just a checklist item.

**D-05 (OQ-2 timeout — C-11)**: Treated as a prerequisite gate, not an open
implementation detail. The spec explicitly blocks Phase 3 (call-site migration) on
the architect's `ARCHITECTURE.md §timeout-semantics` resolution. This prevents the
implementer from making an undocumented timeout decision under time pressure.

**D-06 (Pool distribution via ServiceLayer)**: Made explicit in C-10 and FR-03,
grounded in SR-06 and the established `ServiceLayer` pattern (entries #316, #1560).

**D-07 (num_cpus dependency)**: Called out as "verify presence or add" in the
dependencies table. The SCOPE.md formula uses `num_cpus::get()` but does not confirm
the crate is already in `unimatrix-server/Cargo.toml`. Implementer must check.

---

## Open Questions Remaining

**OQ-2 (BLOCKING)**: Timeout semantics after `spawn_blocking_with_timeout` removal.
Must be resolved in `ARCHITECTURE.md §timeout-semantics` before call-site migration
(Phase 3) begins. Options: per-call `tokio::time::timeout`, `spawn_with_timeout`
variant on `RayonPool`, or documented acceptance of rayon work-stealing semantics.

All other open questions from SCOPE.md and researcher/architect reports are resolved
and reflected in the specification.

---

## Self-Check

- [x] SPECIFICATION.md covers all 11 acceptance criteria from SCOPE.md (AC-01 through AC-11)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (pool size bounds, thread count formula, rayon pool capped at 8 default)
- [x] Domain Models section defines key terms including ubiquitous language table
- [x] NOT in scope section is explicit (10 exclusions)
- [x] Output file is in `product/features/crt-022/specification/` only
- [x] No placeholder or TBD sections — OQ-2 flagged as blocking gate with explicit resolution path
- [x] Knowledge Stewardship report block included

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "rayon thread pool tokio bridge ML inference
  spawn_blocking" — found entries #2491 (bridge pattern) and #2535 (monopolisation
  pattern). Both directly informed NFR-04, FR-08, and the contradiction scan
  single-closure design.
- Queried: `/uni-query-patterns` for "OrtSession EmbedAdapter thread safety Send
  async wrappers" — found #2524, #68, #76. SR-01 prerequisite gate grounded in
  ADR-006 Send+Sync established pattern.
- Queried: `/uni-query-patterns` for "AppState ServiceLayer startup wiring Arc pool" —
  found #316 and #1560. C-10 and workflow 3 grounded in established ServiceLayer
  distribution pattern.
