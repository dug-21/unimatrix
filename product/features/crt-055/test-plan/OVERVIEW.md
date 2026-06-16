# crt-055 Test Strategy — OVERVIEW

**Feature**: context_cycle_review redesign — durable per-cycle aggregates + dual reload + transcript-fold surfacing (consumer of the crt-054 producer pair)
**Stage**: 3a test plan design | **Author**: crt-055-agent-2-testplan | **Date**: 2026-06-16
**Inputs**: RISK-TEST-STRATEGY.md (R-01..R-18), ACCEPTANCE-MAP.md (AC-01..AC-22), ARCHITECTURE.md (§3 pipeline, §6 surface, §9 producer reconciliation), SPECIFICATION.md (FR/NFR), IMPLEMENTATION-BRIEF.md.

> crt-055 is the **consumer half** of a producer/consumer pair. The dominant failure family is not new-code bugs — it is *re-introduction* of fixed classes (#750 empty-clobber, believable-zero) and *silent miscount* at the cross-feature seam (held-route fold, declaration-chain attribution, read-before-purge ordering, clock/unit gate). Every dishonest number erodes the self-learning substrate this feature exists to make trustworthy. The test strategy is therefore weighted toward **negative / inversion / regression-guard** assertions, not happy-path coverage.

---

## 1. Test Layers

| Layer | Where | Purpose |
|-------|-------|---------|
| **Unit** | `unimatrix-store` (`cycle_review_index.rs`, `migration.rs`, `db.rs`), `unimatrix-observe` (`session_metrics.rs`, aggregate module) | Pure reckoning, width conversion, basis-points encoding, gate comparator, migration pragma checks, single-writer structural assertions. `#[tokio::test]` for async store calls. |
| **Integration (Rust, in-process)** | `unimatrix-store` / `unimatrix-observe` test modules against an in-process store + seeded `cycle_events` / `query_log` / `injection_log` / `compaction_events` / `observations` | Cross-table reckoning where a unit boundary cannot reproduce the failure: rank-1 timeline, rank-3 union, attribution chain, read-before-purge ordering + inversion, the three #5022 returns. |
| **Integration (MCP harness, infra-001)** | `product/test/infra-001/suites/test_lifecycle.py`, `test_tools.py` | End-to-end through the `context_cycle_review` MCP handler against the compiled binary. **AC-22 clock/unit, AC-08 read-before-purge, AC-09 held-route regression, AC-15 auto_close, AC-17 no-clobber** are validated here because their failure modes are only observable through the full pipeline + real SQLite + real timestamp units. |
| **Feature-level** | RISK-COVERAGE-REPORT mapping (Stage 3c) | Every R-XX risk → at least one named test, every AC-XX → PASS/FAIL evidence. |

**Naming**: `test_{component_or_concept}_{scenario}_{expected}` (Rust) / `test_{tool_or_concept}_{behavior}` (pytest). Arrange/Act/Assert. No flaky / wall-clock-dependent tests — all timestamps seeded explicitly.

---

## 2. Risk → Test Mapping (RISK-TEST-STRATEGY.md → test plan)

| Risk | Pri | Primary AC | Test plan file | Layer | Key assertion (load-bearing) |
|------|-----|-----------|----------------|-------|------------------------------|
| R-01 second writer / empty-clobber | Crit | AC-17 | store_cycle_review, review_pipeline | unit + int | Exactly ONE writer site; memo-hit / purged-retain / force+purged do NOT write. The three #5022 assertions. |
| R-02 stale-version no-flush | Crit | AC-18, AC-03 | store_cycle_review, review_pipeline | int | stale+present recomputes to fresh non-zero at v5; version-equality test alone insufficient. |
| R-03 read-after-purge zeroes fold | Crit | AC-08 | activity_fold_landing, review_pipeline | int (MCP + Rust) | Read-site strictly precedes purge; **inversion test zeroes columns** (proves load-bearing). |
| R-04 held-route believable-zero | Crit | AC-09 | activity_fold_landing, fail_loud_guard | int (MCP) | Representative held TS-client cycle → fold non-zero (silent-zero regression guard). |
| R-05 attribution silent no-op (#4140) | Crit | AC-11 | compaction_reckoning | int | Declared-only counts; evicted/undeclared → honest partial, never fabricated zero. |
| R-06 believable-zero past guard | Crit | AC-01, AC-21 | fail_loud_guard | unit + int | Per-metric "unavailable" not "0"; behavioral signals render coarse/directional qualifier. |
| R-07 dual reload collapsed | High | AC-13 | reload_overlap_engine | unit + int | Two columns, two gates, one primitive; neither derived from other's window. |
| R-08 gate clock/unit mismatch | Crit | **AC-22**, AC-12 | compaction_reckoning | **int (MCP, mandated)** | Seed exact-boundary / −500ms / +1s: +1s (floors `T+1`) counts; exact-boundary (floor `T`, strict `>`) and −500ms (floor `T−1`) do not; injected millis mismatch caught (seconds-vs-seconds). |
| R-09 integer-width corruption | Med | AC-14, AC-20 | store_cycle_review, activity_fold_landing | unit | Checked/saturating u64/u32→i64; basis-points clamp 0–10000. No float bind (designed out). |
| R-10 three-path migration drift | High | AC-02, AC-03 | cycle_review_index_schema | unit | pragma on fresh+upgraded DB agree; idempotent; pinned-version test moves in same change. |
| R-11 structural leak gate | High | AC-19 | store_cycle_review, activity_fold_landing | unit | `test_candidates_structurally_absent…` holds; no content field; metadata-only surfaces. |
| R-12 producer-contract drift | Med | (AC-07) | activity_fold_landing, compaction_reckoning | unit | Reads `class_counts[0]=error`,`[1]=refusal` by pinned index; merge boundary check. |
| R-13 token-field re-intro | Med | AC-10 | store_cycle_review | grep/struct | No token-named field/column; no `reread`/`compaction` regex class. |
| R-14 auto_close ordering/dup | Med | AC-15 | auto_close | int (MCP) | Stop written at TOP before rank-1; idempotent; via event writer not second review writer. |
| R-15 rank-1 timeline mis-reckon | High | AC-04, AC-05 | aggregate_reckoning | unit + int | Close-then-reopen = rework not new phase; #556 never-closed true/false positive. |
| R-16 rank-3 union under/over-count | High | AC-06 | aggregate_reckoning | int | reuse == size of UNION(query_log ∪ injection_log); both-logs entry counts once. |
| R-17 pre-divided ratio | Med | (AC-05) | aggregate_reckoning, fail_loud_guard | unit | num/den PAIRS stored; "0 of 0"→unavailable, "0 of N"→measured. |
| R-18 migration version handshake | Med | (AC-03) | cycle_review_index_schema | int + SM | crt-054/crt-055 distinct sequential CURRENT_SCHEMA_VERSION; both apply either order. |

Also covered: **AC-07** (fold landing → columns) → activity_fold_landing; **AC-16** (#206-4 knowledge-that-helped, response-time, no column) → review_pipeline.

---

## 3. Cross-Component Test Dependencies

- **review_pipeline** is the integration spine — it orders auto_close → read-before-purge → aggregate → reload → presence → single persist. Component tests for `auto_close`, `activity_fold_landing`, `aggregate_reckoning`, `compaction_reckoning`, `reload_overlap_engine` validate units; **review_pipeline** validates their *ordering and single-persist composition* (R-01, R-03, R-14 are ordering risks, untestable in isolation).
- **store_cycle_review** depends on **cycle_review_index_schema** (columns must exist before binds can be asserted).
- **compaction_reckoning** depends on the `compaction_events` read accessor (schema) AND the seconds-normalization in **reload_overlap_engine**/aggregate site — AC-22 spans both.
- **fail_loud_guard** consumes the per-metric `available` flags set by the pipeline from every other component's source-presence — its tests must drive each source class independently to zero.

---

## 4. Integration Harness Plan (infra-001)

**Binary**: `cargo build --release` before any harness run. Harness exercises the compiled `unimatrix-server` via MCP JSON-RPC.

### 4.1 Suites this feature touches (per selection table)

| Feature touches | Suites to run (Stage 3c) |
|-----------------|--------------------------|
| `context_cycle_review` tool logic + new `auto_close` param | `tools`, `protocol` |
| Store/retrieval (cycle_review_index columns) | `tools`, `lifecycle`, `edge_cases` |
| Schema / storage changes (v5 migration) | `lifecycle`, `volume` |
| Any change at all | `smoke` (MANDATORY minimum gate) |

**Mandatory gate**: `python -m pytest suites/ -v -m smoke --timeout=60` must pass before/after.
**Targeted run**: `python -m pytest suites/test_lifecycle.py suites/test_tools.py -v --timeout=60` plus `test_edge_cases.py` for boundary values.

### 4.2 Existing coverage to lean on (extend, do not re-scaffold)

The harness already has the exact substrate the mandated integration tests need (CLAUDE.md: test infra is cumulative — extend it):
- `server.context_cycle_review(topic, agent_id, format, timeout)` helper (`harness/client.py:654`, `uds_client.py:553`).
- `server.context_cycle(type, topic, phase, next_phase, agent_id)` for start / phase-end / stop events.
- `_seed_cycle_events_lifecycle(db_path, cycle_id, events)` — direct SQL seed of `cycle_events` rows (phase timeline).
- `_seed_observation_sql_lifecycle(db_path, feature_ids, num_records)` — direct SQL seed of `observations` (PostToolUse reads, `ts_millis`).
- `_compute_db_path_lifecycle(project_dir)` + `sqlite3` direct-connect pattern for seeding `query_log`, `injection_log`, `compaction_events`.
- `_compaction_events_columns(db_path)` — pragma reader for the producer table.
- Precedents: `test_phase_tag_store_cycle_review_flow` (cycle_events → phase narrative), `test_cycle_review_knowledge_reuse_cross_feature_split` (query_log seed → reuse rendering).

### 4.3 New integration tests to ADD in Stage 3c

All additions go into `suites/test_lifecycle.py` (cross-table seed + full-pipeline review) and `suites/test_tools.py` (the `auto_close` param), using the `server` fixture (fresh DB, no leakage) and the existing SQL-seed helpers.

| New test | Suite | AC | What it seeds / asserts |
|----------|-------|----|--------------------------|
| `test_cycle_review_compaction_reread_seconds_boundary` | lifecycle | **AC-22** | Seed `compaction_events.compacted_at = T` (seconds) and `observations.ts_millis` at `T*1000` (exact boundary), `T*1000 − 500` (−500ms), `T*1000 + 1000` (+1s). Gate: `(ts_millis ÷ 1000) > compacted_at` (integer floor, strict `>`). Exact boundary floors `T` (strict `>` → not counted); −500ms floors `T−1` (floor-catching guard → not counted); +1s floors `T+1` (→ counts). Assert `compaction_reread_count == 1`. **Sub-second boundary exercises the ÷1000 floor** — a ±1s window would pass even with a broken floor. |
| `test_cycle_review_compaction_reread_unit_mismatch_guarded` | lifecycle | **AC-22** | Inject an unnormalized millis `ts` against a seconds `compacted_at`; assert the gate does NOT flip to all-or-nothing (every-read or zero-read) — i.e. normalization-to-seconds holds, cross-table millis×seconds mis-compare prevented. |
| `test_cycle_review_compaction_count_vs_reread` | lifecycle | AC-11, AC-12 | Multi-compaction session (N>1 rows): `compaction_count` reports all rows; `compaction_reread_count` gates on MIN(`compacted_at`), each read counted once. |
| `test_cycle_review_compaction_attribution_declared_only` | lifecycle | AC-11 | Seed `compaction_events` for a declared session and an undeclared/evicted one (#4140); assert only declared rows count; undeclared → no fabricated zero. |
| `test_cycle_review_read_before_purge_columns_nonzero` | lifecycle | AC-08 | Full review with held activity → `transcript_*` columns non-zero (read-before-purge holds end-to-end through the binary). |
| `test_cycle_review_held_route_fold_nonzero` | lifecycle | AC-09 | Representative TS-client cycle with held activity → fold source non-empty, `transcript_bytes_total`/`_delta_count` non-zero (silent-zero regression guard). |
| `test_cycle_review_fold_lands_into_columns` | lifecycle | AC-07 | Known fold → `transcript_error_count`==class[0], `_refusal_count`==class[1], `signal_class_counts_json` matches catalog. |
| `test_cycle_review_empty_source_renders_unavailable` | lifecycle | AC-01 | Cycle with each empty source class → rendered report shows "unavailable", never literal "0", per metric. |
| `test_cycle_review_behavioral_signals_directional_qualifier` | lifecycle | AC-21 | Rendered report carries coarse/directional qualifier on `transcript_error/refusal`; compaction_count / rework ratio do NOT — presentations distinguishable. |
| `test_cycle_review_auto_close_writes_stop_before_pipeline` | tools | AC-15 | `auto_close=true`, no prior stop → stop written before rank-1, final phase not false-never-closed. |
| `test_cycle_review_auto_close_idempotent_when_stop_exists` | tools | AC-15 | `auto_close=true`, stop exists → no duplicate. |
| `test_cycle_review_auto_close_false_open_phase_never_closed` | tools | AC-15, AC-04 | `auto_close=false`, open final phase → honest never-closed, not error. |
| `test_cycle_review_knowledge_reuse_union_dedup` | lifecycle | AC-06 | Extend the col-026 precedent: entry served via BOTH query_log and injection_log → counted once; count == union size. |
| `test_cycle_review_stale_present_recomputes` / `_purged_retains` | lifecycle | AC-17, AC-18 | Stale pre-v5 row + present source → fresh non-zero at v5; + purged → byte-identical retain, no write. (End-to-end #5022 a/b/c.) |
| `test_cycle_review_index_v5_columns_present` | lifecycle | AC-02, AC-03 | Extend `_compaction_events_columns` pragma pattern to `cycle_review_index`: every v5 column present with type/default on fresh + restarted (upgraded) DB. |

### 4.4 NOT new integration tests (unit suffices / out of scope)

- Basis-points rounding arithmetic (AC-20) — pure unit on the encode fn; only the *column type INTEGER not REAL* check needs the pragma (covered by `test_cycle_review_index_v5_columns_present`).
- Checked/saturating width conversion (AC-14) — pure unit at the persist boundary.
- Single-writer structural assertion (AC-17) — Rust static/structural test (one call site); the *behavioral* three returns are integration.
- Migration version handshake with crt-054 (R-18, AC-03) — SM merge-coordination check, not a harness test. No harness *infrastructure* change required → no GH Issue.

### 4.5 Integration-level scenarios to validate (end-to-end semantics)

1. **Clock/unit gate (AC-22, the marquee mandate)** — sub-second boundary across `compaction_events` × PostToolUse reads, both unit-normalized to seconds.
2. **Read-before-purge ordering (AC-08)** — columns non-zero end-to-end; the *inversion* (zeroes columns) is asserted at the Rust integration layer where the call order is directly manipulable.
3. **No-clobber across the four returns (AC-17/18)** — full handler, real SQLite, real memo state.
4. **Attribution chain (AC-11)** — declared vs evicted sessions through the real declaration chain.

---

## 5. Self-Check (Stage 3a)

- [x] Risk → test scenario mapping from RISK-TEST-STRATEGY.md (§2)
- [x] Integration harness plan — suites to run, existing coverage, new tests, NOT-new rationale (§4)
- [x] Per-component test plans match the 9 architecture component boundaries (separate files)
- [x] Every Critical/High risk has ≥1 specific test expectation
- [x] Integration tests defined at component boundaries (review_pipeline ordering, compaction seam, attribution)
- [x] All output under `product/features/crt-055/test-plan/`

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search`(category=decision, topic=crt-055) + `context_get`(4236, 5022) — surfaced ADR-001/002/009 (#5051/#5037/#5044), the #5022 three-assertion empty-clobber lesson, and #4236 (epoch-migration three-tier boundary test pattern: named-constant + boundary-insertion + multi-window) which directly shapes the AC-22 ÷1000-floor sub-second boundary design.
- Stored: deferred to Stage 3c / retro — no novel test infrastructure discovered at plan time; the harness SQL-seed + `context_cycle_review` helper pattern is already established (`test_lifecycle.py`), and #4236 already captures the epoch-boundary test pattern AC-22 reuses.
