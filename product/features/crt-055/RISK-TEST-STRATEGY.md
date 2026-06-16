# Risk-Based Test Strategy: crt-055

**Mode**: architecture-risk | **Date**: 2026-06-16 | **Author**: uni-risk-strategist (crt-055-agent-3-risk)
**Inputs**: SCOPE.md (§Producer contract, §Consumer persistence, §Binding constraints), ARCHITECTURE.md + ADR-001..010, SPECIFICATION.md (FR/NFR/AC), SCOPE-RISK-ASSESSMENT.md (SR-01..11).
**Historical evidence**: lesson #5022 (the #750 empty-clobber class, the three required assertions), #4140 (declaration-chain attribution silent no-op on evicted session), #4529/#4533 (`push_bind(f64)` without `is_finite()` → silent wrong SQL — the footgun the basis-points-INTEGER `context_reload_pct` decision designs out, no longer a runtime guard), patterns #4153/#4373/#4484 (schema-bump three-path + cascade), #4178 (derived aggregates on cycle_review_index).

**Binding design decisions (human, product owner — this revision):** (1) `context_reload_pct` is basis-points INTEGER (0–10000), not REAL — drops the REAL column and `is_finite()` guard; the #4529 float footgun is eliminated by construction (R-09). (2) Behavioral signals (transcript_error/refusal) are coarse/directional unvalidated content-opaque regex matches — must render with a directional qualifier distinct from exactly-counted aggregates (R-06). (3) Compaction-gate clock/unit is a binding contract clause — all gate timestamps normalized to Unix seconds; a must-have integration test asserts unit-consistent comparison (R-08, elevated to Critical).

