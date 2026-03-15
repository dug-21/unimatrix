# Gate 3b Report: crt-018b

> Gate: 3b (Code Review)
> Date: 2026-03-14
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All components match pseudocode logic; one naming deviation (see findings) |
| Architecture compliance | PASS | ADR-001/002/003/004 all honored; integration points match |
| Interface implementation | PASS | All function signatures match; `EffectivenessStateHandle` non-optional per ADR-004 |
| Test case alignment | PASS | All pseudocode test scenarios implemented; test plan coverage complete |
| Code quality — compile | PASS | `cargo build --workspace` clean with 0 errors |
| Code quality — no stubs | PASS | No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` found |
| Code quality — no bare unwrap | WARN | One `.unwrap()` on line 413 of `background.rs` guarded by `is_some()` check |
| Code quality — file size | WARN | `background.rs` production code (~1007 lines pre-test-module) exceeds 500-line limit; pre-existing condition in `briefing.rs` |
| Security — input validation | PASS | `UNIMATRIX_AUTO_QUARANTINE_CYCLES` validated at startup; rejects > 1000 and non-integers |
| Security — no hardcoded secrets | PASS | `SYSTEM_AGENT_ID = "system"` is a compile-time constant, not user-controlled |
| Security — lock poison recovery | PASS | All lock acquisitions use `.unwrap_or_else(|e| e.into_inner())` on `RwLock`/`Mutex` ops |
| Security — lock ordering R-01 | PASS | Read guard explicitly scoped to drop before `cached_snapshot.lock()` at all sites |
| NFR-02 — write lock before SQL | PASS | Write lock collected into block ending at line 501; SQL calls begin at line 505 |
| R-02 — all 4 call sites apply delta | PASS | Steps 7 and 8 (4 call sites), plus Step 11 ScoredEntry construction |
| AC-04 — utility_delta inside penalty | PASS | Formula is `(rerank + delta + prov) * penalty` at all sites; unit tests assert both |
| AC-12 — AUTO_QUARANTINE_CYCLES=0 | PASS | Guard at process_auto_quarantine top; also guarded in to_quarantine scan |
| Knowledge stewardship | PASS | Agent reports contain stewardship blocks with Queried entries |

## Detailed Findings

### Pseudocode Fidelity

**Status**: PASS

All six components (EffectivenessState, background tick writer, search utility delta, briefing tiebreaker, auto-quarantine guard, auto-quarantine audit) are implemented as specified in the pseudocode files.

One naming deviation is benign: the pseudocode calls the store method `quarantine_entry()` throughout, but the actual `Store` API is `update_status(entry_id, Status::Quarantined)`. No `quarantine_entry` method exists in the store crates. The implementation correctly uses the real API. This is a pseudocode label discrepancy, not an implementation fault, and does not affect correctness.

The `effectiveness_priority` scale in `briefing.rs` matches the Architecture Component 4 scale (Effective=2, Settled=1, None/Unmatched=0, Ineffective=-1, Noisy=-2), which supersedes the 3-2-1-0 scale stated in SPECIFICATION FR-07 (as directed by the pseudocode IMPLEMENTATION-BRIEF note in `briefing-tiebreaker.md`).

The `EffectivenessReport.all_entries` field — an open question in the pseudocode — was resolved correctly: the field is added to `EffectivenessReport` (`unimatrix-engine/src/effectiveness/mod.rs` line 119) and populated by `build_report()` (line 378). The tick writer reads it at line 427.

`Outcome::Failure` specified in the pseudocode does not exist in the `Outcome` enum; the actual enum variants are `Success`, `Denied`, `Error`, `NotImplemented`. The implementation correctly uses `Outcome::Error` for `tick_skipped` events, which matches the test assertion at line 1722 of `background.rs`.

### Architecture Compliance

**Status**: PASS

All four ADRs are honored:

- **ADR-001** (generation counter for clone avoidance): `EffectivenessSnapshot` held as `Arc<Mutex<EffectivenessSnapshot>>` in both `SearchService` and `BriefingService`. Generation comparison pattern is identical across both services. Shared `Arc` ensures rmcp-cloned service instances use the same cache (R-06 mitigation).

- **ADR-002** (hold-not-increment on tick error): `emit_tick_skipped_audit` is called and `Err(error)` is returned from `maintenance_tick` without touching `EffectivenessState`. Counter values are preserved from the previous successful tick.

- **ADR-003** (utility delta inside penalty multiplication): Verified at all four `rerank_score` call sites in `search.rs`. Step 7 formula: `base_a = rerank_score(...) + delta_a + prov_a; final_a = base_a * penalty_a`. Step 8 formula: `final_a = (base_a + delta_a + boost_a + prov_a) * penalty_a`. Unit tests `test_utility_delta_inside_deprecated_penalty` and `test_utility_delta_inside_superseded_penalty` numerically verify correct placement (lines 817-869).

- **ADR-004** (non-optional constructor param): `BriefingService::new()` takes `effectiveness_state: EffectivenessStateHandle` as a required parameter at `briefing.rs` line 153. `services/mod.rs` line 331 passes `Arc::clone(&effectiveness_state)` to the constructor. Missing wiring is a compile error.

### Interface Implementation

**Status**: PASS

All interfaces match the approved architecture:

- `EffectivenessState` struct fields: `categories: HashMap<u64, EffectivenessCategory>`, `consecutive_bad_cycles: HashMap<u64, u32>`, `generation: u64` — exact match.
- `EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>` — exact match.
- `EffectivenessSnapshot` with `generation: u64` and `categories: HashMap<u64, EffectivenessCategory>` — exact match.
- Constants `UTILITY_BOOST = 0.05`, `SETTLED_BOOST = 0.01`, `UTILITY_PENALTY = 0.05` — exact match; `SETTLED_BOOST < 0.03` invariant satisfied.
- `spawn_background_tick` signature includes `effectiveness_state: EffectivenessStateHandle` and `auto_quarantine_cycles: u32` as required.
- `BriefingService::new()` takes `effectiveness_state: EffectivenessStateHandle` — non-optional.
- `EffectivenessReport.auto_quarantined_this_cycle: Vec<u64>` added with `#[serde(default)]` and `all_entries: Vec<EntryEffectiveness>` added — both required fields present.

