# crt-054 Test Plan — OVERVIEW

**Date**: 2026-06-16 — Stage 3a (test plan design, producer-only re-scope).
**Roots**: `RISK-TEST-STRATEGY.md` (R-01..R-15), `ACCEPTANCE-MAP.md` (AC-01..AC-16), `architecture/ARCHITECTURE.md`, `specification/SPECIFICATION.md`, `crt-055/SCOPE.md` §"Producer contract" (binding).
**Component test plans** map 1:1 to the brief's Component Map.

> crt-054 is the **producer half**. It writes two surfaces — Surface A (durable `compaction_events` table) and Surface B (in-memory `activity_snapshot()` fold) — and persists nothing of Surface B. The test plan's center of gravity is the **believable-zero family** (R-01/AC-06 routing, R-02/AC-07 sequencing): these are Critical and MUST be exercised as held-route, drain→hold→review **integration** tests on the cumulative crt-052/vnc-025 fixtures. A registered-only or unit-only test does NOT satisfy AC-06/AC-07 (pattern #3624 — a no-op-path test gives false confidence; #750/#5025 is exactly the class that slips through such a test).

---

## 1. Overall Test Strategy

Three layers, each with a defined boundary:

| Layer | What it proves | Where it lives |
|-------|----------------|----------------|
| **Unit** | Pure fold arithmetic, scanner multi-class, config `validate()`, snapshot shape/Copy/content-opacity, migration DDL on fresh + upgraded DB, cast-free producer widths. | `crates/unimatrix-server/src/infra/*_tests.rs`, `crates/unimatrix-store/tests/migration_v28_to_vNN.rs`, `sqlite_parity.rs` |
| **Integration (in-crate)** | Held-route fold continuity across drain→hold→re-adopt; read-before-purge ordering; Surface A INSERT at `handle_compact_payload` under lock contention; named failure counter on forced INSERT failure; undeclared-session row + no-fabricated-zero. | `crates/unimatrix-server/src/infra/`, `crates/unimatrix-server/src/uds/listener/tests/` (alongside `transcript_hold_tests.rs`, `purge_audit.rs`) |
| **Integration (infra-001, MCP)** | System-level behaviors visible through the JSON-RPC seam: schema migration / restart persistence of `compaction_events`; smoke gate; cross-gate seconds-boundary (AC-16, co-owned with crt-055). | `product/test/infra-001/suites/` |

**Critical-risk discipline.** AC-06 and AC-07 are NOT unit tests. They are in-crate integration tests that drive the real `SessionRegistry` + crt-052 Wave B hold + the `apply_delta` fold seam, on the cumulative crt-052/vnc-025 transcript fixtures (`infra/transcript_hold_tests.rs`, `session_transcript_tests.rs`). Each carries a **mandatory negative-mutation check**: if the held-route `apply_delta` fold call (or the survival ordering) is removed, the test must fail red. A test that stays green under that mutation is invalid and must be rejected at Gate 3a/3c.

---

## 2. Risk → Test Mapping

