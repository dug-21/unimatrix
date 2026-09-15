# vnc-049-synthesizer report

Compiled Session 1 design outputs into implementation-ready deliverables.

## Produced
- `product/features/vnc-049/IMPLEMENTATION-BRIEF.md` — source links, component map (C1..C9) + cross-cutting artifacts, goal, resolved-decisions table (references ADR-001..009 file paths), files to create/modify, data structures, function signatures, constraints, dependencies, NOT-in-scope, alignment status, C18 ledger guardrail, open questions.
- `product/features/vnc-049/ACCEPTANCE-MAP.md` — AC-01..07 each mapped to its binding verification; AC-06 end-path (C18 gating, R-01) and AC-03 stored-record (R-02) assertions authoritative/non-proxy; supporting risk scenarios R-04/05/10/13/14/15.
- GH Issue #986 updated with a design-complete comment: https://github.com/dug-21/unimatrix/issues/986#issuecomment-5688785357

## Notes
- Issue #986 already existed (body synced to approved SCOPE, labels goal:personal-cloud/goal:platform/enhancement preserved) — updated via comment, no duplicate created.
- SCOPE.md §Tracking already carries #986; no SCOPE edit needed.
- Ingest-persistence reframe (ADR-001) carried prominently into brief + issue: event-derived resolution alone reads OpenCode back as claude-code; attribution persisted at ingest via schema migration.

## Open questions carried to delivery
- OQ-2/R-05 (WARN): write-time source_domain stamp scope — opencode-only (safe default) vs generalized. Human/architect decision before delivery.
- OQ-3/R-11: session.idle→Stop PoC + over-count guard.
- OQ-4/R-02: pin the derivation site producing the stored source_domain.
- Schema version number confirmation (latest v27→v28).
- AC-04/R-06: child-before-parent ordering race.
- OQ-6: subagent MCP connection (does not gate this cycle).
- OQ-7: PreCompact experimental-API flag default.
