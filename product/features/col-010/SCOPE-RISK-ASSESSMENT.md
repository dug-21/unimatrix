# col-010 Scope Risk Assessment

Feature: Session Lifecycle Persistence & Structured Retrospective
Date: 2026-03-02
Assessor: col-010-agent-0-scope-risk

---

## Risk Summary

| Severity | Count |
|----------|-------|
| Critical | 1     |
| High     | 2     |
| Medium   | 6     |
| Low      | 5     |
| **Total**| **14**|

---

## Risks

### SR-01 — col-009 Hard Dependency: Unmerged Prerequisite
**Severity**: Critical
**Component**: All
**Description**: col-010 has a hard dependency on col-009 (`drain_and_signal_session()`, `SignalOutput.final_outcome`, schema v4 SIGNAL_QUEUE table). The SCOPE.md acknowledges this: "GH Issue: TBD (to be created after col-009 merge)." The entire dependency chain is col-006 → col-007 → col-008 → col-009 → col-010. If col-009 carries any unresolved issues, col-010 is blocked at the gate. The `SessionClose` handler design — which intercepts `SignalOutput.final_outcome` to write the `SessionRecord` — cannot be implemented or tested until col-009 is stable.
**Mitigation**: Confirm col-009 PR is merged and all acceptance criteria pass before scoping implementation. Do not begin col-010 implementation on col-009 branches.
**Flag for architect**: Yes — gate check required.

---

