# Agent Report: crt-044-agent-1-pseudocode

**Agent ID:** crt-044-agent-1-pseudocode
**Date:** 2026-04-03
**Stage:** Session 2 Stage 3a — Pseudocode

---

## Outputs Produced

| File | Lines | Description |
|------|-------|-------------|
| `product/features/crt-044/pseudocode/OVERVIEW.md` | ~70 | Component interaction, data flow, shared types, sequencing constraints |
| `product/features/crt-044/pseudocode/migration_v19_v20.md` | ~160 | v19→v20 two-statement SQL back-fill pseudocode |
| `product/features/crt-044/pseudocode/graph_enrichment_tick_s1_s2_s8.md` | ~200 | Second write_graph_edge call per pair in S1/S2/S8 tick functions |
| `product/features/crt-044/pseudocode/graph_expand_security_comment.md` | ~80 | Two-line // SECURITY: comment at graph_expand signature |

---

## Components Covered

1. **migration_v19_v20** — `crates/unimatrix-store/src/migration.rs`
   - `CURRENT_SCHEMA_VERSION` bump: 19 → 20
   - `if current_version < 20` block with Statement A (S1+S2 Informs) and Statement B (S8 CoAccess)
   - Full SQL for both statements with NOT EXISTS guards
   - Transaction scope, error propagation, idempotency layers
   - 10 key test scenarios including per-source, exclusion, idempotency, and empty-table cases

2. **graph_enrichment_tick_s1_s2_s8** — `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`
   - Second `write_graph_edge` call per pair in `run_s1_tick`, `run_s2_tick`, `run_s8_tick`
   - Full pseudocode for each modified loop body with exact argument positions
   - `pairs_written` semantic change documented (per-pair → per-edge, 2× for new pairs)
   - Three-case return contract for `write_graph_edge` captured (entry #4041)
   - 6 key test scenarios including bidirectionality, steady-state false-return, and counter assertions

3. **graph_expand_security_comment** — `crates/unimatrix-engine/src/graph_expand.rs`
   - Exact two-line `// SECURITY:` comment text specified
   - Exact insertion point identified (between closing `///` doc line and `pub fn graph_expand(`)
   - Before/after diff shown
   - Confirmed zero logic change

---

## Source Files Read

- `product/features/crt-044/IMPLEMENTATION-BRIEF.md`
- `product/features/crt-044/architecture/ARCHITECTURE.md`
- `product/features/crt-044/specification/SPECIFICATION.md`
- `product/features/crt-044/RISK-TEST-STRATEGY.md`
- `product/features/crt-044/architecture/ADR-001-migration-strategy.md`
- `product/features/crt-044/architecture/ADR-002-forward-write-pattern.md`
- `product/features/crt-044/architecture/ADR-003-security-comment-approach.md`
- `crates/unimatrix-store/src/migration.rs` (lines 1-30, 600-720 — v18→v19 block as template)
- `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` (full, lines 1-447)
- `crates/unimatrix-engine/src/graph_expand.rs` (lines 1-100 — signature context)

---

## Open Questions / Flags

None. All open questions from SCOPE.md were resolved before pseudocode stage (OQ-1, OQ-2, OQ-3).

One delivery-sequencing note (not a blocker for pseudocode):

- **R-02 (Critical):** If crt-043 has already merged and consumed v20, the implementation agent
  must renumber this migration to v21 and update all version references: `CURRENT_SCHEMA_VERSION`,
  the `if current_version < 20` guard, the `UPDATE counters SET value = 20` statement, and all
  test fixtures. The pseudocode uses v20 throughout on the assumption crt-044 merges first.

- **graph_enrichment_tick.rs actual location:** The file is at
  `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`, not
  `crates/unimatrix-store/src/services/graph_enrichment_tick.rs` as stated in the IMPLEMENTATION-BRIEF.
  The ARCHITECTURE.md also states the wrong crate. The implementation agent must use the server crate path.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (pattern: graph edge back-fill migration) — returned
  entries #3889 (back-fill filter by source pattern) and #4078 (S8 gap pattern). Both directly
  used in pseudocode: #3889 informs the `source` discriminator choice over `created_by`; #4078
  confirms S8 was not covered by v18→v19.
- Queried: `mcp__unimatrix__context_search` (decision: crt-044) — returned ADR entries #4079,
  #4080, #4081. All three ADRs read in full.
- Queried: `mcp__unimatrix__context_briefing` — confirmed entries #3889, #4078, #4080 as top
  relevant results; entry #4041 (write_graph_edge return contract) also surfaced and incorporated
  into tick pseudocode.
- Deviations from established patterns: none. Pseudocode follows the `co_access_promotion_tick.rs`
  two-call pattern (ADR-002) and the v18→v19 migration template exactly.