### Test Case Alignment

**Status**: PASS

Unit tests cover all pseudocode test scenarios:

- `services/effectiveness.rs`: 10 tests covering cold-start, generation, handle independence, snapshot sharing (R-06), poison recovery, read/write ordering (R-01).
- `services/search.rs`: 21 tests including `utility_delta` all 5 categories + None, constant invariants, AC-05 (Effective outranks near-equal Ineffective at cw=0.15 and cw=0.25), ADR-003 placement for Deprecated and Superseded, absent entry zero delta, clone sharing (R-06), lock ordering (R-01), generation cache.
- `services/briefing.rs`: 8 `effectiveness_priority` tests plus briefing clone sharing, sort tiebreaker tests (confidence primary, effectiveness secondary for both injection history and convention lookup), empty state no-panic.
- `background.rs`: 30+ tests covering tick write (categories update, generation increment), consecutive bad cycle semantics (increment/reset for all 5 categories, remove absent entry, three-tick recovery sequence), auto-quarantine threshold logic (threshold 0=disabled, threshold 1, threshold N, category restriction for Settled/Unmatched/Effective), parse validation for AUTO_QUARANTINE_CYCLES (default=3, zero valid, 1000 accepted, >1000 rejected, non-integer rejected), audit event schema fields, write lock scope test.

The test plan `outcome == Failure` assertion (background-tick-writer.md line 84) uses `Outcome::Error` in actual test code (line 1722), which matches the real enum variant. The test plan used the wrong variant name but the test itself is correct.

### Code Quality — Compile

**Status**: PASS

`cargo build --workspace` completes with 0 errors and 6 warnings (pre-existing; none in the crt-018b modified files). All test suites pass: 1295 tests in `unimatrix-server`, plus passing suites in all other crates.

### Code Quality — Stubs

**Status**: PASS