### SR-02 — Feature Bundle Delivery Risk
**Severity**: High
**Component**: All 7 components
**Description**: col-010 bundles seven distinct deliverables: (1) schema v5 migration, (2) UDS listener session persistence, (3) session GC, (4) auto-outcome entries, (5) `from_structured_events()`, (6) tiered retrospective output + evidence synthesis (resolving issue #65), (7) lesson-learned auto-persistence + provenance boost. Components 1–5 are core session lifecycle persistence. Components 6–7 are retrospective quality improvements that could stand independently. A blocker in any component — particularly the more novel evidence synthesis or lesson-learned ONNX embedding path — delays the entire feature. The cumulative AC count (24 criteria) amplifies delivery risk.
**Mitigation**: Consider a clear internal priority order: components 1–5 are P0 (required for col-011); components 6–7 are P1 (resolve issue #65 but not col-011 blocking). If timeline is tight, P1 can slip to a follow-on feature without breaking the dependency graph.
**Flag for architect**: Yes — consider explicit P0/P1 split in implementation brief.

---

### SR-03 — context_retrospective Default Behavior Change
**Severity**: High
**Component**: Component 6 (tiered output)
**Description**: The default `detail_level` changes from returning ~87KB of raw evidence arrays to ~1-2KB summary. AC-17 preserves `detail_level = "full"` for backward compatibility, but existing callers using `context_retrospective` without specifying `detail_level` will silently receive a reduced response. This includes: any agent that uses raw evidence arrays from `hotspots[].evidence` directly, any test that asserts on evidence array contents, and the `uni-tester` or `ndp-tester` agents that inspect retrospective output. The SCOPE.md acknowledges this is "a user-visible behavior change." What is not acknowledged is the downstream test breakage risk — existing integration tests for `context_retrospective` likely assert on the old full format.
**Mitigation**: Audit existing tests for `context_retrospective` assertions before implementing the tiered output change. Update affected tests in the same PR. Document the default change prominently in the implementation brief.
**Flag for architect**: Yes — integration test audit required before implementing component 6.

---

### SR-04 — INJECTION_LOG Orphan Records on Session GC
**Severity**: Medium
**Component**: Component 1 (storage), Component 3 (GC)
**Description**: `gc_sessions()` deletes SESSIONS records older than 30 days. The corresponding INJECTION_LOG records are keyed by monotonic `u64` (`log_id`) with `session_id` stored as a field, not as a foreign key index. Nothing in the SCOPE.md specifies cascading deletes for INJECTION_LOG when sessions are removed. Over time, INJECTION_LOG accumulates orphaned records — injection events whose parent session no longer exists. The `from_structured_events()` function scans INJECTION_LOG and filters by `session_id` in-process; orphaned records for deleted sessions waste scan time and memory. At the stated volume of <5,000 records/day, after 30 days of steady use, this is ~150,000 orphaned records minimum.
**Mitigation**: `gc_sessions()` should also delete corresponding INJECTION_LOG records in the same write transaction. Add to `sessions.rs` GC logic: scan INJECTION_LOG entries where `session_id` is not present in SESSIONS, delete. Alternatively, batch-delete InjectionLog entries by session_id list during GC.
**Flag for architect**: Yes — GC design gap, must be addressed in specification.

---

### SR-05 — Schema Migration Idempotency Under Restart
**Severity**: Medium
**Component**: Component 1 (schema v5 migration)
**Description**: `migrate_v4_to_v5()` creates two new tables and writes `next_log_id = 0` to COUNTERS. The migration is triggered by `migrate_if_needed()` on `Store::open()`. If the server restarts after a partial migration (e.g., after SESSIONS table is created but before INJECTION_LOG is created, or before `next_log_id` is written), and `CURRENT_SCHEMA_VERSION` was not incremented yet, the migration will re-run. Writing `next_log_id = 0` again after injection records already exist would corrupt the counter. The prior migrations (v3→v4) created tables only; this migration also writes a counter value.
**Mitigation**: Write `next_log_id = 0` only if the key does not already exist in COUNTERS (check-then-write). Follow same pattern if prior migrations did this. Alternatively, write counter first, then create tables, then bump version — ensuring the counter write is idempotent.
**Flag for architect**: Verify migration transaction scope in specification.

---

### SR-06 — Abandoned Session Status Modeling Ambiguity
**Severity**: Medium
**Component**: Component 2 (UDS listener), Component 5 (structured retrospective)
**Description**: `SessionLifecycleStatus` has three variants: `Active`, `Completed`, `TimedOut`. Abandoned sessions are written as `status = Completed, outcome = "abandoned"` — there is no `Abandoned` status variant. Queries on `status = Completed` in `scan_sessions_by_feature()` will return abandoned sessions alongside genuine successes and rework sessions. The `from_structured_events()` function uses session outcomes for narrative synthesis — if abandoned sessions are included in the retrospective, they inflate metrics (e.g., injection counts from a failed/cancelled session). A distinct `Abandoned` variant would allow precise filtering.
**Mitigation**: Add `Abandoned` as a `SessionLifecycleStatus` variant. Update `scan_sessions_by_feature()` to support filtering by status. The `from_structured_events()` function should exclude `Abandoned` sessions from hotspot metric computation.
**Flag for architect**: Low-cost fix with meaningful correctness impact; recommend adding variant.

---

### SR-07 — ONNX Embedding Latency in context_retrospective Hot Path
**Severity**: Medium
**Component**: Component 7 (lesson-learned auto-persistence)
**Description**: AC-20 requires that `context_retrospective` auto-writes a `lesson-learned` entry with full ONNX embedding when ≥1 hotspot or recommendation is found. ONNX embedding on a narrative summary (Layer 2 content, potentially 500-1000 tokens) takes 100-500ms depending on hardware and model warmup state. The `context_retrospective` MCP tool is currently synchronous — this adds an unbounded blocking step to every retrospective call that produces results. This is distinct from session outcome entries (which correctly skip embedding). Retrospective calls during active development cycles (e.g., mid-feature-cycle check) would experience unexpected latency.
**Mitigation**: Write the lesson-learned entry via `spawn_blocking` with fire-and-forget, returning the retrospective report to the caller before embedding completes. The entry will be available for search on next query after embedding finishes. If embedding fails, log but do not fail the retrospective call. AC-20 should not require synchronous embedding completion.

---

### SR-08 — Evidence Synthesis Heuristic Fragility
**Severity**: Medium
**Component**: Component 6 (evidence synthesis for Layer 2)
**Description**: The 30-second timestamp clustering window and monotone-increasing sequence detection were designed based on one observed feature cycle (col-006). Different agents, different task types, and concurrent multi-agent sessions may produce event distributions that break these assumptions. Specifically: (a) 30-second windows may be too narrow for slow agent execution or too wide for fast tool calls; (b) monotone-increasing sequence detection for sleep escalation assumes a specific pattern — partial escalations or non-linear backoff will produce empty `sequence_pattern`; (c) top-5 file truncation loses information for highly distributed features. The heuristics may surface misleading narratives (e.g., false clustering) before sufficient calibration data exists.
**Mitigation**: Treat evidence synthesis as best-effort in Layer 2. Ensure `HotspotNarrative.summary` strings include confidence caveats ("based on X events") and that empty `sequence_pattern = None` is handled gracefully throughout. Consider making the clustering window a constant that can be tuned in a follow-on.

---

### SR-09 — Concurrent context_retrospective Lesson-Learned De-duplication
**Severity**: Medium
**Component**: Component 7 (lesson-learned supersede)
**Description**: AC-21 requires that calling `context_retrospective` twice produces exactly one active lesson-learned entry — the second call supersedes the first. The supersede operation requires: (1) find existing entry by topic, (2) deprecate it via `context_correct`, (3) write new entry. Under concurrent calls (two agents both completing a retrospective for the same feature_cycle simultaneously), both reads could find the same entry as active, both attempt supersede, and produce a split chain or two active entries. This is unlikely in practice but represents a correctness gap.
**Mitigation**: Use a write transaction that checks for existing active lesson-learned entry and supersedes atomically. Or accept the race condition as a tolerated edge case (concurrent retrospective calls for the same cycle are rare) and note it in the specification as a known limitation.

---

### SR-10 — Vision Document Discrepancy: session_id on EntryRecord
**Severity**: Low
**Component**: Scope definition
**Description**: The PRODUCT-VISION.md col-010 summary states "adds `session_id: Option<String>` field on `EntryRecord`." The SCOPE.md explicitly lists this as a Non-Goal: "would require a full scan-and-rewrite migration (bincode is positional). The benefit... is low priority. Deferred to a future feature." These are directly contradictory. This creates confusion for agents reading the vision document as authoritative scope.
**Mitigation**: Update PRODUCT-VISION.md col-010 row to remove the `session_id` field reference after the SCOPE.md is approved. No implementation impact, but reduces agent confusion.

---

### SR-11 — Auto-Outcome Entry Bypasses MCP Validation Layer
**Severity**: Low
**Component**: Component 4 (auto-outcomes)
**Description**: SCOPE.md explicitly notes: "bypasses the MCP validation layer." Auto-outcome entries are written via `store.insert_entry()` directly without going through the input validation, content scanning, or category allowlist enforcement that `context_store` applies. If the session data contains unexpected characters in `session_id` or `agent_role` fields (e.g., from adversarial agent naming), these propagate unchecked into ENTRIES. The trust hierarchy mitigates this (entries are tagged `source = "hook"` which is internal), but the validation bypass creates a structural inconsistency.
**Mitigation**: Apply at minimum the category allowlist check and tag validation before writing auto-outcome entries. Input sanitization for `session_id` (restrict to alphanumeric + `-_`) should be applied at the `SessionRecord` write point, upstream of auto-outcome generation.

---

### SR-12 — Counter Contention on INJECTION_LOG Under Concurrent Sessions
**Severity**: Low
**Component**: Component 1 (injection_log.rs)
**Description**: Each `InjectionLogRecord` insert increments the `next_log_id` counter in a write transaction. Under concurrent multi-agent sessions, multiple `ContextSearch` injections can arrive within the same request window. Each write transaction on the redb COUNTERS table serializes — redb uses a single writer model. At high injection rates (e.g., 5 concurrent agents each injecting 3 entries per prompt), contention on the counter write can produce measurable latency. The SCOPE.md notes `spawn_blocking` for all writes, which mitigates tokio blocking, but does not address redb write serialization.
**Mitigation**: Batch INJECTION_LOG writes per ContextSearch response (one transaction for all N injected entries) rather than one transaction per entry. This reduces counter increment operations from N per response to 1 per response.

---

### SR-13 — trust_source = "system" Retroactive Scoring Change
**Severity**: Low
**Component**: Component 4 (auto-outcomes), Component 7 (lesson-learned)
**Description**: SCOPE.md characterizes `trust_source = "system"` as "a correctness fix, not a boost mechanism." However, setting this on newly written entries (auto-outcomes, lesson-learned) does not change existing entries. If prior entries written by cortical implant hooks without an explicit trust_source fall into the `_ => 0.3` arm of `trust_score()`, they will score lower than new system-written entries. This creates a scoring inconsistency between historical and new system-generated knowledge. Not a data corruption risk, but may require a follow-on confidence sweep to normalize historical entries.
**Mitigation**: Note in the implementation brief that historical entries written without `trust_source = "system"` (if any exist) may need a one-time migration or confidence refresh. Check AUDIT_LOG for existing `source = "hook"` entries.

---

### SR-14 — Lesson-Learned Category Empty: Supersede Chain Readiness
**Severity**: Low
**Component**: Component 7 (lesson-learned)
**Description**: The MEMORY.md confirms: "lesson-learned" category currently has 0 active entries. The `context_correct` supersede path has been used extensively (48 correction chains), but `lesson-learned` entries have never been superseded before. The AC-21 de-duplication logic depends on the supersede chain working correctly for this category. No explicit test exists for superseding a `lesson-learned` entry. The category allowlist must also include `lesson-learned` — per MEMORY.md, the initial allowlist covers: outcome, lesson-learned, decision, convention, pattern, procedure. This appears correct, but should be verified.
**Mitigation**: Include a specific AC-21 integration test that creates a lesson-learned entry, runs retrospective again, and verifies exactly one active entry with correct `superseded_by` chain. Verify `lesson-learned` is in the active category allowlist before writing the first entry.

---

## Top 3 Risks for Architect Attention

| Priority | Risk | Why It Matters |
|----------|------|----------------|
| 1 | **SR-01** — col-009 Hard Dependency (Critical) | col-010 cannot start without col-009 merged and all ACs passing. The SessionClose handler design depends on `SignalOutput.final_outcome`. Gate check required before scoping implementation work. |
| 2 | **SR-02** — Feature Bundle Delivery Risk (High) | 7 components + 24 ACs in one feature. Components 6–7 (tiered output, lesson-learned) are independent from the core session persistence work needed by col-011. Consider explicit P0/P1 split in the implementation brief to protect the critical path. |
| 3 | **SR-04** — INJECTION_LOG Orphan Records on GC (Medium) | The GC design deletes sessions but does not cascade to INJECTION_LOG. This is a data integrity gap that grows over time and degrades `from_structured_events()` scan performance. Must be addressed in the storage specification before implementation. |
