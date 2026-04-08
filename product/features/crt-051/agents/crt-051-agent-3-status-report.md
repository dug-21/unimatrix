# Agent Report: crt-051-agent-3-status

## Task

Implement the `status.rs` call site change for crt-051: replace `report.total_quarantined`
with `report.contradiction_count` as the first argument to
`coherence::contradiction_density_score()`, and add the phase-ordering comment.

## Files Modified

- `crates/unimatrix-server/src/services/status.rs`

## Changes Made

At Phase 5 (~lines 747–750), replaced:

```rust
report.contradiction_density_score =
    coherence::contradiction_density_score(report.total_quarantined, report.total_active);
```

With:

```rust
// report.contradiction_count is populated in Phase 2 (contradiction cache read);
// Phase 5 must not be reordered above Phase 2. See crt-051 ADR-001.
report.contradiction_density_score =
    coherence::contradiction_density_score(report.contradiction_count, report.total_active);
```

The `generate_recommendations()` call site (~lines 786–792) was read and confirmed
unchanged — still passes `report.total_quarantined`.

## Verification

### Static checks (test plan scenarios)

- S-01 (AC-06): `contradiction_density_score(report.contradiction_count` — exactly 1 match in status.rs. PASS.
- S-02 (AC-09): Grep `contradiction_density_score.*total_quarantined` across workspace — zero matches in `crates/`. PASS.
- S-02 (AC-09): Grep `total_quarantined.*contradiction_density_score` across workspace — zero matches in `crates/`. PASS.
- S-03 (AC-16): Phase-ordering comment present at Phase 5 call site. PASS.
- S-04 (AC-08): `generate_recommendations()` call site still passes `report.total_quarantined`. PASS.
- S-05 (AC-07): Phase 2 block precedes Phase 5 block in sequential code. PASS (structural, not reordered).

### Build

`cargo build -p unimatrix-server` — no errors, 18 pre-existing warnings. PASS.

## Test Results

No new unit tests required for this component (per test-plan/status.md). The call site
change is verified by static analysis; pure function behavior is covered by coherence.rs
unit tests (agent-1's scope).

## Issues / Blockers

None. The type mismatch (`u64` → `usize`) that the brief flagged as a compile-time
correctness guard was confirmed — the compiler would have caught any omission. Coherence.rs
(agent-1) had already been updated before this agent ran, so no type error occurred.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_search(query: "crt-051 architectural decisions", category: "decision", topic: "crt-051") — returned entry #4259 (ADR-001: contradiction_density_score uses scan pair count instead of quarantine counter). Confirms the design decision: raw pair count from ContradictionScanCacheHandle, optimistic 1.0 cold-start default.
- Stored: nothing novel to store — the call site pattern (read a field populated in an earlier sequential phase, pass it to a pure scoring function) is standard Rust and the phase-ordering comment technique is straightforward. No runtime traps or crate-specific gotchas were discovered.
