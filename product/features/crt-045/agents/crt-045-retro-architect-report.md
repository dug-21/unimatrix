# crt-045 Retrospective — Architect Report

> Agent: crt-045-retro-architect (uni-architect)
> Date: 2026-04-03
> Feature: crt-045 — Eval Harness: Wire TypedGraphState Rebuild into EvalServiceLayer

---

## 1. Patterns

### New Entries
None. All relevant patterns were stored during delivery:
- #4096 — EvalServiceLayer cold-start pattern (stored by delivery agent)
- #4097 — Arc::clone verification before write-after-construction (stored by design agent)
- #4103 — Three-layer integration test for eval service layer graph wiring (stored by test-plan agent)
- #4104 — Supersedes cycle detection requires UPDATE entries.supersedes not INSERT GRAPH_EDGES (stored by delivery agent)

### Updated Entries
None required. Verification findings below confirm all stored entries are accurate.

### Verification: #4097 (Arc::clone before write-after-construction)

Accurate and complete. The implementation at `layer.rs:389–395` confirmed the propagation mechanism: `ServiceLayer::with_rate_config()` stores `typed_graph_state` at line 399, passes `Arc::clone(&typed_graph_state)` to `SearchService::new()` at line 419, and exposes `typed_graph_handle()` returning another `Arc::clone` at line 297. All three consumers share the same `RwLock<TypedGraphState>` allocation — the pre-implementation verification checklist in #4097 precisely describes what needed checking. No update needed.

### Verification: #4104 (Supersedes cycle detection trap)

Accurate and complete. The delivery agent discovered this trap at implementation time when the raw SQL `INSERT INTO graph_edges` approach from the test plan did not trigger cycle detection. `build_typed_relation_graph` Pass 2b explicitly skips `relation_type=Supersedes` rows because they are derived from `entries.supersedes` in Pass 2a. The entry correctly documents the root cause and the fix. No update needed.

### Verification: #4103 (Three-layer test pattern)

Accurate. The fixture detail is complete: Active entries only, `bootstrap_only=0` edges, VectorIndex dumped to sibling `vector/` dir, raw SQL via `store.write_pool_server()`. The Layer 3 acceptance of `EmbeddingFailed` (not only `Ok(_)`) is not explicit in the entry but is CI-specific context rather than generalizable knowledge. No update needed.

### Skipped: Integration test module splitting pattern

The rework at gate-3b required extracting `seed_graph_snapshot()` into a shared helper and splitting new tests into `layer_graph_tests.rs` when `layer_tests.rs` hit 677 lines. This is already covered by two existing entries:
- #3778 — "When a test file split still exceeds 500 lines, split tests.rs into tests.rs + feature_tests.rs, sharing helpers via pub(super)"
- #3583 — "Eval Harness render.rs 500-Line Split: New Section = New render_*.rs Module"

The crt-045 variant (adding a sibling `layer_graph_tests.rs`) is the same principle as #3778. Not generalizable beyond what's already stored.

---

## 2. Procedures

### Existing: #3549 — Eval Harness Baseline Recording Procedure

No update needed. The procedure documents the `unimatrix eval run` baseline recording steps used for R-09 (mrr_floor drift check). crt-045's use of this procedure was standard — R-09 was correctly deferred as a manual pre-merge check per RISK-TEST-STRATEGY.md. The procedure itself did not change and the steps remain accurate.

The eval infrastructure referenced in the briefing (`product/test/eval-baselines/log.jsonl` + `run_eval.py` in ass-039) was used per the existing procedure. No new procedure entry is needed.

---

## 3. ADR Status

All five ADRs are validated by the delivery. Entries #4098, #4099, and #4101 are already stored in Unimatrix.

| ADR | Entry | Status | Validation Evidence |
|-----|-------|--------|---------------------|
| ADR-001: Post-construction write-back | #4098 | Validated | `layer.rs:389–395` write-lock swap implemented exactly as specified; `layer.rs:453` delegates to `self.inner.typed_graph_handle()` |
| ADR-002: Degraded mode on rebuild failure | #4099 | Validated | `test_from_profile_returns_ok_on_cycle_error` confirms `Ok(layer)` with `use_fallback=true`; both error arms set `rebuilt_state = None` and continue |
| ADR-003: Three-layer test (live search) | No standalone entry; covered by #4103 | Validated | Gate 3b initially failed without the three-layer structure; the test plan correctly required all three layers; prevented wired-but-unused failure mode |
| ADR-004: pub(crate) accessor | #4101 | Validated | `layer.rs:452` confirmed `pub(crate)` without `#[cfg(test)]` guard; also useful to `runner.rs` (C-10 validated) |
| ADR-005: distribution_change=false | No standalone entry | Validated | `ppr-expander-enabled.toml` confirmed with `distribution_change=false`, `mrr_floor=0.2651`, `p_at_5_min=0.1083`; parse test `test_parse_no_distribution_change_flag` passes |

No ADRs flagged for supersession.