> crt-055 is the consumer half of a producer/consumer pair. The highest-impact failure classes are not new code bugs — they are *re-introductions* of fixed classes (#750 empty-clobber, believable-zero) and *silent miscount* at the cross-feature seam (held-route fold, declaration-chain attribution, read-before-purge ordering). Every dishonest number here erodes the self-learning substrate this feature exists to make trustworthy.

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Second writer or recompute ignoring the data-presence gate re-introduces #750 empty-clobber: a purged cycle's real columns overwritten with zeros | High | High | Critical |
| R-02 | SUMMARY_SCHEMA_VERSION 4→5 bump treated as advisory-only — stale pre-v5 rows never recompute; believable-zero served until human force | High | High | Critical |
| R-03 | Read-after-purge: `activity_snapshot()` read after `purge_cycle_transcripts` silently zeroes all transcript_* columns | High | Med | Critical |
| R-04 | Held-route believable-zero: fold misses the held route → a real `0` indistinguishable from "no activity"; consumed and persisted as a measured zero | High | Med | Critical |
| R-05 | Cross-feature attribution miss: declaration-chain (session→feature_cycle) silently no-ops for evicted/undeclared sessions → compaction_count / fold under-attributed to the cycle | High | Med | Critical |
| R-06 | Believable-zero leaks past the per-metric presence guard: a metric with an empty source class renders `0` not "unavailable" (#750 presentation class) | High | Med | Critical |
| R-07 | Dual reload collapsed into one number/window: `context_reload` (cross-session) and `compaction_reread` (post-compaction within-cycle) conflated by a shared engine refactor | Med | Med | High |
| R-08 | `compaction_reread` gate clock/unit mismatch (binding contract clause): PostToolUse read `ts` vs `compacted_at` not both Unix **seconds** → unit mismatch makes every read count or none do — a believable-wrong-number (the silent-failure class this feature exists to kill); plus multi-compaction boundary selection (earliest) double-count/undercount | High | Med | Critical |
| R-09 | Integer-width corruption: producer `u64`/`u32` → `i64` columns wrap instead of saturate. (`context_reload_pct` is now basis-points INTEGER 0–10000 — the non-finite-float/`push_bind(f64)` footgun #4529 is **designed out**, not merely guarded; no REAL column remains) | Med | Low | Medium |
| R-10 | Three-path migration drift: ALTER block, `db.rs` fresh-create, pinned-version test, cascade file out of sync → a DB path on the wrong schema (#4153/#4484) | Med | Med | High |
| R-11 | Structural leak gate breach: a content field added to RetrospectiveReport/CycleReviewRecord, or a content read enters the persist path (R-A) | High | Low | High |
| R-12 | Producer-contract drift: crt-054 ships a field/width/catalog/`SUMMARY_SCHEMA_VERSION` deviation; consumer reads wrong indices or double-owns the schema | Med | Low | Medium |
| R-13 | Token field re-introduction: a `token_bytes_per_unit`/"tokens (est.)" column or a `reread`/`compaction` regex class violates the bytes-only honesty boundary | Med | Low | Medium |
| R-14 | `auto_close` ordering/duplication: `cycle_stop` written AFTER rank-1 reckoning (final phase mis-counts as never-closed #556), or non-idempotent duplicate stop, or a second writer | Med | Low | Medium |
| R-15 | Rank-1 timeline mis-reckoning: a closed phase re-opened counted as a new phase not rework; #556 never-closed false-positive when `cycle_stop` exists | Med | Med | High |
| R-16 | Rank-3 source under/over-count: knowledge-reuse reads only one of query_log/injection_log (not the union), or double-counts an entry served via both (#320 regression) | Med | Med | High |
| R-17 | Pre-divided ratio persisted: rework ratio / reuse rate stored as one number → "0 of 0" indistinguishable from "0 of N" (re-introduces believable-zero at the ratio) | Med | Low | Medium |
| R-18 | Migration version handshake with crt-054 collides or skips: two disjoint-table ALTERs take the same `CURRENT_SCHEMA_VERSION` number (#4095) | Med | Low | Medium |

---

## Risk-to-Scenario Mapping

### R-01: Second writer / data-presence-gate bypass re-introduces #750 empty-clobber
**Severity**: High **Likelihood**: High **Impact**: A purged cycle's genuine durable aggregates overwritten with zeros — silent, permanent data corruption of the cross-cycle baseline. Evidence #5022: the only `store_cycle_review()` writer sits past two presence guards; the store layer persists an empty record if called directly.

**Test Scenarios**:
1. Stale pre-v5 row, source data present → assert recompute writes fresh non-zero columns via clear-memo-and-fall-through (the single writer), at schema_version 5.
2. Stale pre-v5 row, source purged → assert the stored row is retained byte-identical, no write, advisory = "source purged, cannot recompute" (never "use force=true").
3. `force=true` on a purged row → assert `computed_at` does NOT advance and columns are not clobbered with zeros (the :2089-class interceptor path).
4. Static/structural: assert exactly ONE `store_cycle_review()` call site writes the new columns; memo-hit / purged-retain / force+purged returns contain no column write.

**Coverage Requirement**: The three #5022 assertions (a/b/c) plus the single-writer structural assertion all pass (AC-17, AC-18). No second `store_cycle_review` near the memo/`check_stored_review` site.

### R-02: Schema-version bump is advisory-only — stale rows never flush
**Severity**: High **Likelihood**: High **Impact**: Believable-zero pre-v5 summaries served indefinitely until a human forces recompute (the exact #750/#5022 failure). The bump alone does nothing.

**Test Scenarios**:
1. Bump 4→5, then assert a stale+data-present row recomputes to a fresh row at schema_version 5 on next review (not advisory-served).
2. Assert the serve/recompute decision lives in the handler (typed `Staleness` enum reading `attributed`), not inside `check_stored_review` (which holds only `&CycleReviewRecord`).
3. Purged+stale → distinct advisory, retain stored; no recompute attempted.

**Coverage Requirement**: A test that asserts the cache actually flushes for stale+present is required — a version-equality test alone is insufficient (AC-03, AC-18, NFR-03).

### R-03: Read-after-purge zeroes the transcript fold
**Severity**: High **Likelihood**: Med **Impact**: All transcript_* and signal_class_counts_json columns land as zeros for an active cycle — a believable-zero indistinguishable from a quiet cycle. A future refactor moving the purge earlier silently breaks this.

**Test Scenarios**:
1. Ordering assertion: the `activity_snapshots_for_feature()` call site strictly precedes `purge_cycle_transcripts` in the review pipeline.
2. Inversion test: a test that reverses the order fails (columns zeroed) — proving the assertion is load-bearing, not decorative.
3. Cycle with known non-zero fold → assert columns equal the summed snapshot after a full pipeline run that includes the purge.

**Coverage Requirement**: AC-08 — read-site-precedes-purge asserted; the inverted-order case demonstrably zeroes the columns (Constraint 6, SR-09).

### R-04: Held-route believable-zero
**Severity**: High **Likelihood**: Med **Impact**: A cycle's fold reads zero because the held route was missed (the #1 producer regression risk, crt-054 ADR-001), surfaced and persisted as a measured zero. The consumer cannot distinguish it from genuine silence without the presence flag.

**Test Scenarios**:
1. Representative TS-client cycle with held activity → assert the fold source is non-empty and `transcript_*` columns are non-zero (the regression guard).
2. Cycle with one undeclared session among valid declared sessions → assert the valid sessions' fold is NOT zeroed by the undeclared one (per-session presence handled in the sum).
3. Undeclared-only cycle → transcript metrics render "unavailable", never `0`.

**Coverage Requirement**: AC-09 (silent-zero regression guard) — non-empty fold asserted for a representative held cycle; per-metric presence flag drives "unavailable" for the empty case (SR-08, NFR-08).

### R-05: Cross-feature attribution silent miss (declaration-chain no-op)
**Severity**: High **Likelihood**: Med **Impact**: `compaction_events` rows and folds attribute to a cycle via the session→`feature_cycle` chain at review. Evidence #4140: `set_feature_force` silently no-ops for an evicted SM session, so its observations get `feature_cycle = NULL` and never attribute. A cycle whose SM session was drained before `context_cycle(start)` under-counts compactions and fold — silently.

**Test Scenarios**:
1. Seed `compaction_events` for declared and undeclared sessions → assert only declared-session rows count toward `compaction_count` (AC-11).
2. Evicted/undeclared-session scenario (the #4140 condition) → assert the cycle surfaces "unavailable" or an honest partial, never a fabricated complete-looking zero.
3. Multi-session cycle → assert fold and compaction sums attribute across all *declared* sessions of the cycle, none from other cycles.

**Coverage Requirement**: Attribution is by the declaration chain at review; undeclared/evicted sessions do not mis-attribute and do not fabricate zeros (AC-11, NFR-08, SR-10). Reference #4140 as the known silent-no-op condition.

### R-06: Believable-zero leaks past the presentation guard
**Severity**: High **Likelihood**: Med **Impact**: An empty source class renders `0` not "unavailable" — the #750 presentation class (#4998), an honest computation with a dishonest output, polluting cross-cycle baselines.

**Test Scenarios**:
1. Synthesize a cycle per source class (zero cycle_events; zero compaction_events; empty fold; zero served-knowledge) → assert each renders "unavailable", never literal "0".
2. Assert per-metric `available` flags are independent — one empty source does not flip another metric's flag, and one present source does not mask another's emptiness.
3. Ratio metrics: "0 of 0" → "unavailable"; "0 of N" → a genuine measured rate (drives off the stored num/den pair).
4. **Behavioral-signal honesty (binding decision):** `transcript_error` / `transcript_refusal` counts are **coarse/directional** — unvalidated, content-opaque regex matches, NOT exactly-counted aggregates. Assert the report renders these with a directional/coarse qualifier (e.g. "~", "directional", "approx") that visually distinguishes them from precisely-counted aggregates (cycle_events, compaction_count). A non-zero count must read as "directional signal of N", never as an authoritative exact tally.

**Coverage Requirement**: AC-01 — every metric with an empty source renders "unavailable"; per-metric (not cycle-wide) granularity verified. **Presentation-honesty (new):** transcript_error/refusal counts render with the coarse/directional qualifier, visibly distinct from exactly-counted aggregates — a presentation-honesty assertion alongside the "unavailable"-not-"0" coverage (FR-01/02, ADR-003).

### R-07: Dual reload collapsed into one number
**Severity**: Med **Likelihood**: Med **Impact**: `context_reload` (cross-session continuity) and `compaction_reread` (within-cycle compaction tax) conflated — two distinct semantics reported as one, destroying the signal the feature exists to add. The shared overlap engine is the temptation surface (ADR-005).

**Test Scenarios**:
1. Assert both columns persist independently from distinct overlap windows; neither is derived from the other's window.
2. A cycle with cross-session reload but zero compactions → `context_reload_pct` non-zero, `compaction_reread` "unavailable".
3. A cycle with compaction-rereads but a single session → `compaction_reread` non-zero, `context_reload` reflects only cross-session (here near-zero/unavailable).

**Coverage Requirement**: AC-13 — two distinct columns, two gates, one shared primitive; a refactor cannot silently merge the windows (SR-06, Constraint 5).

### R-08: compaction_reread gate clock/unit mismatch + boundary-selection error
**Severity**: High **Likelihood**: Med **Impact**: The `compaction_reread` gate compares PostToolUse read `ts` against `compacted_at`. **Binding contract clause (human decision):** ALL gate timestamps are normalized to Unix **seconds** (millis→seconds at the boundary) before comparison. A latent unit mismatch (one side seconds, one side millis) is a silent-failure class — every read counts (millis `ts` always > seconds `compacted_at`) or none do — a *believable-wrong-number*, exactly the family this feature exists to kill: no error, no crash, just a plausible-looking corrupt tax. Separately, wrong multi-compaction boundary mis-counts reads on the boundary.

**Test Scenarios**:
1. **Unit-consistent gate (must-have integration test):** drive the full gate path — a PostToolUse read recorded **+500ms after** `compacted_at` is counted as a reread; a read **−500ms before** is not (sub-second boundary that actually exercises the ÷1000 floor — a ±1s window passes even if the floor is absent/wrong). Then inject a deliberate unit mismatch (one side in millis) and assert the normalization catches it / the comparison still uses seconds-vs-seconds (the mismatch does NOT flip the count to all-or-nothing). This is an integration test across the compaction-events table and the PostToolUse read source, not a unit test of the comparator alone.
2. PostToolUse reads straddling `compacted_at` (one just before, one just after, one exactly at) → assert only strictly-after overlapping re-reads count; assert seconds-vs-seconds comparison at the boundary.
3. Multi-compaction session (N>1 rows) → assert the earliest `compacted_at` (MIN) is the gate (ADR-006) and each re-read counts at most once (no per-boundary double-count).
4. Assert `compaction_count` still reports the full count of boundaries even though the reread gate uses one (ADR-005/006).

**Coverage Requirement**: AC-12 — **unit-consistent gate comparison is a must-have integration test** (read +500ms post-compaction counted, −500ms pre-compaction not — sub-second boundary exercising the ÷1000 floor; injected unit mismatch caught, never all-or-nothing); seconds-aligned binding contract enforced at the boundary; earliest-boundary selection; re-read counted once; `compaction_count` ≠ reread gate (FR-14/15, SR-11).

### R-09: Integer-width corruption at the persist boundary
**Severity**: Med **Likelihood**: Low **Impact**: `u64` near max wraps to negative `i64` on cast — a corrupt baseline that looks valid. **Design decision (binding):** `context_reload_pct` is stored as basis-points INTEGER (0–10000), not REAL — the `push_bind(f64)` NaN/Inf silent-wrong-SQL footgun (#4529/#4533) is eliminated by construction for this column, not deferred to an `is_finite()` runtime guard. No REAL column remains in the new schema, so the non-finite-float scenario is retired (was the only float bind).

**Test Scenarios**:
1. Near-`u64::MAX` / large `u32` fold values → assert persisted `i64` is correct or saturated-and-warned, never wrapped.
2. `context_reload_pct` round-trip: producer ratio → basis-points `i64` (0–10000) → `CycleReviewRecord` field → re-read equals stored; assert an out-of-range value (>10000 or negative) is clamped/rejected before bind, never silently truncated.
3. Generic int-width round-trip across the new `i64` columns: persisted value re-reads byte-identical.

**Coverage Requirement**: AC-14 — checked/saturating conversion verified across all `i64` columns; basis-points range guard (0–10000) on `context_reload_pct` at the bind boundary. The `is_finite()`/non-finite-float requirement is retired — the float footgun is designed out by integer storage (FR-12, evidence #4529).

### R-10: Three-path migration drift
**Severity**: Med **Likelihood**: Med **Impact**: ALTER block, `db.rs` fresh-create, pinned-version test, and the cascade file fall out of sync → a fresh DB and an upgraded DB disagree on schema (#4153/#4373/#4484).

**Test Scenarios**:
1. Migration test: `pragma_table_info` lists every new column with correct type/default on BOTH a fresh DB and an upgraded DB; idempotent re-run is a no-op.
2. Assert the pinned `SUMMARY_SCHEMA_VERSION == 5` and `CURRENT_SCHEMA_VERSION` test assertions move in the same change.
3. Assert the prior `migration_vN_to_vN+1.rs` cascade file exists (#4484) before the new one.

**Coverage Requirement**: AC-02, AC-03 — fresh-create and upgrade paths agree; pragma-guarded ALTERs; idempotent; pinned-version test updated (NFR-03).

### R-11: Structural leak gate breach
**Severity**: High **Likelihood**: Low **Impact**: A content field on the report/record, or a content read on the persist path, leaks transcript bytes into the durable substrate — violates the core honesty/safety invariant the whole feature is built around.

**Test Scenarios**:
1. `test_candidates_structurally_absent_from_memoized_report` holds after all new fields are added.
2. Structural test: assert no new field on `CycleReviewRecord`/`RetrospectiveReport` carries content (only `i64`/`f64`/`String`-aggregate types; `signal_class_counts_json` is a count map, not content).
3. Assert the consumed surfaces (`activity_snapshot()`, `compaction_events`) expose counters/metadata only — no `Display`/content serialization enters the persist path.

**Coverage Requirement**: AC-19 — leak gate holds; no content field; no content read on persist (NFR-01/02, R-A default NO).

### R-12: Producer-contract drift
**Severity**: Med **Likelihood**: Low **Impact**: crt-054 deviates from the binding contract (field rename, width change, catalog index shift, or a stray `SUMMARY_SCHEMA_VERSION`/`cycle_review_index` touch) → consumer reads wrong indices, or the two features double-own the schema.

**Test Scenarios**:
1. Assert consumer reads `class_counts[0]=error`, `[1]=refusal` against the contract-pinned indices (ADR-008); a catalog reorder is caught.
2. Assert crt-054 does NOT bump `SUMMARY_SCHEMA_VERSION` and does NOT ALTER `cycle_review_index` (boundary test / code review at merge).
3. Field/width/signature compatibility test against the published `ActivitySnapshot` / `compaction_events` shapes (§6 Integration Surface).

**Coverage Requirement**: Contract field-by-field alignment holds (ARCHITECTURE §9, already verified ALIGNED); merge-time boundary check confirms disjoint ownership (SR-04/05).

### R-13: Token field / forbidden regex class re-introduction
**Severity**: Med **Likelihood**: Low **Impact**: A `token_bytes_per_unit`/"tokens (est.)" column or a `reread`/`compaction` regex class re-imports the prior contradiction and violates the bytes-only honesty boundary.

**Test Scenarios**:
1. Structural/grep guard: no token-named field on `CycleReviewRecord`/`RetrospectiveReport`; no token-named column.
2. Assert no `reread`/`compaction` class exists in `[transcript_signals]` (those are review-time reckoning, not in-stream signatures).

**Coverage Requirement**: AC-10 — absence of any token-named field and any `reread`/`compaction` regex class (FR-11, NFR-07, SR-05).

### R-14: auto_close ordering / duplication
**Severity**: Med **Likelihood**: Low **Impact**: `cycle_stop` written after rank-1 reckoning → the final phase reads as never-closed (false #556 hotspot); or a non-idempotent duplicate stop; or a second writer violating Constraint 2.

**Test Scenarios**:
1. `auto_close=true`, no prior stop → `cycle_stop` written synchronously at the TOP of the pipeline, before rank-1 reads the timeline; final phase closes (not a false never-closed).
2. `auto_close=true`, stop already exists → no duplicate written (idempotent).
3. `auto_close=false` → no stop; an open final phase correctly surfaces as never-closed (#556 fail-loud, not an error).
4. Assert `auto_close` writes a `cycle_events` row via the existing event writer, not a second `store_cycle_review` writer.

**Coverage Requirement**: AC-15 — three auto_close paths; ordered-before-reckoning; idempotent; no second cycle_review_index writer (FR-18/19, ADR-010).

### R-15: Rank-1 timeline mis-reckoning
**Severity**: Med **Likelihood**: Med **Impact**: A closed phase that re-opens counted as a new phase rather than rework; or a #556 never-closed false-positive when a `cycle_stop` (or matching phase-end) actually exists → wrong phase aggregates.

**Test Scenarios**:
1. Seed `cycle_events` with a phase that closes then re-opens → assert it counts as `phase_rework_count`, not a second `phase_count`.
2. Phase with declared start and no close → `phase_unclosed_count` increments (#556); phase with a matching close → does NOT.
3. With `auto_close=true` closing the cycle, the final phase is NOT counted as never-closed.

**Coverage Requirement**: AC-04, AC-05 — never-closed detection and rework-loop reckoning correct against a seeded timeline (FR-05/06, ADR-004).

### R-16: Rank-3 knowledge-reuse source error (#320)
**Severity**: Med **Likelihood**: Med **Impact**: Reads only query_log OR injection_log (not the union), or double-counts an entry served via both → the #320 regression (under/over-count of knowledge reuse).

**Test Scenarios**:
1. Seed served entries split across query_log and injection_log, including cross-cycle-tagged → assert reuse count == size of the UNION (not same-cycle-tagged only).
2. An entry served via BOTH logs → assert it counts once (union dedup), not twice.
3. Confirm the actual injection_log table/column names at spec time (Open Q1) — a wrong table name yields a silent zero.

**Coverage Requirement**: AC-06 — union semantics, dedup, all-served (not same-cycle-tagged) (FR-08, ADR-004).

### R-17: Pre-divided ratio re-introduces believable-zero
**Severity**: Med **Likelihood**: Low **Impact**: Storing a single divided ratio (rework rate, reuse rate) loses the denominator → "0 of 0" indistinguishable from "0 of N" at the ratio level.

**Test Scenarios**:
1. Assert numerator/denominator PAIRS are stored (`rework_session_count`/`total_session_count`), ratio derived at presentation.
2. "0 of 0" → "unavailable"; "0 of N" → measured rate.

**Coverage Requirement**: Num/den pairs persisted, never a pre-divided number (ADR-004, ADR-003).

### R-18: Migration version handshake collision with crt-054
**Severity**: Med **Likelihood**: Low **Impact**: Both disjoint-table ALTERs claim the same `CURRENT_SCHEMA_VERSION` number at merge → one migration silently skipped (#4095).

**Test Scenarios**:
1. At merge, assert crt-054 and crt-055 hold distinct sequential `CURRENT_SCHEMA_VERSION` numbers (N and N+1); the migration test for each runs.
2. Both migrations apply cleanly in either merge order (disjoint tables).

**Coverage Requirement**: SM merge-coordination check; distinct sequential versions (NFR-04, ARCHITECTURE §9, lesson #4095).

---

## Integration Risks

The crt-055↔crt-054 producer/consumer seam is the dominant integration surface and concentrates the Critical risks:
- **Read-before-purge ordering (R-03)** — a temporal coupling between crt-055's review pipeline and crt-052's `purge_cycle_transcripts`; the only protection is an asserted call-site ordering.
- **Held-route fold + declaration-chain attribution (R-04, R-05)** — crt-055 consumes counters crt-054 produced and attributes them via a chain that silently no-ops (#4140). The integration miss surfaces as a believable zero, not an error.
- **Shared `[transcript_signals]` catalog index contract (R-12)** — crt-055 lands columns by fixed index (`0=error`, `1=refusal`); a producer reorder corrupts every transcript column with no type error.
- **Gate clock/unit coupling (R-08, Critical)** — `compaction_reread` depends on `compacted_at` and PostToolUse `ts` both being Unix seconds — a cross-table comparison with no compiler enforcement. Binding contract clause normalizes all gate timestamps to seconds; the must-have integration test asserts unit consistency (read 1s post-compaction counted, mismatch caught) because the failure mode is a believable-wrong-number, not an error.
- **Single-writer coexistence with #758 (R-01, R-02)** — crt-055's new columns must thread through one INSERT (`:249`) + one UPDATE (`:284`) and coexist with the four-return discipline; the integration failure is a re-introduced empty-clobber.
- **Migration version handshake (R-18)** — two features, two disjoint ALTERs, one sequential-number coordination point at merge.

## Edge Cases

- Cycle with **zero declared sessions** → every source-derived metric "unavailable", no fabricated zeros (R-06).
- Cycle with a **mix of declared and undeclared/evicted sessions** → declared sessions aggregate; undeclared do not zero them and do not mis-attribute (R-04, R-05).
- Session that **compacts multiple times** → earliest boundary gates reread; `compaction_count` reports all (R-08).
- **Read recorded +500ms after `compacted_at`** → counted as a reread; **−500ms before** → not counted (sub-second boundary exercising the ÷1000 floor); a **unit mismatch** (millis vs seconds) → caught by seconds-normalization, never an all-or-nothing count (R-08, binding contract clause).
- Session that **compacts but never re-reads** → `compaction_count` > 0, `compaction_reread_count == 0` (a genuine measured zero, distinct from "unavailable").
- **`u64::MAX`-adjacent** fold values; **out-of-range** (>10000 / negative) basis-points `context_reload_pct` candidate (R-09).
- **Pre-v5 stale + purged** vs **pre-v5 stale + present** vs **fresh v5** — three distinct recompute outcomes (R-01, R-02).
- **Re-review with `auto_close=true`** when stop already exists → idempotent no-op (R-14).
- Phase that **closes then re-opens** → rework, not a new phase (R-15).
- Knowledge entry **served via both** query_log and injection_log → counted once (R-16).
- **`auto_close=false` on an open cycle** → final phase honestly surfaces as never-closed (correct, not a bug).

## Security Risks

crt-055 accepts no new untrusted *network* input; its inputs are the MCP `auto_close: bool` parameter and three trusted internal data surfaces. Assessment:
- **`[transcript_signals]` config (untrusted-ish: operator-supplied regex).** A malicious/malformed regex is the primary external-input surface. Catastrophic-backtracking patterns (ReDoS) run on the hot ingest path under the buffer lock. Mitigation: producer-side `validate()` rejects invalid regex loudly at startup (ADR-008); crt-055 only consumes compiled counts. **Blast radius**: a bad regex is a producer (crt-054) startup failure, not a crt-055 runtime path — but crt-055's tests should assert it consumes only bounded `[u32; MAX_SIGNAL_CLASSES]` counters and never the pattern or matched bytes.
- **`signal_class_counts_json` (constructed TEXT).** Built from a `class_name → count` map. Risk: a class_name from config injected into JSON. Mitigation: serialize via a real JSON serializer (never string concatenation); assert class_names are config-validated (no duplicate, bounded count) and counts are integers. Blast radius: a malformed JSON column, read back by the report — contained, but assert round-trip integrity.
- **`compaction_events` / fold (content-opacity).** The defining security property: NO transcript content enters the persist path (R-11). Blast radius if breached = leaking conversation bytes into a durable, cross-cycle, exportable substrate. This is the highest-consequence breach and is gated structurally (no content field, metadata-only consumed surfaces).
- **Integer binding (R-09).** Width-wrap on `u64→i64` is a silent-wrong-SQL surface, not an injection, but corrupts the durable baseline — guard with checked/saturating conversion. The prior NaN/Inf-via-`push_bind(f64)` surface (#4529) is removed: `context_reload_pct` is basis-points INTEGER, so no float reaches the bind.
- **`auto_close` is informs-not-controls (NFR-07).** It writes a record event, never controls execution — no privilege escalation or orchestration surface.

## Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| Source class empty for a metric | Render "unavailable" with a terse reason, never `0` (R-06) |
| Transcript fold absent (undeclared / held-route miss) | "unavailable" per-metric; do not zero other valid sessions' contribution (R-04) |
| Zero compaction_events for the cycle | `compaction_count = 0`, `compaction_reread` "unavailable" (no boundary), distinct from a measured zero |
| Gate timestamp unit mismatch (millis vs seconds) | Seconds-normalization at the boundary catches it; comparison stays seconds-vs-seconds, never all-or-nothing count (R-08, binding contract clause) |
| Stale pre-v5 row, source present | Auto-recompute via clear-memo-fall-through, fresh non-zero columns at v5 (R-02) |
| Stale pre-v5 row, source purged | Retain stored bytes; advisory "source purged, cannot recompute"; no write (R-01) |
| `force=true` on a purged row | Serve stored; do not advance `computed_at`; no zero-clobber (R-01) |
| Integer overflow on width conversion | Saturate-and-warn, never panic, never wrap (R-09) |
| Out-of-range `context_reload_pct` (basis-points) | Clamp/reject to 0–10000 before bind; integer storage removes the NaN/Inf class entirely (R-09) |
| Evicted/undeclared SM session | Honest partial / "unavailable"; never a fabricated complete-looking zero (R-05, #4140) |
| `auto_close=true`, stop exists | Idempotent no-op (R-14) |
| Migration re-run | Idempotent (pragma-guarded ALTERs) (R-10) |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 silent-zero / empty-clobber | R-01, R-02, R-06 | ADR-002 single writer + four returns + #758 data-presence gate; ADR-003 per-metric presence guard. Three #5022 assertions (AC-17/18) + per-source "unavailable" (AC-01). |
| SR-02 stale-version no-flush | R-02 | ADR-001/002 — typed `Staleness` enum in handler, clear-memo-fall-through recompute; AC-18 asserts flush-on-stale-present. |
| SR-03 three-path schema bump | R-10, R-18 | ADR-001 — crt-047 template, pragma-guarded ALTERs, pinned-version test in same change, cascade-file check (#4153/#4373/#4484); AC-02/03. |
| SR-04 producer-contract drift | R-12 | ARCHITECTURE §9 field-by-field reconciliation (verified ALIGNED); #5006 already deprecated→#5032; merge-time disjoint-ownership boundary check. |
| SR-05 bytes-vs-tokens | R-13 | ADR-007/008 bytes-only; AC-10 structural guard asserts no token-named field, no `reread`/`compaction` class. |
| SR-06 dual-reload collapse | R-07 | ADR-005 two columns / two gates / one engine; AC-13 asserts independent windows. |
| SR-07 scope creep on point-issues | R-15, R-16, R-17 (+ ADR-009 #206-4 response-only) | ADR-004 locked rank shapes; ADR-009 keeps #206-4 a response-time list (no column); ADR-008 tiny catalog. |
| SR-08 held-route believable-zero | R-04, R-06 | ADR-003/007 per-metric presence flags; AC-09 regression guard asserts non-empty fold for a representative TS-client cycle. |
| SR-09 read-before-purge | R-03 | ADR-007 read-site pinned ahead of `purge_cycle_transcripts`; AC-08 ordering + inversion test. |
| SR-10 attribution + int-width | R-05, R-09 | ADR-006/007 declaration-chain attribution + checked/saturating conversion; basis-points INTEGER `context_reload_pct` (float footgun designed out, no `is_finite()` guard needed); AC-11/14; evidence #4140 (evicted-session no-op), #4529 (the float footgun this storage choice eliminates). |
| SR-11 multi-compaction boundary | R-08 | ADR-006 earliest `compacted_at`, counted once; binding contract clause normalizes all gate timestamps to Unix seconds; AC-12 boundary-selection assertion + must-have unit-consistent-gate integration test (read 1s post-compaction counted; unit mismatch caught). |

All eleven SR-XX risks are traced to at least one architecture risk and resolving ADR/AC. No scope risk is accepted-unaddressed or out-of-scope.

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 7 (R-01..R-06, R-08) | ~22 scenarios — the #750/empty-clobber class, schema-flush, read-before-purge, held-route fold, cross-feature attribution, presentation/behavioral-signal-honesty guard, compaction-gate clock/unit contract |
| High | 4 (R-07, R-10, R-15, R-16) | ~10 scenarios — dual-reload split, migration three-path, rank-1 timeline, rank-3 union |
| Medium | 7 (R-09, R-11, R-12, R-13, R-14, R-17, R-18) | ~14 scenarios — int-width binding, leak gate, contract drift, token guard, auto_close, ratio pairs, version handshake |
| **Total** | **18** | **~46 scenarios** |

**Top coverage requirements (must-have before merge):**
1. The three #5022 assertions (data-present recompute / purged retain / force+purged no-clobber) + single-writer structural assertion (R-01, R-02 — AC-17/18).
2. Read-before-purge ordering assertion AND the inverted-order-zeroes-columns test (R-03 — AC-08).
3. Held-route non-empty-fold regression guard for a representative TS-client cycle (R-04 — AC-09).
4. Per-source "unavailable"-never-"0" across every metric source class, AND the behavioral-signal honesty assertion — transcript_error/refusal render with the coarse/directional qualifier, visibly distinct from exactly-counted aggregates (R-06 — AC-01).
5. Declaration-chain attribution: declared-only counting, evicted/undeclared no fabricated zero (R-05 — AC-11; evidence #4140).
6. **Unit-consistent compaction-gate integration test** — read +500ms post-`compacted_at` counted, −500ms pre not (sub-second boundary exercising the ÷1000 floor); injected millis/seconds unit mismatch caught, never an all-or-nothing count (R-08 — AC-12; binding contract clause: all gate timestamps Unix seconds).

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (lesson-learned, pattern, integration) and `context_get` — surfaced #5022 (the #750 empty-clobber three-assertion lesson, the single-writer-past-two-guards detail), #4140 (declaration-chain attribution silent no-op on evicted session — elevated R-05 likelihood), #4529/#4533 (`push_bind(f64)` non-finite silent wrong SQL — informs R-09), patterns #4153/#4373/#4484 (schema-bump three-path + cascade — R-10/R-18). Evidence raised R-05 to Critical and added the `is_finite()` scenario to R-09.
- Stored: nothing novel to store — the recurring patterns this feature exposes (single-writer-past-presence-guards, schema-bump-three-path, declaration-chain silent no-op, content-opacity persist gate) are already captured as #5022/#4153/#4140/#4178. No 2+-feature risk pattern emerged that is not already in Unimatrix.