| Risk | Priority | Anchor AC | Test(s) — name + layer | Component plan |
|------|----------|-----------|------------------------|----------------|
| R-01 Held-route believable-zero | **Critical** | AC-06 | `test_held_route_fold_nonempty_at_review` (integration), `test_held_route_fold_continuity_across_drain` (integration), `test_held_route_fold_negative_mutation_guard` (integration, mutation) | apply-delta-fold.md, activity-collector.md |
| R-02 Read-after-purge / fold dropped | **Critical** | AC-07 | `test_read_before_purge_ordering` (integration), `test_no_crt054_path_zeroes_accumulator` (integration), `test_snapshot_survives_drain_hold_review` (integration) | activity-snapshot.md, apply-delta-fold.md |
| R-03 Lock-graph deadlock/stall at INSERT seam | **Critical** | AC-04 | `test_compaction_insert_under_lock_contention_no_deadlock` (integration), `test_high_water_guard_dropped_before_insert` (integration/review) | compaction-events-writer.md |
| R-04 Schema-version sequencing collision | **Critical** | AC-01 | `test_compaction_events_fresh_create` (unit), `test_migration_v28_to_vNN_adds_compaction_events` (unit), `test_schema_version_is_NN` + `test_schema_column_count` (sqlite_parity), merge-order grep (SM gate) | compaction-events-migration.md |
| R-05 Producer-contract drift | High | AC-08, AC-10 | `test_activity_snapshot_shape_matches_contract` (unit), `test_compaction_events_columns_match_contract` (unit) | activity-snapshot.md, compaction-events-migration.md |
| R-06 Single-event/per-turn-drain dependence | High | AC-13 | `test_no_pretooluse_or_single_hook_dependence` (grep/structural) + covered transitively by AC-06 | apply-delta-fold.md |
| R-07 Producer→consumer width truncation | Medium | AC-14 | `test_producer_path_has_no_narrowing_cast` (grep), `test_bytes_total_near_u64_max_round_trips` (unit) | activity-snapshot.md |
| R-08 Late-bind attribution fabricates a zero | Medium | AC-03, AC-12 | `test_undeclared_session_no_activity_entry` (integration), `test_compaction_row_written_for_undeclared_session` (integration) | activity-collector.md, compaction-events-writer.md |
| R-09 Surface A row written under buffer lock | Medium | AC-04 | folded into `test_high_water_guard_dropped_before_insert` | compaction-events-writer.md |
| R-10 `[transcript_signals]` silent fallback | Medium | AC-11, AC-10 | `test_config_over_cap_rejected`, `test_config_invalid_regex_rejected`, `test_default_config_error_refusal_only` (unit) | transcript-signals-config.md |
| R-11 `compacted_at` granularity / cross-gate | Medium | AC-02, AC-01a, AC-16 | `test_compacted_at_is_seconds_within_tolerance` (integration), `test_second_compaction_adds_monotonic_row` (integration), `test_compaction_events_seconds_boundary` (infra-001, AC-16 producer half) | compaction-events-writer.md |
| R-12 Stale-scope residue re-imported | Medium | AC-15 | `test_diff_no_cycle_review_index_no_summary_schema_version` (grep), `test_no_token_or_reread_symbol` (grep), `test_snapshot_has_no_latch_field` (unit) | activity-snapshot.md, transcript-signals-config.md |
| R-13 `high_water` over-trusted | Low | AC-02 | `test_high_water_equals_buffer_high_water` (integration) + documented semantics | compaction-events-writer.md |
| R-14 Multi-compaction row semantics | Low | AC-02 | `test_second_compaction_adds_monotonic_row` (integration) | compaction-events-writer.md |
| R-15 INSERT-failure silent undercount | High | AC-04a | `test_insert_failure_increments_named_counter` (integration, fault-injection), `test_insert_failure_non_blocking_no_panic`, `test_insert_failure_counter_is_content_free` | compaction-insert-helper.md, compaction-events-writer.md |

Every R-01..R-15 has ≥1 named test. Every AC-01..AC-16 is covered (see §6 AC coverage table). Critical risks each get the held-route/sequencing/contention/migration integration coverage the strategy mandates.

---

## 3. Cross-Component Test Dependencies

- **Held-route family (AC-06/AC-07)** depends on the cumulative **crt-052 Wave B hold fixtures** (`infra/transcript_hold_tests.rs`, `infra/transcript_hold_ac11_tests.rs`) and the existing `apply_delta`/`SessionRegistry` test harness (`session_transcript_tests.rs`, `session.rs`). Extend these — do not build isolated scaffolding (CLAUDE.md: test infra is cumulative).
- **`activity_snapshots_for_feature` collector** (activity-collector.md) mirrors `take_transcripts_for_feature` and is exercised by the same drain→hold fixture the held-route fold uses; the read-before-purge test (activity-snapshot.md) layers `purge_cycle_transcripts` (`uds/listener/tests/purge_audit.rs` precedent) on top of it.
- **Surface A writer** (compaction-events-writer.md) depends on the **migration** (compaction-events-migration.md) and the **INSERT helper + failure counter** (compaction-insert-helper.md): the writer test cannot land a row until the table exists, and the fault-injection test drives the helper's error path.
- **Snapshot shape** (activity-snapshot.md) and **config catalog** (transcript-signals-config.md) jointly verify the producer contract: `class_counts[0]=error`, `class_counts[1]=refusal`, `MAX_SIGNAL_CLASSES == 16` — these MUST equal crt-055's constants (single source = crt-055 §"Producer contract").

---

## 4. Integration Harness Plan (infra-001)

### 4.1 Suite selection

Per the suite-selection table, crt-054 touches **server tool logic + store/retrieval behavior + schema/storage changes**:

