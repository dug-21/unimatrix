## ADR-003: base_score Trust-Source Differentiation — Active Status Only

### Context

Change 3 adds trust-source differentiation to `base_score`. Auto-sourced Active entries should
score lower (≈0.35) than human/agent/system Active entries (0.5). This widens the trust gap that
`trust_score("auto") = 0.35` provides via W_TRUST, giving auto-extracted entries a consistent
lower baseline even before usage or vote signals accumulate.

SR-04 flagged a scope boundary risk: `test_scenarios.rs` defines `auto_extracted_new()` with
`Status::Proposed` and `trust_source: "auto"`. T-REG-01 asserts `good > auto > stale >
quarantined`. If `base_score` differentiation applies to `Proposed` status as well as `Active`,
the auto-proposed entry's base score drops to 0.35, potentially invalidating the `auto > stale`
ordering where `stale` uses `base_score(Deprecated) = 0.2`.

The SCOPE offers two implementation paths:
- Clean two-parameter signature change: `base_score(status: Status, trust_source: &str) -> f64`
- Delegating to `compute_confidence` only: handle differentiation at the composite level, not
  the component level.

### Decision

**`base_score` differentiation applies to `Status::Active` entries only.**

`base_score(Status::Proposed, "auto")` returns 0.5 — unchanged from current behavior.
Only `base_score(Status::Active, "auto")` returns 0.35.

This preserves T-REG-01's `auto > stale` ordering: the `auto_extracted_new()` scenario uses
`Status::Proposed`, which continues to return 0.5. The stale scenario uses
`Status::Deprecated` → `base_score = 0.2`, so `auto (0.5) > stale (0.2)` holds unchanged.

**Implementation: clean two-parameter signature change.**

```rust
pub fn base_score(status: Status, trust_source: &str) -> f64 {
    match status {
        Status::Active => {
            if trust_source == "auto" { 0.35 } else { 0.5 }
        }
        Status::Proposed => 0.5,
        Status::Deprecated => 0.2,
        Status::Quarantined => 0.1,
    }
}
```

The `compute_confidence` path was considered (add a `if trust_source == "auto" { subtract delta
}` at the composite level) but rejected: it splits the component/weight model — `base_score`
would no longer represent the full component value, creating a hidden adjustment that calibration
tests do not see cleanly. The signature-change path is explicit and testable.

**Call site blast radius (confirmed from source):**

1. `confidence.rs` line 205: `base_score(entry.status)` → `base_score(entry.status, &entry.trust_source)` (production)
2. `confidence.rs` unit test `base_score_active` → requires two assertions: one with `"agent"` and one with `"auto"`
3. `confidence.rs` unit tests `base_score_proposed`, `base_score_deprecated`, `base_score_quarantined` → add `""` as second argument
4. `pipeline_calibration.rs` line 94: `base_score(entry.status)` in `confidence_with_adjusted_weight` → add `&entry.trust_source`
5. Any other direct call sites (verify during implementation by checking for `base_score(` pattern)

All call sites are mechanical 1-line updates with no logic change.

**T-REG-02 update (SR-06):** T-REG-02 must be updated before implementing the weight changes —
update the constant assertions first, then change the constants, confirm the test passes. The
`base_score` differentiation is independent of T-REG-02 but the two changes (weights and
base_score) must both be done before calibration tests are run to completion.

### Consequences

**Easier:**
- T-REG-01 ordering is preserved without modification.
- `base_score` remains fully testable as a pure function.
- The differentiation is visible and explicit in both code and tests.
- Auto-sourced Active entries start 0.027 lower (W_BASE × 0.15 = 0.16 × 0.15 ≈ 0.024) than
  agent entries, widening the trust gap.

**Harder:**
- 5 call sites require mechanical updates.
- New unit tests needed for `base_score(Active, "auto")` vs `base_score(Active, "agent")`.
- The `auto_vs_agent_spread` calibration scenario must verify that with otherwise equal signals,
  an `"auto"` trust_source entry scores below an `"agent"` trust_source entry.
- `compute_confidence` signature now takes `alpha0, beta0` AND `base_score` takes `trust_source`
  — implementers must update both dimensions consistently.
