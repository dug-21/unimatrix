# Gate 3b Report: crt-045

> Gate: 3b (Code Review)
> Date: 2026-04-03
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All three changes match pseudocode exactly |
| Architecture compliance | PASS | ADRs followed; post-construction write pattern correct |
| Interface implementation | PASS | C-01 through C-10 all satisfied |
| Test case alignment | WARN | Cycle test uses entries.supersedes UPDATE instead of GRAPH_EDGES INSERT per test plan; correctly documented as intentional deviation |
| Code quality — compiles | PASS | `cargo build -p unimatrix-server` finishes with 0 errors |
| Code quality — no stubs | PASS | No todo!(), unimplemented!(), TODO, FIXME, or placeholder code |
| Code quality — no unwrap in non-test | PASS | No .unwrap() calls in layer.rs production code |
| Code quality — file size | FAIL | layer_tests.rs is 677 lines; exceeds 500-line cap |
| Security | PASS | No hardcoded secrets; no path traversal; no command injection |
| cargo audit | WARN | cargo-audit not installed; could not verify |
| Knowledge stewardship | PASS | Both rust-dev agents have Queried and Stored/reasoned entries |

## Detailed Findings

### Pseudocode Fidelity

**Status**: PASS

**Evidence**:

1. **Step 5b (rebuild call):** `layer.rs` lines 188–215 implement the exact match structure specified in pseudocode. `TypedGraphState::rebuild(&*store_arc).await` is called with `.await` directly. The `Ok(state)` arm calls `tracing::info!` with `profile` and `entries` fields and stores `Some(state)`. The `Err(e)` arm inspects `e.to_string().contains("cycle")` to select the correct `tracing::warn!` message; both error arms leave `rebuilt_state = None` and continue — no `?` propagation.

2. **Step 13b (write-back):** `layer.rs` lines 389–395 match pseudocode exactly: `if let Some(state) = rebuilt_state`, acquire write lock with `unwrap_or_else(|e| e.into_inner())`, swap `*guard = state`, immediately `drop(guard)`.

3. **Accessor:** `layer.rs` lines 446–454 — `pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle` delegates to `self.inner.typed_graph_handle()`. No `#[cfg(test)]` guard (C-10). Matches pseudocode verbatim.

**Minor deviation — extra info! log at Step 13b:** Pseudocode specifies one `info!` log in the `Ok(state)` arm of Step 5b. The implementation also emits `tracing::info!("eval TypedGraphState rebuilt")` at line 394 (inside the Step 13b `if let` block) after performing the write. This results in two info-level log events on successful rebuild. The second log is not specified in the pseudocode but does not violate any correctness constraint or NFR-04 (which only prohibits error!-only or debug!-only logging).

### Architecture Compliance

**Status**: PASS

**Evidence**:

- ADR-001 (post-construction write): implemented correctly — rebuild before `with_rate_config()`, write-back after via `inner.typed_graph_handle()`.
- ADR-002 (degraded mode): both error paths return `Ok(layer)` with `rebuilt_state = None`; handle stays cold-start.
- ADR-003 (three-layer test): test has all three layers — handle state, `find_terminal_active` graph connectivity, and live `layer.inner.search.search()` call.
- ADR-004 (accessor visibility): `pub(crate)` without `#[cfg(test)]` guard.
- ADR-005 (TOML): `distribution_change = false` with explanatory comment block.
- `ServiceLayer::with_rate_config()` signature is unchanged (C-03, NFR-05).
- `ScenarioResult`, `ProfileResult`, runner/report types not touched (C-07).

### Interface Implementation

**Status**: PASS

**Evidence**: All constraints verified in code:

| Constraint | Status | Evidence |
|-----------|--------|---------|
| C-01: rebuild().await (no spawn_blocking) | PASS | line 188–189: `.await` direct |
| C-02: rebuild errors → warn + Ok(layer) | PASS | lines 199–214: both error arms warn, continue |
| C-03: with_rate_config() signature unchanged | PASS | verified by reading services/mod.rs (not modified) |
| C-04: typed_graph_handle() is pub(crate) | PASS | line 452 |
| C-05: snapshot is read-only | PASS | only reads in rebuild() |
| C-06: TOML values exact | PASS | mrr_floor=0.2651, p_at_5_min=0.1083, distribution_change=false |
| C-07: no type changes | PASS | no changes to ScenarioResult, ProfileResult |
| C-08: accessor delegates to self.inner | PASS | line 453 |
| C-09: test uses Active entries + real edge | PASS | lines 384–431 in layer_tests.rs |
| C-10: no #[cfg(test)] guard on accessor | PASS | accessor is unconditional |

### Test Case Alignment

**Status**: WARN

**Evidence**:

Test A (`test_from_profile_typed_graph_rebuilt_after_construction`): matches test plan and pseudocode completely.
- Layer 1: `!guard.use_fallback` and `guard.all_entries.len() >= 2` (lines 459–466)
- Layer 2: `find_terminal_active(id_a, ...)` returns `Some(id_a)` (lines 471–477)
- Layer 3: `layer.inner.search.search(...)` accepts `Ok(_)` or `EmbeddingFailed` (lines 512–528)