---

## 4. Lessons

### A. Gate 3b failure: seed_graph_snapshot() helper not extracted

The gate-3b failure (677-line file, 500-line cap) was caused by the delivery agent inlining the full seeding logic (store open + entry insert + edge insert + vector dump) in both new test functions rather than extracting the `seed_graph_snapshot()` helper that OVERVIEW.md had specified.

Check against existing lessons:
- #3778 covers the module-split remedy (already applied)
- #3386 covers "Stage 3b agents implement happy-path but skip edge-cases" — a different failure mode (omission)
- #3935 covers tracing-test AC deferral — unrelated

The specific failure here is: **pseudocode specified a helper function by name, the delivery agent did not implement it, and the omission caused a gate failure**. This is distinct from existing lessons. However, the gate-3b report itself noted the general lesson (extract shared test helpers to stay under 500-line cap) is already in `rust-workspace.md` rules and does not need a separate Unimatrix entry.

Assessment: Agree with gate-3b reviewer. The rule is already in project conventions. The specific mechanism (missing named pseudocode helper) is a one-feature occurrence without enough recurrence evidence to store as a lesson. Skip.

### B. Compile cycles (47 cycles)

#3932 fully covers this pattern. 47 cycles is within the range documented there (nan-002: 60, crt-036: 76) and consistent with `layer.rs` + test iteration. No new lesson needed.

### C. context_store failures (16 transient)

Transient failures that all resolved successfully. No systemic pattern. The briefing notes 25 entries stored vs mean 9.5 — this is explained by 5 ADRs × 2 stores (design agent + corrections). Not a process problem, not lesson-worthy.

### D. P@5 flat despite MRR improvement (R-09 eval result)

MRR +0.0122 improvement confirmed; P@5 flat because graph edge density is insufficient for top-5 reranking benefit. This is a measurement observation specific to the current knowledge base state, not a generalizable lesson about eval methodology. The existing eval baseline lesson (#4085) covers snapshot-pinning concerns. Skip.

### New lessons stored: None

No new lessons meet the bar — all candidates are either covered by existing entries or are feature-specific observations without recurrence evidence.

---

## 5. Retrospective Findings

### Hotspot Actions

| Hotspot | Assessment | Action |
|---------|------------|--------|
| cold_restart (172-min gap, 12 re-reads) | Session break between design and delivery. Expected for human-paced workflows. No agent confusion. | None |
| context_load (161 KB before first write) | Security reviewer role — reading all prior work before analysis is expected behavior. | None |
| file_breadth (67 files) | Full protocol run touches design artifacts, codebase, test files, gate reports. Within normal range. | None |
| compile_cycles (47) | Within range for single-file fix with test rework. Covered by #3932. | None |
| mutation_spread (46 files) | Mostly design artifacts from full protocol run. Expected. | None |
| context_store failures (16 transient) | All entries stored successfully. Not a process problem. | None |
| session_timeout (2.9h gap) | Human pause. Not an agent issue. | None |

### Outlier Notes

| Outlier | Value | Mean | Assessment |
|---------|-------|------|------------|
| knowledge_entries_stored | 25 | 9.5 | Inflated by 5 ADRs × 2 stores each. Not a process problem; ADR count (5) is modestly above typical (3) but the feature scope warranted all five decisions. |
| parallel_call_rate | 0.3 | 0.2 | Positive. Agents batched independent tool calls correctly. |
| post_completion_work_pct | 0.0 | 3.9 | Positive. Clean stop with no work after gate-3c. |
| permission_friction_events | 0.0 | 11.3 | Positive. No permission issues throughout delivery. |

### Gate 3b Rework Assessment

The single gate failure (layer_tests.rs 677 lines) was a straightforward rework: extract `seed_graph_snapshot()` helper, split new tests into `layer_graph_tests.rs`. The pseudocode had specified the helper — this was a delivery gap, not a design gap. The rework was clean and gate-3b passed on iteration 2 with no further issues. The 500-line cap is enforced correctly by the gate process.

The `entries.supersedes` vs `graph_edges` test deviation (WARN in gate-3b) was correctly identified and documented as intentional. The delivery agent's finding was immediately stored as #4104. This is a positive stewardship example — non-obvious trap caught, documented in real time.

---

## Summary

| Category | New | Updated | Skipped | Reason for Skip |
|----------|-----|---------|---------|----------------|
| Patterns | 0 | 0 | 2 | Integration test split covered by #3778/#3583; eval layer patterns already stored during delivery |
| Procedures | 0 | 0 | 1 | Existing #3549 accurate; R-09 used it without modification |
| ADRs | 0 | 0 | — | All 5 validated; 3 already in Unimatrix |
| Lessons | 0 | 0 | 4 | Compile cycles (#3932), helper omission (in project rules), transient store failures (no pattern), P@5 flat (feature-specific observation) |

Existing entries verified accurate: #4096, #4097, #4098, #4099, #4101, #4103, #4104.
