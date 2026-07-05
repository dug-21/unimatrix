# Agent Report — nxs-014-agent-1-architect

## Task
Design architecture for nxs-014: wire the cross-version hash chain (weak mode) in
context_correct, plus a transport-agnostic chain-verify core exposed via CLI.

## Artifacts
- `product/features/nxs-014/architecture/ARCHITECTURE.md`
- `product/features/nxs-014/architecture/ADR-001-chain-verify-core-placement.md` (Unimatrix #5502)
- `product/features/nxs-014/architecture/ADR-002-weak-mode-threat-boundary.md` (Unimatrix #5503)
- `product/features/nxs-014/architecture/ADR-003-correction-chain-population-and-verify-semantics.md` (Unimatrix #5504)

## Key Decisions
- **Verify core placement (SR-03, load-bearing):** new `unimatrix-store::chain_verify` module,
  a PURE `verify_entries(&[EntryRecord]) -> ChainReport`. Store is a leaf crate; no cycle; both
  the v1 CLI and a future MCP tool (both in server) are thin callers. Rejected server placement.
- **validate_hashes refactor:** becomes a thin adapter loading entries from its import transaction
  connection and calling the same core — one integrity oracle, not two.
- **CLI:** new `Command::Verify` + `unimatrix-server::verify::run_verify`, mirroring `run_import`;
  opens `SqlxStore::open_readonly`, exit non-zero on a break.
- **Write path (SR-06):** fix BOTH the struct literal (:539-540) and the INSERT binds (:582-583),
  binding from the record; acceptance test reads back FROM THE DB.
- **Verify semantics (SR-02):** skip empty `previous_hash` as unverifiable-legacy; key the link
  check on the authoritative `supersedes` edge; fail loud naming the offending entry id.
- **Threat boundary (SR-01/04):** `compute_content_hash` FROZEN; tamper-RECORDED not tamper-evident;
  no cascade/anchor/migration; README wording pinned.

## Open Questions
1. Confirm `query_all_entries()` returns Deprecated (superseded) entries — predecessors are
   Deprecated. If status-filtered, CLI loader needs an all-status query.
2. `ChainReport` v1 output is human-readable text; a `--json` flag is a possible follow-up for the
   MCP wrapper, not v1.
3. Capability #5478 wording (tamper-EVIDENCE) vs weak-mode delivery (tamper-RECORDED) — reconcile
   before marking `proven` (vision session, non-blocking).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned integrity capabilities (#5475/#5478),
  goal #5474, the single-oracle pattern (vnc-034 #4948), the test-support feature-flag pattern
  (#747), and hash-verification lessons (#4642/#4648); applied the single-oracle and
  leaf-crate-placement guidance directly to the core-placement decision.
- Stored: #5502 ADR-001 (core placement), #5503 ADR-002 (weak-mode threat boundary), #5504
  ADR-003 (correction population + verify semantics), all via context_store category=decision.
  No typed edges asserted — no cross-feature link meets the traversal-necessity bar (ADR-004 is
  reaffirmed, not superseded); intra-feature Prerequisite spine left for retro per convention.
