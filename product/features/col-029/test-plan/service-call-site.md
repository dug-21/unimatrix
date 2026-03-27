# Test Plan: service-call-site

Component: `StatusService::compute_report()` Phase 5 call site in
`crates/unimatrix-server/src/services/status.rs`

---

## Scope

The service call site has no runtime unit tests — its correctness is validated through:

1. Static grep checks verifying single call site and pool usage (AC-15, AC-17).
2. The non-fatal error path (`Err(e) => tracing::warn!(...)`) is a simple match arm with no
   side effects beyond logging; its structure is compile-verified.
3. End-to-end coverage via integration smoke tests (`test_status_empty_db`,
   `test_status_all_formats`) which confirm the full Phase 5 code path executes without panic.

This component covers R-06 (tick path exclusion) and the non-fatal error handling requirement
from the RISK-TEST-STRATEGY.md Failure Modes section.

---

## Static Checks (AC-11, AC-15, AC-17)

All three static checks are run as part of Gate 3c delivery review.

### Single Call Site Check (AC-15, R-06)

```bash
grep -rn "compute_graph_cohesion_metrics" crates/
```

**Expected output:** Exactly ONE result, located in `services/status.rs` inside
`compute_report()`. The function must NOT appear in:
- `load_maintenance_snapshot()`
- `maintenance_tick()`
- Any other function in `services/status.rs` or elsewhere.

**Failure signal:** Two or more results indicate the function was inadvertently placed in the
tick path. Zero results indicate the call site was not added.

### Pool Usage Check (AC-17)

```bash
grep -A 10 "fn compute_graph_cohesion_metrics" crates/unimatrix-store/src/read.rs
```

**Expected:** `self.read_pool()` appears in the function body. `write_pool_server()` must NOT
appear.

```bash
grep "write_pool_server" crates/unimatrix-store/src/read.rs
```

**Expected:** No matches in any code related to `compute_graph_cohesion_metrics`. ADR-003
mandates `read_pool()` exclusively.

### Six Field Assignments Check (AC-11)

```bash
grep -n "graph_connectivity_rate\|isolated_entry_count\|cross_category_edge_count\|supports_edge_count\|mean_entry_degree\|inferred_edge_count" \
  crates/unimatrix-server/src/services/status.rs
```

**Expected:** Six assignment lines, one per field, inside the `Ok(gcm) =>` arm of the match
in Phase 5.

### ADR-003 Comment Presence (R-11)

```bash
grep -n "read_pool\|ADR-003\|WAL\|snapshot" crates/unimatrix-store/src/read.rs | grep -A 2 -B 2 "compute_graph_cohesion"
```

**Expected:** A comment near the `read_pool()` call in `compute_graph_cohesion_metrics()`
that references ADR-003 and notes the WAL snapshot semantics are intentional. This prevents
a future developer from "correcting" the pool choice back to `write_pool_server()` (R-11
mitigation).

---

## Non-Fatal Error Path

The `Err(e) => tracing::warn!("graph cohesion metrics failed: {e}")` arm requires no runtime
unit test because:

1. It contains no logic that can fail independently — it is a single `tracing::warn!` call.
2. Injecting a store error requires a mock or a corrupted pool, neither of which is available
   in the standard test infrastructure without introducing a new test abstraction.
3. The pattern is identical to the Phase 4 co-access error handling which is already present
   and tested implicitly by the integration test suite.

**Integration coverage:** `test_status_empty_db` in `test_tools.py` exercises the full
`compute_report()` code path including Phase 5 against a real (empty) store. A store error
is not expected, but the path through `Ok(gcm)` is exercised, confirming the match arm
structure is wired correctly.

**Structural verification:** `cargo check -p unimatrix-server` confirms the match arm is
exhaustive and the `Err` branch compiles. No `StatusReport` fields are mutated in the error
arm — the defaults from `StatusReport::default()` are preserved.

---

## Phase 5 Placement Verification

The call to `compute_graph_cohesion_metrics()` must appear after the HNSW `graph_quality_score`
assignment and before the lambda computation. The ARCHITECTURE confirms this sequencing.

**Verification:** Code review at Gate 3c. The reviewer confirms the call is within the
`// Phase 5` comment block and not in the `load_maintenance_snapshot()` branch.

---

## Summary of Verification Items

| Check | Method | AC / Risk |
|-------|--------|-----------|
| Single call site in `compute_report()` | `grep -rn "compute_graph_cohesion_metrics" crates/` | AC-15, R-06 |
| Call NOT in `maintenance_tick()` or `load_maintenance_snapshot()` | same grep, negative assertion | AC-15, R-06 |
| `read_pool()` used, `write_pool_server()` absent | grep on `read.rs` | AC-17 |
| All six fields assigned in `Ok(gcm)` arm | grep on `services/status.rs` | AC-11 |
| ADR-003 comment at call site in `read.rs` | grep for comment keyword | R-11 |
| Error arm does not abort report | compile + smoke integration test | Failure Modes |
