# crt-052 Test Strategy + Integration Plan

GH Issue: #689. Source: RISK-TEST-STRATEGY.md (R-01..R-20), ACCEPTANCE-MAP.md (AC-01..AC-13,
AC-V-SEAM, AC-V-FUZZ), IMPLEMENTATION-BRIEF.md (6 merge gates, Wave A/B boundary, ADR-001..009).
This is the binding test contract for Stage 3c. Per-component plans map 1:1 to the C1..C10
Component Map; this file owns the cross-component strategy, the risk→test and AC→test maps, the
integration-harness plan, and the merge-gate coverage.

## Test Layers

| Layer | Where | Scope |
|-------|-------|-------|
| Unit (pure) | `#[cfg(test)]` in `unimatrix-observe/src/distill/*` | jsonl parse, markers, select, reconstruct — over committed fixtures, no I/O, no locks |
| Unit (server) | `#[cfg(test)]` in `infra/session_transcript*.rs`, `infra/transcript_hold*.rs`, `infra/config.rs`, `mcp/distill_handler*.rs`, `server.rs` | snapshot primitive, seam, held store, retention gate, helper glue |
| Component integration (in-crate) | `infra/*_tests.rs`, handler tests | seam↔snapshot, helper↔four-returns, seam↔held-store↔purge, snapshot↔fallback↔loss |
| Feature lifecycle | `mcp/` server-level test driving `context_cycle_review` | `continuity_simulated_lifecycle` (AC-11), four-return wiring, no-leak, cache-hit |
| MCP integration (infra-001) | `product/test/infra-001/suites/` | the compiled binary through JSON-RPC — see Integration Harness Plan |
| Static gate | grep/source-assertion tests + `cargo audit` | single-reader, no-parse-under-lock, content-leak, Debug, dependency-direction |