| Suite | Why it applies to crt-054 | Action |
|-------|---------------------------|--------|
| `smoke` | Any change at all — **mandatory minimum gate** (Stage 3c). | Run as-is. Must pass. |
| `protocol` | Server binary change (new table, config block, startup precondition); confirm handshake/discovery/shutdown unaffected. | Run as-is (regression). |
| `tools` | crt-054 modifies `handle_compact_payload` and adds a config block; confirm no tool surface regressed. | Run as-is (regression). |
| `lifecycle` | **Schema/storage change** — restart-persistence is the key concern: a `compaction_events` row written, server restarted, table + row survive (migration durability). | Run as-is; **add new test** (below). |
| `edge_cases` | Empty-DB operations, boundary values — confirm `compaction_events` migration is clean on an empty DB and the server starts. | Run as-is (regression). |
| `volume` | Schema change at scale — confirm migration of a populated DB is sound. | Run as-is (regression). |

Suites NOT required (no feature-relevant behavior): `confidence`, `contradiction`, `security` (no new content-bearing MCP surface — content-opacity is enforced structurally in-crate via AC-08, not at the MCP boundary; Surface A `session_id` uses the store's parameterized INSERT). Run them only as a full-suite pre-merge regression, not as a targeted gate.

### 4.2 Existing-suite coverage vs gaps

Most of crt-054's behavior is **in-crate** (the fold, the lock seam, the failure counter, the held route) and is NOT observable through the MCP JSON-RPC interface — those belong in the Rust integration tests (§1, layer 2), not infra-001. The harness validates only the two MCP-visible facets:

1. **Migration / restart persistence** of `compaction_events` — no existing suite covers this new table.
2. **Cross-gate seconds boundary (AC-16)** — no existing suite covers compaction-event timestamp semantics.

### 4.3 New integration tests to add (Stage 3c)

| New test | Suite / file | Validates | Risk/AC |
|----------|--------------|-----------|---------|
| `test_compaction_events_table_survives_restart` | `suites/test_lifecycle.py` | After a server restart the `compaction_events` table exists (migration durable across restart) — restart-persistence gate for the new schema. Fixture: `shared_server` or restart pattern used by existing persistence tests. | R-04 / AC-01 |
| `test_compaction_events_seconds_boundary` | `suites/test_lifecycle.py` (or a new `test_compaction.py` if a discrete file is cleaner) | **AC-16 producer half** — see §5. crt-054 lands the assertion that the persisted `compacted_at` is in Unix **seconds** at the boundary; references crt-055 for the `ts/1000` normalization + `read_ts_secs > compacted_at` half. | R-11 / AC-16 |

These follow the harness conventions: `test_{concept}_{behavior}` naming; default `server` fixture except where restart/state accumulation needs `shared_server`. No new harness infrastructure (no GH Issue needed). If the AC-16 boundary test requires harness changes neither feature should own (e.g. injecting a synthetic PostToolUse `ts`), file a GH Issue per USAGE-PROTOCOL rather than overload the PR — flagged as an open question (§7).

### 4.4 Failure triage

Per USAGE-PROTOCOL: a failure in code crt-054 changed → fix it. A pre-existing/unrelated failure → file a GH Issue, mark `@pytest.mark.xfail(reason="Pre-existing: GH#NNN — …")`, continue. A bad assertion → fix the test. **Never** fix an unrelated integration failure in this PR.

---

## 5. AC-16 Ownership Split (Coordination Item 1 — RESOLVED in this plan)

AC-16 is the cross-gate seconds-boundary integration test, **co-owned with crt-055**. To ensure it is neither dropped nor duplicated, ownership is split by which half each feature can authoritatively assert:

- **crt-054 PHYSICALLY LANDS the seconds-PRODUCER half.** Its test (`test_compaction_events_seconds_boundary`, infra-001 / `test_compaction_events.compacted_at` in-crate assertion at `handle_compact_payload`) asserts: a compaction at a known wall-clock instant writes a `compaction_events` row whose `compacted_at` is in Unix **seconds** (`.as_secs()` / `now_secs()`), within tolerance of `now`, and monotonic across repeat compactions. This is the producer guarantee crt-055's gate rests on. It is landed as an **integration** test (driven through the compaction seam), NOT a unit-only test.
- **crt-054 REFERENCES crt-055 for the normalization half.** crt-055 owns the `ts/1000` (epoch millis → seconds) normalization and the `read_ts_secs > compacted_at` pre/post-compaction classification (crt-055 Binding constraint 8). crt-054 does NOT land that half; its test plan documents the reference so the consumer side is provably present somewhere.
- **Neither side lands its half as a unit-only test.** crt-054's seconds-producer assertion is integration-level; crt-055's normalization+gate assertion is integration-level. The full end-to-end pre/post-boundary classification (one read just after the boundary = post-compaction, one just before = pre-compaction) is the consumer-side (crt-055) test that consumes crt-054's seconds rows.

**Net:** crt-054 owns and lands "rows are written in Unix SECONDS at the boundary"; crt-055 owns and lands "normalize the read `ts` to seconds and classify against `compacted_at`." The SM confirms this split at the producer/consumer test-plan handoff; recorded here and in `compaction-events-writer.md`.

---

## 6. AC Coverage Table (forward map)

| AC | Covered by (component plan → test) |
|----|-------------------------------------|
| AC-01 | compaction-events-migration.md → fresh-create + v28→vNN upgrade + sqlite_parity |
| AC-01a | compaction-events-migration.md → "Unix SECONDS" DDL-comment grep (both paths) |
| AC-02 | compaction-events-writer.md → one-row, seconds-tolerance, high_water, second-row-monotonic |
| AC-03 | compaction-events-writer.md → undeclared-session row written, no feature_cycle/content column |
| AC-04 | compaction-events-writer.md → lock-contention no-deadlock + high_water-guard-dropped review |
| AC-04a | compaction-insert-helper.md → fault-injection named counter + non-blocking + content-free |
| AC-05 | apply-delta-fold.md → registered-route fold counters advance |
| AC-06 | apply-delta-fold.md / activity-collector.md → **held-route nonempty + continuity + negative-mutation** |
| AC-07 | activity-snapshot.md → **read-before-purge ordering + survival** |
| AC-08 | activity-snapshot.md → Copy struct shape, content-opacity, metadata-only Debug, no Display |
| AC-09 | transcript-activity.md → one shared scan per delta, multi-class increment |
| AC-10 | transcript-signals-config.md → default = error(0)/refusal(1) only, no SDLC/reread/compaction |
| AC-10a | transcript-signals-config.md → delivery-time calibration check (manual, recorded in artifact) |
| AC-11 | transcript-signals-config.md → `MAX_SIGNAL_CLASSES == 16`, over-cap + invalid-regex rejected loud |
| AC-12 | activity-collector.md → undeclared session contributes no entry, no fabricated zero |
| AC-13 | apply-delta-fold.md → no PreToolUse/single-hook dependence (grep) + transitive AC-06 |
| AC-14 | activity-snapshot.md → producer cast-free grep + near-u64::MAX round-trip |
| AC-15 | transcript-signals-config.md / activity-snapshot.md → no cycle_review_index / SUMMARY_SCHEMA_VERSION / token_* / reread-compaction (grep) |
| AC-16 | compaction-events-writer.md (producer half, §5) + crt-055 reference (consumer half) |

---

## 7. Open Questions

1. **AC-16 harness injection mechanism.** Driving a PostToolUse read with a controlled `ts` just-before/just-after the compaction boundary through the infra-001 MCP seam may need a harness hook to set or observe read timestamps. If crt-054's seconds-producer assertion can be landed purely on the `compaction_events` row (assert `compacted_at` is seconds within tolerance), no new harness infra is needed; the full pre/post classification is crt-055's consumer test. Confirm with the SM at the handoff whether crt-054's half stays at the row-assertion level (preferred — no harness change) or needs a shared hook (→ GH Issue, not in this PR).
2. **Schema version 29 vs 30.** The migration test file name (`migration_v28_to_v29.rs` vs `_v30.rs`) and the pinned-version assertions depend on merge order with crt-055 (Coordination Item 2). The test plan is written version-agnostic as `vNN`; the actual number is reconciled by the SM at merge (grep `CURRENT_SCHEMA_VERSION` before finalizing — lesson #4095). Stage 3c must update the file name + `read_schema_version >= NN` assertions to the assigned number.
3. **`high_water` value assertion** (R-13/AC-02) requires a session whose buffer has `high_water > 0` at compaction; the fixture must send non-trivial bytes before compaction so the assertion is non-trivial (not just the DEFAULT 0).

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` — surfaced ADR-009 (#5033 held-route believable-zero guard, non-empty held source + negative-mutation), ADR-006 (#5031 survival-to-review / never-zero-before-purge), the schema-version cascade checklist (#4373), pattern #3624 (no-op-path false confidence), lesson #4998/#5025/#750 (the originating believable zero), #4799 (per-turn drain → held route exists). All load-bearing for the AC-06/AC-07 integration-test discipline.
- Stored: nothing novel to store at plan-design time. The patterns this plan leans on (#3624 no-op-path guard, #4373 schema cascade, #4095 merge-order collision) are already captured; a test-infra pattern (if any) emerges at Stage 3c execution, not at plan design. Will revisit storing a held-route-fold integration-test fixture pattern in the 3c report if the negative-mutation harness proves reusable.