Test B (`test_from_profile_returns_ok_on_cycle_error`): the test plan specified inserting Supersedes cycle via raw SQL into `graph_edges`. The implementation instead uses `UPDATE entries SET supersedes = id_b WHERE id = id_a` (lines 596–608). This deviation is correct and intentional — as documented in the agent report, GRAPH_EDGES rows with `relation_type='Supersedes'` are skipped in `build_typed_relation_graph` Pass 2b because Supersedes edges are derived from `entries.supersedes` in Pass 2a. The raw SQL approach would not trigger cycle detection. The alternative implementation correctly produces a cycle and the test passes. The pseudocode (layer_tests.md) also contained a contingency note for this.

The WARN is for the test plan deviation being technically undocumented as a scope change, though the pseudocode itself acknowledged the implementation should "use whichever is simpler given the SqlxStore API available."

### Code Quality — Compilation

**Status**: PASS

**Evidence**: `cargo build -p unimatrix-server` finishes with zero errors. Output:
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```
17 pre-existing warnings (3 auto-fixable). None in modified files.

### Code Quality — No Stubs or Placeholders

**Status**: PASS

**Evidence**: No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or placeholder functions found in `layer.rs` or `layer_tests.rs`.

### Code Quality — No .unwrap() in Non-Test Code

**Status**: PASS

**Evidence**: No `.unwrap()` calls in `layer.rs` production code. Write-lock acquisition uses `.unwrap_or_else(|e| e.into_inner())` (poison recovery pattern). Canonicalize uses `.unwrap_or_else(|_| ...)`.

### Code Quality — File Size

**Status**: FAIL

**Evidence**: `layer_tests.rs` is 677 lines. The 500-line cap applies to all source files.

Before crt-045: `layer_tests.rs` was 384 lines (confirmed via `git show 9122165a`). crt-045 added 293 lines (two new tests with full inline seeding logic — no shared helper), bringing the total to 677 lines.

**Issue**: This exceeds the 500-line limit by 177 lines. The two new test functions (`test_from_profile_typed_graph_rebuilt_after_construction` at ~172 lines and `test_from_profile_returns_ok_on_cycle_error` at ~102 lines) could partially be extracted to a shared helper (e.g., `seed_graph_snapshot()` as specified in the pseudocode's OVERVIEW.md) to reduce duplication. The pseudocode explicitly proposed this helper — it was not implemented.

**Fix**: Extract shared seeding logic (store open + entry insert + edge insert + vector dump) into a `seed_graph_snapshot()` async helper inside `mod layer_tests`. This would remove ~80–100 lines of duplication. Target: under 550 lines (or split into a second test module file if still over 500).

### Security

**Status**: PASS

**Evidence**:
- No hardcoded secrets, API keys, or credentials in any modified file.
- No file path operations that accept user-controlled input without validation.
- No command injection vectors.
- `SqlxStore::open_readonly()` is read-only; no writes to snapshot store.
- Serialization via sqlx with parameterized queries (`.bind()`) — no SQL injection risk.

### cargo audit

**Status**: WARN

**Evidence**: `cargo-audit` is not installed in this environment (`error: no such command: audit`). Cannot verify CVE status. This is an environmental gap, not a code defect. Pre-existing tooling issue.

### Knowledge Stewardship

**Status**: PASS

**Evidence**:

`crt-045-agent-3-eval-service-layer-report.md`:
- `Queried: mcp__unimatrix__context_briefing` — returned 14 entries
- `Stored: entry #4104 "To trigger Supersedes cycle detection in tests, UPDATE entries.supersedes — not INSERT INTO graph_edges" via /uni-store-pattern`

`crt-045-agent-3-toml-fix-report.md`:
- Queried: not invoked with explicit reason — "TOML-only edit, no implementation patterns to surface"
- `Stored: nothing novel to store -- the key finding is a one-time schema observation specific to this feature`

The TOML agent's decision not to query is a minor process deviation (the stewardship rule says "evidence of /uni-query-patterns before implementing"), but the agent provided an explicit reason and the TOML-only nature of the task makes the deviation defensible. Not escalating to FAIL.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| layer_tests.rs exceeds 500-line cap (677 lines) | uni-rust-dev | Extract shared seeding logic into a `seed_graph_snapshot()` async helper (as specified in pseudocode OVERVIEW.md), removing duplicated store-open + entry-insert + edge-insert + vector-dump code from both new tests. Target: under 500 lines total, or split into a second test file if necessary. |

## Knowledge Stewardship

- Stored: nothing novel to store -- the file-size-cap failure pattern from crt-045 is feature-specific (test file growth from inline seeding); the general lesson (extract shared test helpers to stay under 500-line cap) is already captured in the rust-workspace.md rules and does not need a separate Unimatrix entry.