**Conventions** (cumulative — extend, never re-scaffold): Rust unit tests live in `#[cfg(test)] mod
tests` (synthesis.rs:215 style) or a sibling `*_tests.rs` with `#[path]` include (the
session_transcript 500-line split). Delta sets are `(offset,len)` pairs sliced from `src_bytes`;
expected content derived programmatically from the covered-range union (#2984), never hand-copied —
reuse `apply_all`/`covered_union`. Names: `test_{unit}_{scenario}_{expected}`. Async server tests:
`#[tokio::test]`. Concurrency: stress loop (or loom where available). No flaky tests — pin every
clock, seed, and truncation order.

## Risk → Test Mapping

| Risk | Pri | Component plan(s) | Primary test(s) | Merge gate |
|------|-----|-------------------|-----------------|------------|
| R-01 wrong-cycle re-adopt | Crit | held-buffer-store, snapshot-seam | `test_readopt_cycle_match_rebinds`, `test_readopt_cycle_mismatch_fails_loud`, `test_readopt_null_cycle_no_silent_adopt`, AC-11(b) | AC-11 |
| R-02 unbounded held memory | Crit | held-buffer-store, config-knobs | `test_hold_cap_evicts_oldest`, `test_hold_ttl_sweep_without_review`, `test_hold_memory_bounded_no_review`, AC-11(c)(d) | AC-11 |
| R-03 audit not exactly-once | Crit | held-buffer-store, distill-handler | `test_audit_once_at_review`, `_at_sweep`, `_at_eviction`, `_across_multi_readopt`, AC-11(e) | AC-11 |
| R-04 content leak to SQL/log/audit | Crit | response-types, distill-handler, snapshot-types | `test_report_struct_has_no_candidate_field`, `test_rereview_stored_record_no_candidates`, content-leak grep gate, `test_audit_detail_content_free` | Content-leak |
| R-05 unfaithful AC-11 sim | Crit | held-buffer-store | `continuity_simulated_lifecycle` (≥3 drains, inter-drain deltas); negative: single-turn rejected | AC-11 |
| R-06 third buffer reader | High | snapshot-types, snapshot-seam | AC-V-SEAM source assertion; `test_700_reuse_parses_snapshot_bytes_no_contiguous_tail` | AC-V-SEAM |
| R-07 not-all-four-returns | Crit | distill-handler | `test_distill_before_purge_at_return_{2110,2236,2925,3027}`, `test_exhaustiveness_fifth_return_fails` | Four-return |
| R-08 parse/match under lock; torn read | High | snapshot-seam, snapshot-types | AC-01(a) no-parse-under-lock source assertion; `test_concurrent_deltas_during_review_consistent` (stress/loom) | AC-01 |
| R-09 fallback mis-calibrated | High | reconstruct, distill-handler, snapshot-types | `test_fallback_empty_snapshot`, `_elided_above_threshold`, `_holes_fraction_boundary`, cap-edge + overflow boundary | — |
| R-10 parser panic on adversarial | Crit | selection-module, distill-handler | AC-V-FUZZ corpus at module + handler level; `test_jsonl_*_skip_with_count_no_panic` | AC-V-FUZZ |
| R-11 Wave B contaminates Wave A | High | distill-handler, selection-module, reconstruct | `test_wave_a_no_transcript_hold_dependency` (source/dep assertion); Wave-A-only empty-buffer degrade run | — |
| R-12 array-relative byte_offset | High | selection-module, snapshot-types | `test_byte_offset_logical_under_overflow`, `_equals_in_snapshot_when_no_overflow` | — |
| R-13 snapshot/purge scan divergence | High | snapshot-seam, held-buffer-store | `test_registered_and_held_both_snapshotted_and_purged`, `test_no_double_snapshot_arc_identity`, `test_no_held_survives_post_review` | — |
| R-14 topic_source as filter | Low | reconstruct | `test_topic_source_reorders_drops_nothing`, `test_all_vote_still_reconstructs` | — |
| R-15 non-deterministic aggregate cap | Med | distill-handler, config-knobs | `test_cycle_cap_truncation_chronological_keep_earliest_repeatable`, `test_session_and_cycle_caps_independent` | — |
| R-16 silent eviction/poison drop | High | held-buffer-store, snapshot-seam, response-types | `test_eviction_emits_audit`, `test_poison_recovery_surfaces_loss_in_section` | — |
| R-17 delta routing regresses hot path | Med | held-buffer-store | `test_held_lookup_is_o1_keyed_no_scan`, delta-apply lock-class assertion | — |
| R-18 RetainDays distills/purges | Low | retention-gate | `test_retention_match_no_wildcard` (compile), `test_retaindays_rejected_at_validate`, `test_retaindays_helper_returns_none` | — |
| R-19 content-bearing Debug | Med | snapshot-types, held-buffer-store | `test_snapshot_debug_metadata_only`, `test_heldbuffer_debug_metadata_only`, grep no-derive-Debug gate | Content-leak |
| R-20 self-fulfilling fixture | High | selection-module | provenance-header review check; `test_independent_corpus_recall_ge_090`, `test_selected_volume_le_10pct` | — |

## AC → Test Mapping

| AC | Component plan | Verification | Test(s) |
|----|----------------|--------------|---------|
| AC-01 **[gate]** | snapshot-seam, snapshot-types | test + grep | no-parse-under-lock source assertion; two-phase discipline test; concurrency/stress streaming deltas during review |
| AC-02 | selection-module, distill-handler | test | drop tool_use/tool_result/thinking; whole-block; dedup; per-session 24KB + per-cycle aggregate cap; deterministic truncation; ordering+hints populated |
| AC-03 | selection-module | test + manual | independent committed corpus; block-level recall ≥0.90; selected volume ≤10%; provenance header review check |
| AC-04 | response-types, distill-handler | test | serde round-trip field omitted when None; golden-diff no-transcript review byte-identical for existing fields |
| AC-05 **[gate]** | distill-handler | test | per-path distill→purge at all four returns; exhaustiveness regression (fifth return fails); memoization-hit fresh; error-path retained-no-candidates |
| AC-06 **[gate]** | response-types, distill-handler, snapshot-types | test + grep | (a) RetrospectiveReport no candidate field (compile); (b) re-review stored record no candidates; (c) content-leak grep/log/SQL gate; (d) audit detail content-free; (e) metadata-only Debug |
| AC-07 | reconstruct, snapshot-types | test | empty→reconstructed labeled; hole-ridden≥threshold→whole-session fallback; no buffer write + no observation row; topic_source reorders only |
| AC-08 | response-types, distill-handler | test | elided/holed/reconstructed metadata populated; omitted when zero; provenance matches path; per-session + per-cycle cap truncation surfaces dropped-count; eviction/poison surfaces loss |
| AC-09 | distill-handler | test | extend `detection_isolation` with distill active — rule inputs byte-identical; no new path into `insert_observations_batch`; batch filter listener.rs:1238 unchanged |
| AC-10 | retention-gate | test | no-wildcard exhaustive match (compile); RetainDays rejected at validate(); constructed-RetainDays helper returns None |
| AC-11 **[gate]** | held-buffer-store | test | `continuity_simulated_lifecycle` — ≥3 drains + inter-drain deltas; (a) cross-turn content; (b) loud re-adopt/mismatch; (c) held-count cap + eviction; (d) sweep reclaims w/o review; (e) audit exactly-once |
| AC-12 | snapshot-types, selection-module | test | timed pass over 4 MiB fixture <50ms off-lock; cycle-review latency class unchanged |
| AC-13 | consumer-guidance, config-knobs | manual + shell | uni-retro doc checklist (four families, Q8 folds, call-time-vs-cached, feature-attributed context_store); `cargo audit` pass; regex-class-only dependency diff |
| AC-V-SEAM **[gate]** | snapshot-types, snapshot-seam | test + grep | two-production-reader source assertion (fails on third); #700-shaped reuse over `snapshot.bytes`; all four metadata fields exposed |
| AC-V-FUZZ **[gate]** | selection-module, distill-handler | test | malformed/adversarial corpus skip-with-count, no Err, no panic at module + handler level; truncated final line tolerated; bounded nested/gigantic; fully-corrupt snapshot → normal response candidates-absent |

## Cross-Component Test Dependencies

These integration seams are owned across plans; the listed component plan owns the assertion but the
test exercises ≥2 components — place each in the crate that owns the higher boundary.

1. **Seam ↔ held store ↔ purge (R-01/R-03/R-13)** — `take_transcripts_for_feature` scans registered ∪
   held; `purge_held_for_feature` + `clear_transcripts_for_feature` clear the same set; audit fires
   once per terminal purge. Highest-density integration risk. Owned by snapshot-seam, exercised with
   held-buffer-store. Single-source key: `feature_cycle`.
2. **Four-return helper ↔ memoization (R-04/R-07)** — helper attaches candidates at assembly AFTER
   `store_cycle_review()` (synchronous SQL, #3793); memoization-hit (#2925) deserializes the cached
   report (#3800) then distills fresh — the cache-hit candidate-vs-report divergence is tested, not
   assumed. Owned by distill-handler.
3. **Snapshot ↔ delta merge (R-08/R-17)** — deltas stream concurrently with snapshot and held-buffer
   merge; lock discipline (registry lock→Arc clone; buffer lock→byte copy/merge; all parse off-lock,
   #3753) is the boundary. Stress/loom. Owned by snapshot-seam + held-buffer-store.
4. **Snapshot metadata ↔ fallback trigger ↔ loss visibility (R-09/R-12/R-16)** — one `snapshot()`'s
   `elided_bytes`/`holes`/`base_offset` feed BOTH the fallback predicate (ADR-006) AND the loss
   section (ADR-007); the SAME predicate must drive both (no re-computation). Owned by reconstruct +
   response-types.
5. **Wave A ↔ Wave B boundary (R-11)** — Wave A modules have zero compile-time reference to
   `transcript_hold.rs`; dependency-direction assertion in CI/review. Owned by distill-handler.

## Integration Harness Plan (infra-001)

The harness exercises the compiled `unimatrix-server` through MCP JSON-RPC. crt-052 touches server
tool logic (`context_cycle_review`), storage/lifecycle, and a security (untrusted-input) boundary, so
per the suite-selection table the applicable suites are **`tools`, `protocol`, `lifecycle`,
`security`, `edge_cases`** plus the mandatory `smoke` gate.

### Existing suites that cover crt-052 behavior

| Suite | Coverage relevance |
|-------|--------------------|
| `smoke` | MANDATORY minimum gate — must stay green; proves crt-052 wiring did not break the critical path |
| `protocol` | handshake/JSON-RPC unchanged with the additive `transcript_candidates` field present |
| `tools` | `context_cycle_review` response shape: additive section, absent-when-empty (AC-04), no new required param |
| `lifecycle` | multi-step store→search and cycle-review flows; restart persistence — confirms candidates are NOT persisted across restart |
| `edge_cases` | empty-DB cycle review, unicode/boundary payloads through the review path |
| `security` | content-scanning / input-validation boundaries — the natural home for the MCP-visible no-leak + no-panic assertions |

### Gaps (no existing suite validates these through MCP)

- The additive `transcript_candidates` section is new to the `context_cycle_review` response.
- Content-leak guarantee observable through the protocol: a re-review (memoization hit) returns the
  cached report with NO candidates, and no candidate bytes appear in any persisted/queryable state.
- Untrusted-JSONL no-panic visible at the handler boundary: a cycle review over corrupt buffer
  content returns a normal response, candidates absent — the MCP call never errors/crashes.

### New integration tests to add (Stage 3c)

| File | Test | Asserts | AC/Risk |
|------|------|---------|---------|
| `suites/test_tools.py` | `test_cycle_review_transcript_candidates_absent_when_empty` | review with no transcripts → response has no `transcript_candidates` key (absent, not null/empty) | AC-04 |
| `suites/test_tools.py` | `test_cycle_review_response_additive_only` | pre-existing response fields byte-stable; only additive section added | AC-04 |
| `suites/test_lifecycle.py` | `test_cycle_review_rereview_no_persisted_candidates` | first review, then forced re-review of the stored record → second response carries no stale candidates; restart-persistence shows no candidate content stored | AC-06, R-04 |
| `suites/test_security.py` | `test_cycle_review_corrupt_buffer_no_panic` | drive corrupt JSONL into the buffer, run cycle review → normal MCP response, candidates absent, no error/crash | AC-V-FUZZ, R-10 |
| `suites/test_security.py` | `test_cycle_review_no_candidate_content_in_query_surface` | after a review with candidates present, no candidate/transcript bytes are returned by any read tool or persisted record | AC-06, R-04 |

Fixtures: `server` (fresh DB) for the no-leak/no-panic checks; `populated_server` is NOT required.
`shared_server`/restart only for the persistence-across-restart leak check. Do NOT add harness
infrastructure changes — if a held-buffer multi-turn drain cannot be simulated through MCP (the
real per-turn drain is internal), that lifecycle proof stays in the Rust `continuity_simulated_lifecycle`
test (AC-11); file a GH Issue rather than reshaping the harness.

### Suite execution order (Stage 3c)

1. `cargo test --workspace 2>&1 | tail -30` (all Rust unit + component-integration, incl. AC-11)
2. `cargo build --release`
3. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` (mandatory gate)
4. `python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_security.py suites/test_edge_cases.py suites/test_protocol.py -v --timeout=60`
5. `cargo audit` (AC-13 dependency posture)

### Failure triage (Stage 3c)

Apply USAGE-PROTOCOL triage: failure caused by crt-052 → fix code, re-run, document. Pre-existing /
unrelated → file GH Issue, `@pytest.mark.xfail(reason="Pre-existing: GH#NNN — ...")`, continue. Bad
assertion → fix test, document. Never fix unrelated integration failures in this PR; never delete or
comment out a test.

## Merge-Gate Coverage Matrix (non-negotiable — PR blocks until all green)

| # | Merge gate | Required evidence | Owning plan | Risks |
|---|-----------|-------------------|-------------|-------|
| 1 | **AC-11 `continuity_simulated_lifecycle`** | one named Rust test: register→deltas→drain→deltas→drain→deltas→drain→re-register→cycle review (≥3 drains, inter-drain deltas). Asserts (a) cross-turn content not just last; (b) loud re-adopt on match / fail-loud on mismatch; (c) held-count ≤ cap + observable eviction; (d) TTL reclaim w/o review; (e) audit exactly-once per held session. Single-turn path explicitly rejected as evidence. | held-buffer-store | R-05, R-01, R-02, R-03 |
| 2 | **Content-leak (AC-06)** | (a) compile-level: `RetrospectiveReport` has no candidate field; (b) re-review-of-stored-record returns no candidates; (c) grep/log/SQL content-leak gate over ALL new paths (Wave A + B, extends vnc-025 AC-12); (d) content-free audit `detail`; (e) metadata-only `Debug` on `TranscriptSnapshot`/`HeldBuffer` + grep no-derive-Debug | response-types, distill-handler, snapshot-types | R-04, R-19 |
| 3 | **Four-return exhaustiveness (AC-05)** | distill→purge wired at all four `result.is_ok()` returns via one shared helper + regression test that fails if a fifth unwired success return is added | distill-handler | R-07 |
| 4 | **AC-V-FUZZ no-panic (R-10)** | malformed/adversarial JSONL corpus (truncated JSON, non-UTF-8, oversized line, unknown record type, embedded NUL) degrades to skip-with-count at module AND handler level; never `Err`, never panic; truncated final line tolerated; bounded nested/gigantic | selection-module, distill-handler | R-10 |
| 5 | **AC-V-SEAM single-reader (R-06)** | source assertion only two production buffer-content readers exist (PreCompact `contiguous_tail` + seam `snapshot()`) — fails on a third; #700-shaped reuse test parsing `TranscriptSnapshot.bytes` without `contiguous_tail`; all four metadata fields exposed | snapshot-types, snapshot-seam | R-06 |
| 6 | **AC-01 snapshot-and-release (R-08)** | no-parse-under-lock source assertion + concurrency/stress test streaming deltas during a review (no deadlock, no torn read, consistent snapshot) | snapshot-seam, snapshot-types | R-08 |

## Prerequisite Delivery Gate (not an AC — blocks Wave B audit move)

**ADR-009 no-consumer audit survey.** Before the audit points move to review/sweep/evict, a delivery
task must survey `gc_audit_log` (crt-036 — GC's by age only), retention/analytics readers of the
audit log, and any test asserting per-close `transcript_session_purged` emission, confirming no
downstream consumer keys on the per-close cadence. Record the result clean. The audit move (Wave B)
must not merge until this survey is recorded. The tester verifies the survey artifact exists and is
clean before accepting the R-03 exactly-once tests as gate evidence (the cadence change is meaningful
only if no consumer depended on the old cadence). Covered as a row in the RISK-COVERAGE-REPORT Gaps
section if not yet recorded at Stage 3c.

## Open Questions / Unpinned Values Blocking Boundary Tests

(Surfaced from RISK-TEST-STRATEGY Coverage Gaps — the spec/delivery defaults must be confirmed before
the boundary tests can assert concrete numbers; starting defaults from the brief used otherwise.)

- `transcript_hold_max_sessions` ≈ 64, `transcript_hold_ttl_secs` ≈ 86400 — cap-eviction and TTL
  boundary tests use these defaults; tests parameterize on the config knob, not the literal.
- `transcript_candidate_cycle_cap_bytes` ≈ 256 KB, truncation = **chronological keep-earliest** (per
  brief) — R-15 determinism test pins this order; if delivery pseudocode pins family-priority instead,
  the test follows the pseudocode.
- `byte_offset` = LOGICAL (base_offset-relative) — closed by ADR-002 / brief; R-12 tests assert this.