No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` markers found in any of the reviewed implementation files.

### Code Quality — Bare Unwrap

**Status**: WARN

One `.unwrap()` call in non-test production code at `background.rs` line 413:

```rust
let effectiveness_report = report.effectiveness.as_ref().unwrap();
```

This is immediately preceded by `if report.effectiveness.is_some()` at line 411, making the unwrap logically safe. However, it violates the project coding standard ("No `.unwrap()` in non-test code") regardless of correctness. The safe alternative would be `if let Some(effectiveness_report) = report.effectiveness.as_ref()`.

This is a WARN, not a FAIL — the code is correct and cannot panic under normal operation, but deviates from project style.

### Code Quality — File Size

**Status**: WARN

`background.rs` production code ends at line 1007 (test module begins at line 1009). The 500-line limit is exceeded by ~500 lines. This file grew from 650 total lines (pre-feature) to 1811 total lines. The growth is legitimate — the feature required adding tick writer, auto-quarantine, audit helpers, and their test suite in the same file per the architecture's cumulative test infrastructure constraint.

`briefing.rs` production code ends at line 576. Pre-existing condition (the file was 1478 lines before this feature). The crt-018b additions to production code in `briefing.rs` are minimal (~50 lines).

Neither exceedance is a new pattern introduced carelessly. Both are the direct result of the ARCHITECTURE requirement to extend existing files (not create isolated scaffolding). Flagging as WARN for operator awareness.

### Security

**Status**: PASS

All security requirements verified:

- `UNIMATRIX_AUTO_QUARANTINE_CYCLES` validated at startup by `parse_auto_quarantine_cycles_str()`: rejects non-integers with error, rejects values > 1000 with error, accepts 0 (disable). Startup error propagates through `main.rs` line 328 via `map_err(ServerError::ProjectInit)`.
- `SYSTEM_AGENT_ID = "system"` is a module-level `const &str` at line 44. Not derived from any request parameter or user input.
- All `RwLock` and `Mutex` acquisitions use `.unwrap_or_else(|e| e.into_inner())` for poison recovery. The single `.unwrap()` at line 413 is on an `Option<&EffectivenessReport>` (not a lock), and is guarded by `is_some()`.
- No path traversal, command injection, or hardcoded credentials found.
- Input validation: env var is parsed with `.parse::<u32>()` (rejects negatives and non-integers), with an upper bound of 1000.
- `cargo audit` not installed in the environment — cannot verify CVEs. This is an environment limitation, not a code issue.

### Lock Ordering R-01 / NFR-02

**Status**: PASS

**R-01 (read guard before mutex)**: In both `search.rs` and `briefing.rs`, the `effectiveness_state.read()` guard is inside an inner `{...}` block that returns only the `generation` field, dropping the guard before `cached_snapshot.lock()` is called. Verified at:
- `search.rs` lines 168-194: inner block returns `guard.generation`, guard drops, then `self.cached_snapshot.lock()` at line 177.
- `briefing.rs` lines 183-205: same pattern.
Unit test `test_snapshot_read_guard_dropped_before_mutex_lock` in `search.rs` exercises this exact sequence.

**NFR-02 (write lock before SQL)**: In `maintenance_tick()`, the write lock block is lines 419-501 (returning `candidates: Vec<(u64, u32, EffectivenessCategory)>`). The lock drops at line 501 when `to_quarantine` is bound. `process_auto_quarantine()` (containing all SQL calls via `spawn_blocking`) is called at line 505, after the write guard has gone out of scope. The comment at line 499 explicitly documents this: "Write lock drops here (end of block scope). CRITICAL: No store calls may be made inside this block."
Unit test `test_write_lock_not_held_after_tick_write_block` verifies the structural invariant.

### R-02 — All Four Call Sites Apply Utility Delta

**Status**: PASS

Exactly four `rerank_score` call sites exist in `search.rs` production code:
1. Step 7 comparator: `delta_a` / `delta_b` (lines 359-368)
2. Step 8 comparator: `delta_a` / `delta_b` (lines 430-435)

Each comparator applies `utility_delta(categories.get(&entry_id).copied())` to both entries being compared. A fifth application of `utility_delta` exists in Step 11 `ScoredEntry` construction (line 460-468) — consistent with the pseudocode's "Step 11 is not a rerank_score call site but should include delta for consistency." All four mandated call sites include the delta; Step 11 is additive.

## Rework Required

None. All gate checks pass or are WARN level.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for validation patterns on "write lock before SQL" — returning existing patterns #1366, #1542 (background tick writers).
- Stored: nothing novel to store — the `.unwrap()` guarded by `is_some()` pattern is a known style deviation, not a new failure pattern. The `Outcome::Failure` vs `Outcome::Error` enum variant mismatch between pseudocode and implementation is a narrow pseudocode documentation gap; too feature-specific to warrant a general lesson entry.
