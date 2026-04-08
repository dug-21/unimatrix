# crt-051-agent-1-architect Report

## Outputs

- `product/features/crt-051/architecture/ARCHITECTURE.md`
- `product/features/crt-051/architecture/ADR-001-contradiction-density-score-input.md`
- ADR-001 stored in Unimatrix as entry #4259

## Key Decisions

1. Signature change: `contradiction_density_score(total_quarantined: u64, total_active: u64)` → `contradiction_density_score(contradiction_pair_count: usize, total_active: u64)`. Formula structure identical; only the first argument's source and meaning change.
2. Call site: pass `report.contradiction_count` (not `report.total_quarantined`) at `status.rs:747`. Phase ordering invariant (Phase 2 before Phase 5) is confirmed safe and should be documented by a comment at the call site.
3. Pair count (not unique entry count) as normalization — human confirmed.
4. Cold-start = 1.0 (optimistic) — human confirmed. Already produced by default since `contradiction_count` initializes to 0.
5. SR-02 fixture fix: `make_coherence_status_report()` in `response/mod.rs` must set `contradiction_count: 15` (with existing `total_active: 50`) to preserve the hardcoded `contradiction_density_score: 0.7000`. The seven other fixtures need no change.
6. GRAPH_EDGES writing remains out of scope.

## Integration Surprises Found

1. **`status.rs` path differs from SCOPE.md:** SCOPE.md references `crates/unimatrix-server/src/tools/status.rs` but the actual file is `crates/unimatrix-server/src/services/status.rs`. The `tools/` directory does not exist as a module; the correct path is `services/status.rs`. Delivery agent must use the correct path.

2. **`response/mod.rs` path differs from SCOPE.md:** SCOPE.md references `crates/unimatrix-server/src/response/mod.rs` but the actual file is `crates/unimatrix-server/src/mcp/response/mod.rs`. Delivery agent must use the correct path.

3. **SR-02 fixture value is not formula-derived:** The 0.7000 in `make_coherence_status_report()` is not `1.0 - (3/50) = 0.940` — it was manually assigned as a scenario value. This means the fixture was never coherently computing its own score. After the fix the discrepancy becomes a compilation-level break (assertion failure), not a silent drift. The correct fix is `contradiction_count: 15` to make the fixture self-consistent.

4. **`contradiction_density_score` unit test names encode old semantics in strings (SR-01):** Test `contradiction_density_quarantined_exceeds_active` and `contradiction_density_no_quarantined` have "quarantined" in their names. These are misleading after the fix even if the numeric values are unchanged. Full rename required as first-class deliverable.

## Open Questions

None — all design questions were resolved before handoff.

## Knowledge Stewardship

- Queried: context_search(query: "coherence scoring Lambda contradiction patterns", category: "pattern")
- Queried: context_search(query: "crt-051 architectural decisions", category: "decision", topic: "crt-051")
- Stored: entry #4259 "ADR-001 contradiction_density_score input source" via context_store (category: decision, tags: ["adr", "crt-051"])
