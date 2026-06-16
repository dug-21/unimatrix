# Risk-Based Test Strategy: crt-054

**Date**: 2026-06-16 — **architecture-risk mode, producer-only re-scope.** Updated 2026-06-16 (open-question rework): the four crt-054 open questions are resolved and ADR-007/ADR-002 updated — see the "Open-question resolutions encoded" note at the foot of Coverage Summary; targeted edits to R-01, R-07, R-11, R-03 + new R-15, numbering otherwise stable. Supersedes the prior RISK-TEST-STRATEGY.md (2026-06-14) written for the wider scope (crt-054 owning `cycle_review_index` / `store_cycle_review` / `SUMMARY_SCHEMA_VERSION`). That surface — the prior highest-risk surface — moved entirely to crt-055 and its risks are gone from this register.
**Inputs**: SCOPE.md (re-scope note), SCOPE-RISK-ASSESSMENT.md (SR-01..SR-10), ARCHITECTURE.md, ADR-001..010, SPECIFICATION.md (FR/NFR/AC-01..15).
**Scope**: two producer surfaces only — Surface A (`compaction_events` table) + Surface B (`activity_snapshot()` in-memory fold).

These risks are specific to the **designed** system: the believable-zero/held-route trap, fold survival across the crt-052 hold purge, the lock graph at the `listener.rs:1854` INSERT seam, schema-version sequencing against crt-055, producer→consumer integer widths, late-bind attribution honesty, and the crt-055 producer-contract coupling. Generic risks are excluded.

---

## Risk Register

Priority = Severity × Likelihood. Critical = High×High, or High×Med where the failure is silent (reads as believable data).

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Held-route believable-zero** — fold misses the held-delta route (`session.rs:388-395`); drained-session bytes silently read as 0 at review. The #750/#5025 class reborn at a new seam. | High | Med | **Critical** |
| R-02 | **Read-after-purge / fold dropped before review** — the counter is zeroed/dropped (or the read moves after `purge_cycle_transcripts`) before crt-055 reads it; every counter reads 0. | High | Med | **Critical** |
| R-03 | **Lock-graph deadlock/stall at the INSERT seam** (`listener.rs:1854`) — a DB acquisition under registry/session/buffer locks held at `handle_compact_payload` deadlocks or stalls the compaction ACK on the hot path. | High | Med | **Critical** |
| R-04 | **Schema-version sequencing collision with crt-055** — both take the next `CURRENT_SCHEMA_VERSION` bump (28→29/30); parallel merge claims the same number or leaves a gap; in-flight feature's migration block goes stale. | High | Med | **Critical** |
| R-05 | **crt-055 producer-contract drift** — field/width/default-catalog/index-order diverge between producer and consumer designed in parallel; columns land mis-typed or class indices mismatch. | High | Med | **High** |
| R-06 | **Single-event / per-turn-drain dependence reintroduced** — an output silently depends on `PreToolUse` or any single hook-event/per-turn-registry-state presence that vanishes under a client change (the #5025/#4799 mechanism). | High | Low | **High** |
| R-07 | **Producer→consumer integer-width truncation** — in-memory `bytes_total: u64` / `delta_count: u32` / `class_counts: [u32;N]` wrap or truncate crossing into crt-055's `i64` columns. | Med | Low | **Medium** |
| R-08 | **Late-bind attribution fabricates a zero** — an undeclared/purged session contributes a fabricated `0` to the activity read instead of signalling absence; a real "no data" looks like a measured zero-activity cycle. | Med | Med | **Medium** |
| R-09 | **Surface A row written while a buffer lock is still held** — `high_water` capture at `:1854` does not drop the buffer guard before the INSERT (a subtler form of R-03 specific to the `high_water()` read). | Med | Med | **Medium** |
| R-10 | **`[transcript_signals]` config: silent fallback on invalid/over-cap input** — invalid regex or > `MAX_SIGNAL_CLASSES` degrades silently or wedges, instead of failing loud at load. | Med | Low | **Medium** |
| R-11 | **`compacted_at` granularity mismatch** — seconds-precision at the seam vs the `ts` crt-055 gates against; an off-by-one-second or unit mismatch corrupts the reload gate comparison. | Med | Low | **Medium** |
| R-12 | **Stale-scope residue re-imported** — `saw_compaction`/`reload` latch, `reread`/`compaction` regex class, `token_bytes_per_unit`, or a `cycle_review_index` ALTER leaks back in from prior artifacts/ADRs. | Med | Med | **Medium** |
| R-13 | **`high_water` over-trusted as wire-precise** — a future precise byte-boundary gate (vnc-036) treats the server-captured `high_water` as wire-exact; semantically it is not. | Low | Low | **Low** |
| R-14 | **Multi-compaction row semantics** — a session compacting N times writes N rows; a producer-side assumption of one-row-per-session corrupts the 0..N contract. | Low | Low | **Low** |
| R-15 | **INSERT-failure silent undercount** — a failed `compaction_events` INSERT silently drops a compaction and undercounts the compaction tax; with only a generic log line the loss is invisible. A believable-undercount, not a believable-zero, but the same silent-degradation class. | High | Low | **High** |

---

## Risk-to-Scenario Mapping

### R-01: Held-route believable-zero (Critical) — AC-06
**Severity**: High · **Likelihood**: Med
**Impact**: Drained sessions (the normal TS-client lifecycle — Stop/SessionClose drains every turn, #4799) fold into the *held* buffer. If the fold only fires on the registered route, every multi-turn cycle reads `bytes_total = 0` — an honest count over an empty source. This is exactly #5025/#750: a metric that reads "the cycle did nothing." A unit test on the accumulator passes while the real seam is broken.

**Test Scenarios**:
1. **Mandatory held-route regression guard**: drive a multi-turn, multi-session cycle through drain→hold→re-adopt (the TS-client lifecycle) with non-trivial delta bytes; at review assert `bytes_total > 0` and `delta_count > 0`. A registered-only or no-op path does NOT satisfy this (pattern #3624).
2. Deltas arriving *after* drain (on the held `Arc` via `held_arc_for_session`) increment the same accumulator the registered route did — assert continuity across the drain boundary, not just two isolated non-zero reads.
3. Negative-mutation check: a test that would still pass if `apply_delta`'s held-route call were removed is insufficient — the guard must fail red if the held route stops feeding the fold.

**Coverage Requirement**: AC-06 satisfied only by an integration test that **provably exercises the held route on a representative TS-client cycle** (drain→hold→re-adopt with measurable bytes) on the cumulative crt-052/vnc-025 fixtures. A registered-only OR unit-only test gives **false confidence** and does NOT satisfy AC-06 — that is precisely the #750/#5025 class (a test green while the live seam reads 0). The guard **must not degrade into a no-op**: scenario 3's negative-mutation check is mandatory — if removing the held-route `apply_delta` call leaves the test green, the test is invalid and must be rejected at the test-design gate. Evidence: #5025 (TS client retired PreToolUse → per-session metrics collapsed to 0), #5007 (the two zero-traps), #4799 (per-turn drain starves review-time consumers).

### R-02: Read-after-purge / fold dropped before review (Critical) — AC-07
**Severity**: High · **Likelihood**: Med
**Impact**: The fold rides the crt-052 Wave B hold to review (ADR-006). If crt-054 zeroes/resets the counter on any path, or if the crt-055 read is ordered *after* `purge_cycle_transcripts`, the snapshot reads 0. Same believable-zero blast radius as R-01, different seam (sequencing not routing).

**Test Scenarios**:
1. Read-before-purge ordering test: assert `activity_snapshot()` returns non-zero counters, then assert `purge_cycle_transcripts` zeroes the buffer — i.e. the read provably happens first.
2. Assert no crt-054 code path zeroes/drops/resets the accumulator between fold and review (the buffer + its embedded accumulator survive drain intact).
3. Survival across the full drain→hold→review lifecycle for a held session: snapshot at review equals the sum of folded deltas, not a partial or reset value.

**Coverage Requirement**: AC-07 — integration test proving read-before-purge ordering. Depends on crt-052 Wave B staying ON/non-disableable (NFR-7 hard dependency; a config re-enabling purge-before-read breaks the fold silently — ADR-010 fails loud at startup if the hold handle is unwired).

### R-03: Lock-graph deadlock/stall at the INSERT seam (Critical) — AC-04 / SR-01
**Severity**: High · **Likelihood**: Med
**Impact**: `handle_compact_payload` holds registry/session/buffer locks; the briefing build and tail read run on the handler. A DB INSERT acquired under those locks can deadlock against background writers or stall the compaction ACK on the hot drain path. ADR-007 resolves this by ordering the INSERT *after* `increment_compaction` with **no lock held across the INSERT** — but that ordering must be tested, not asserted.

**Test Scenarios**:
1. Concurrency test: drive compaction through the handler under registry/session lock contention (concurrent delta routing + background store writes); assert no deadlock and no timeout; the compaction ACK completes within bound.
2. Assert the buffer guard for the `high_water()` read is dropped *before* the INSERT (the `Arc`-shared `high_water` is captured, guard released, then INSERT) — pattern #3753 (use the captured snapshot, never hold/re-acquire the lock across a new step).
3. DB-error path: a failing INSERT proceeds non-blocking; the compaction ACK is never blocked and the handler never panics (ADR-007 §5). Assert a missing row surfaces as fail-loud absence at crt-055, not a handler failure. **The failure must increment a named metric/counter (R-15), not only emit a generic log** — see R-15 scenario 1.

**Coverage Requirement**: AC-04 — a documented lock-ordering review in ARCHITECTURE.md *plus* a concurrency test driving compaction under contention without deadlock/timeout. Evidence: #3753 (pre-cloned snapshot, never re-acquire under a strict order), #3782 (separate hot-path vs policy locks).

### R-04: Schema-version sequencing collision with crt-055 (Critical) — AC-01 / NFR-8 / SR-04
**Severity**: High · **Likelihood**: Med
**Impact**: crt-054 (`compaction_events`) and crt-055 (`cycle_review_index`) both claim the next `CURRENT_SCHEMA_VERSION` (28→29). They migrate **different tables**, so there is no table-content collision — but the global version *number* is a shared counter. Whichever merges second must retroactively become 30; if not reconciled, two features ship claiming 29 (gap/duplicate), and the migration upgrade path corrupts. This is the crt-043/crt-041 failure (#4095) and the ADR-017-003 (#760) precedent.

**Test Scenarios**:
1. Migration test on a fresh DB: `compaction_events` present via `pragma_table_info`; index on `session_id` present; cascade-file existence asserted (#4484).
2. Migration test on a DB upgraded from v28: the table is added by the upgrade block, existence-guarded (re-run idempotent — `CREATE TABLE IF NOT EXISTS`).
3. Merge-order reconciliation check (SM gate, not unit test): immediately before finalizing, `grep CURRENT_SCHEMA_VERSION migration.rs`; if crt-055 merged first and claimed 29, crt-054's migration block + all artifacts update to 30 before delivery (#4095 mandatory pre-delivery check).

**Coverage Requirement**: AC-01 — both fresh-create and upgrade-from-v28 paths tested; the version number is an SM merge-order coordination point, reconciled at merge. Evidence: #4095, #760, #376 (DDL-before-migration ordering causes post-merge failures).

### R-05: crt-055 producer-contract drift (High) — SR-07
**Severity**: High · **Likelihood**: Med
**Impact**: crt-055 §"Producer contract" is binding for every field crt-054 writes; both design in parallel. A unilateral change to a field width, the `class_counts` index order (`0=error, 1=refusal`), the `[transcript_signals]` default catalog, or `MAX_SIGNAL_CLASSES` diverges producer and consumer — columns land mis-typed, or class indices mean different things on each side.

**Test Scenarios**:
1. Contract-conformance test: `ActivitySnapshot` field set + widths + `class_counts` index mapping match crt-055's contract exactly (`error→0`, `refusal→1`, v1 set).
2. `compaction_events` column names/types/index match the contract verbatim (`id`, `session_id` TEXT NOT NULL, `compacted_at` INTEGER NOT NULL, `high_water` INTEGER NOT NULL DEFAULT 0).
3. Cross-feature alignment gate (process, not test): any field/width/catalog change is negotiated in crt-055's contract *first*; the catalog default set is aligned jointly with crt-055's design session (Open Q2).

**Coverage Requirement**: AC-10 (catalog/index mapping) + AC-08 (snapshot shape). Treat the contract as a single source, not a copy.

### R-06: Single-event / per-turn-drain dependence reintroduced (High) — AC-13 / NFR-9 / SR-02
**Severity**: High · **Likelihood**: Low
**Impact**: The originating failure (#5025/#750): a metric gated on `PreToolUse` collapsed to 0 when the TS client stopped emitting it. Any crt-054 output that depends on a single hook-event class — or on per-turn registry state that the per-turn drain empties (#4799) — is the same trap.

**Test Scenarios**:
1. Design/code review asserting neither surface reads `PreToolUse` or any single-hook-event presence; both derive from the delta stream (Surface B) or the server-authoritative compaction seam (Surface A).
2. Covered transitively by AC-06: if the fold survives the held route (which exists *because* of the per-turn drain), it does not depend on per-turn registry presence.

**Coverage Requirement**: AC-13 — structural review + AC-06 route coverage. Evidence: #5025, #4799, #699 (silent data orphaning when a pipeline hardcodes absence).

### R-07: Producer→consumer integer-width contract (Medium) — AC-14 / NFR-5 / SR-03
**Severity**: Med · **Likelihood**: Low
**Impact**: `bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; N]` land in crt-055's `i64` columns. A high-byte cycle near `u32`/`i64` limits could wrap or truncate silently, corrupting the throughput signal. **Clean ownership split (reaffirmed):** the **producer side (`activity_snapshot()`) is cast-free** — it carries native `u64`/`u32` widths and performs NO integer cast (no `as`, no narrowing) toward `i64`; the checked/saturating `→ i64` conversion is **crt-055's, at persist (`store_cycle_review()`)**. A producer-side `as` cast would silently truncate before crt-055's guard ever runs, defeating it.

**Test Scenarios**:
1. **Producer cast-free assertion (crt-054-owned)**: assert `activity_snapshot()` and the `ActivitySnapshot` accessors perform no `as`/narrowing integer cast — the snapshot fields are exposed at their native `u64`/`u32` widths. A grep/structural check that no `as i64`/`as i32` appears on the producer path satisfies the lower-bound; pair with a value-level assertion that a near-`u64::MAX` `bytes_total` round-trips through the snapshot un-narrowed.
2. Boundary-value test near `u32::MAX` and `i64::MAX` at the seam: assert the checked/saturating conversion (not a silent `as`) lives on crt-055's persist side; the producer hands over the full-width value.
3. Assert `class_counts: [u32; N]` elements also reach the seam without producer-side narrowing.

**Coverage Requirement**: AC-14 — producer-side cast-free assertion (crt-054) PLUS the boundary-value test at the producer→consumer seam (the saturating conversion is crt-055's). The crt-054 half is the no-producer-casts assertion.

### R-08: Late-bind attribution fabricates a zero (Medium) — AC-03 / AC-12 / SR-09
**Severity**: Med · **Likelihood**: Med
**Impact**: Undeclared sessions purge at drain — the fold dies (correct fail-loud). The risk is crt-054 contributing a fabricated `0` for that session to the activity read, making genuine absence look like a measured zero-activity cycle. crt-055 surfaces absence via a `raw_signals_available`-style flag; crt-054 must never fabricate the zero.

**Test Scenarios**:
1. An undeclared session contributes **no entry** to the activity read set — assert its bytes do not appear and no zero is fabricated on its behalf (AC-12).
2. The `compaction_events` row is written for an undeclared session *regardless* (session-keyed at the handler) and carries no `feature_cycle`/content column — Surface A and Surface B are independent paths (AC-03).
3. Absence is distinguishable from a measured `0` at the producer boundary.

**Coverage Requirement**: AC-03 + AC-12 — integration tests proving Surface A row-on-undeclared and Surface B absence-not-fabricated-zero.

### R-09: Surface A row written while a buffer lock is held (Medium) — AC-04
**Severity**: Med · **Likelihood**: Med
**Impact**: A subtler R-03: capturing `high_water` requires the buffer lock; if that guard is not dropped before the INSERT, the DB write runs under the buffer lock and can contend with delta merges. ADR-007: read `high_water()`, drop the guard, then INSERT.

**Test Scenarios**:
1. Assert the `high_water` read and the INSERT do not overlap in lock scope (review + concurrency test from R-03 covers this).
2. A delta merge concurrent with a compaction INSERT does not block on the buffer lock for the duration of the DB write.

**Coverage Requirement**: Folded into AC-04 coverage; called out separately so the `high_water`-capture lock scope is explicitly reviewed (pattern #3753).

### R-10: `[transcript_signals]` config silent fallback (Medium) — AC-11 / SR-05
**Severity**: Med · **Likelihood**: Low
**Impact**: An invalid regex or a class set exceeding `MAX_SIGNAL_CLASSES` must fail **loud at load** (dsn-001 / #4591 precedent), not silently drop classes or wedge at runtime. A silent fallback means signatures stop counting with no signal — a quiet believable-zero on `class_counts`.

**Test Scenarios**:
1. Over-limit class set fails `validate()` with a clear error; no silent truncation to `MAX_SIGNAL_CLASSES`.
2. Unparseable regex fails `validate()` loudly at load; no runtime fallback.
3. Default config yields exactly the v1 `error`/`refusal` set at fixed indices, with `#[serde(default)]` parsing.

**Coverage Requirement**: AC-11 (negative config tests) + AC-10 (default set). Evidence: #2577 (boundary/validate tests must ship in the same pass).

### R-11: `compacted_at` granularity mismatch / cross-gate boundary (Medium) — Open Q3 (RESOLVED) / AC-02
**Severity**: Med · **Likelihood**: Low
**Impact**: `compacted_at` is Unix **seconds** (ADR-007 §4 DDL comment); crt-055 gates PostToolUse read `ts` (epoch **millis**, `session_metrics.rs:115`) against it. Q3 is resolved: **`compacted_at` stays SECONDS at the seam; the `ts/1000` normalization is crt-055's at the gate** (crt-055 Binding constraint 8). crt-054 supplies seconds + rows and does no reckoning. If the seam's clock source or unit drifts from the documented seconds contract, the producer→consumer gate comparison is corrupted at the boundary — a reload just after a compaction is mis-classified as pre-compaction (or vice-versa).

**Test Scenarios**:
1. Assert `compacted_at` is written in seconds (not millis), within tolerance of `now`, from a clock source consistent with the PostToolUse `ts` crt-055 gates against (`now_secs()`/`.as_secs()`, ADR-007 §4).
2. A second compaction on the same session writes a second row with a later `compacted_at` (monotonic, 0..N rows).
3. **Cross-gate boundary integration test (Q3, co-owned with crt-055)** — across the producer→consumer seam, drive a compaction at a known `compacted_at`, then exercise two reads: one whose `ts` falls just AFTER the boundary MUST classify as **post-compaction**, one just BEFORE as **pre-compaction**. The test must apply crt-055's `ts/1000` normalization at the gate (proving the unit contract holds end-to-end: producer seconds vs consumer-normalized millis), and assert the off-by-one-second window resolves correctly. This is the seam where the schema/contract risks (R-04, R-05) and the granularity risk converge into a single observable behavior.

**Coverage Requirement**: AC-02 — assert seconds-granularity, the 0..N multi-row contract, AND the cross-gate boundary classification (pre/post) across the producer→consumer seam with `ts/1000` applied gate-side. The boundary test is co-owned with crt-055 (crt-054 owns the seconds-producer half; crt-055 owns the normalization-and-gate half); neither side may land its half as a unit-only test. Cross-check unit consistency against crt-055's gate, not only the contract doc.

### R-12: Stale-scope residue re-imported (Medium) — AC-15 / SR-05 / SR-06
**Severity**: Med · **Likelihood**: Med
**Impact**: Prior wider-scope ADRs (#5006 ADR-008, residue in #4999/#5002/#5007) carry `saw_compaction`/`reload` latches, a `reread`/`compaction` regex class, `token_bytes_per_unit`, and `cycle_review_index` ownership. Designing/implementing against them re-introduces removed scope and re-opens the bytes-vs-token contradiction.

**Test Scenarios**:
1. Structural/grep-level test: crt-054's diff touches neither `cycle_review_index` nor `SUMMARY_SCHEMA_VERSION` and introduces no token-named symbol (`token_*`).
2. `ActivitySnapshot` has no `saw_compaction`/`reload_after_compaction` latch field; `[transcript_signals]` has no `reread`/`compaction` class.

**Coverage Requirement**: AC-15 — negative/structural review + grep-level test. Architect `context_correct`s #5006; regenerated ADRs supersede residue (#5007 already deprecated).

### R-13: `high_water` over-trusted as wire-precise (Low) — SR-10
**Severity**: Low · **Likelihood**: Low
**Impact**: `high_water` is server-captured at the handler, not wire-exact (vnc-036 shelved). A future precise byte-boundary gate that treats it as wire-precise mis-gates. Accepted for v1 (populating now avoids a second migration); risk is downstream over-trust.

**Test Scenarios**:
1. `high_water` is populated on every row (non-default where the buffer has sent bytes) — assert the value equals the buffer's `high_water()` at compaction.
2. Documentation assertion: the reserved/server-captured semantics are recorded so crt-055/future gating does not over-trust it.

**Coverage Requirement**: AC-02 (`high_water` value correctness) + documented semantics. Accepted-risk; no mitigation beyond documentation for v1.

### R-14: Multi-compaction row semantics (Low) — AC-02
**Severity**: Low · **Likelihood**: Low
**Impact**: A session compacting N times writes N rows (0..N). A producer assumption of one-row-per-session (e.g. an UPSERT) would collapse the history and break crt-055's boundary selection.

**Test Scenarios**:
1. Two compactions on one session write two distinct rows (insert-only; no UPDATE/DELETE path exists).

**Coverage Requirement**: AC-02 — assert the second compaction adds a second row.

### R-15: INSERT-failure silent undercount (High) — AC-04 / ADR-007 §6 / recommendation 1
**Severity**: High · **Likelihood**: Low
**Impact**: The INSERT is non-blocking on error (R-03) — correct for the hot path, but a failed write silently drops a compaction event and undercounts the compaction tax crt-055 computes. With only a `tracing::warn!`, systematic loss (a degraded store, a permission fault) is invisible: the gate signal erodes with no alarm. ADR-007 §6 resolves this by requiring a **named metric/counter** on INSERT failure, not a generic log line, and notes crt-055 can cross-check the row-derived `compaction_count` against the in-memory `increment_compaction` count to detect drift downstream.

**Test Scenarios**:
1. **Forced-failure test (mandatory)**: inject a store INSERT failure at the seam; assert (a) a **named counter** (e.g. `compaction_events_insert_failed`) increments by exactly 1 — a generic log assertion does NOT satisfy this; (b) the compaction path proceeds **non-blocking** (ACK completes, no panic); (c) no `compaction_events` row lands for that event.
2. The counter is content-free (ids/counts only, ADR-005/§7) — no transcript bytes in the metric label or log.
3. Downstream-detector note (crt-055-owned, not a crt-054 test): the row-derived `compaction_count` vs `increment_compaction` drift-check is crt-055's; crt-054 only guarantees the counter exists and increments so the drift is observable.

**Coverage Requirement**: AC-04 — forced-failure test asserting the named counter increments AND the path proceeds non-blocking. The metric (not a log) is the load-bearing assertion. Evidence: #5025/#750 silent-degradation class (here an undercount rather than a zero).

---

## Integration Risks

The two cross-component seams own the Critical/High risks:

- **The `listener.rs:1854` write seam (Surface A → store)** — R-03, R-09, R-11, R-15. New DB acquisition inside a lock-heavy handler. Lock ordering (ADR-007: INSERT after `increment_compaction`, no lock across it; pattern #3753) and clock-unit consistency with crt-055's gate are the failure points. The INSERT is non-blocking-on-error so a store failure degrades to fail-loud absence, never a handler stall — but the failure must increment a **named counter** (R-15), not just log, so a systematic undercount is detectable and crt-055 can flag row-vs-`increment_compaction` drift. The pre/post-boundary classification (R-11) is exercised end-to-end as a cross-gate integration test co-owned with crt-055.
- **The `activity_snapshot()` → crt-055 read seam (Surface B → review)** — R-01, R-02, R-05, R-07, R-08. Routing (held route), sequencing (read-before-purge), width conversion, contract conformance, and attribution honesty all live here. This is where #750/#5025 was reborn-risk and where the mandatory regression guard (AC-06) and read-before-purge test (AC-07) concentrate coverage.
- **The schema-version seam (shared global counter)** — R-04. Disjoint tables but a shared `CURRENT_SCHEMA_VERSION`; an SM merge-order coordination point, not a code seam (#4095).
- **The crt-052 Wave B dependency (NFR-7)** — Surface B survival rests on the hold being ON/non-disableable; ADR-010 asserts this at startup (fail-loud if the `HeldBufferScan` handle is unwired). A regression making the hold disableable breaks the fold silently — a hard dependency, not a code path crt-054 owns.

## Edge Cases

- **Drained-then-redelivered delta** — a delta arriving on the held `Arc` after drain folds into the same accumulator (R-01 scenario 2). The continuity across the drain boundary is the edge, not just non-zero on each side.
- **Session compacting N times** — N rows, monotonic `compacted_at` (R-14, R-11 scenario 2).
- **Undeclared session** — Surface A row written (session-keyed), Surface B fold dies, no fabricated zero (R-08).
- **Poisoned buffer mutex** — `activity_snapshot()` degrades to empty (#4764), same as `snapshot()`; this empty must be distinguishable from a real zero at crt-055 (absence flag), not silently summed.
- **Delta matching multiple classes** — one shared scan increments multiple `class_counts` in a single pass (AC-09); not one pass per pattern.
- **Counters near `u32::MAX` / `i64::MAX`** — checked/saturating conversion (R-07).
- **INSERT fails (DB error)** — named counter increments, handler proceeds non-blocking, missing row = fail-loud absence at review (R-15, R-03 scenario 3).
- **Read just before vs just after a compaction boundary** — a read whose `ts` falls one second after `compacted_at` classifies post-compaction; one second before, pre-compaction (R-11 cross-gate test, `ts/1000` applied gate-side).
- **Empty-delta / zero-length delta** — `delta_count` increments, `bytes_total += 0`; assert no panic and no spurious class match.

## Security Risks

Untrusted external input reaches crt-054 at two points; both are content-bearing, so content-opacity is the primary control.

- **Transcript delta bytes (Surface B input)** — untrusted agent/model conversation content streamed into `apply_delta`. **Blast radius is bounded by construction**: the fold is integer arithmetic + one `RegexSet` scan; `ActivitySnapshot` is structurally incapable of carrying transcript bytes (no `Vec<u8>`/`String`/`&[u8]` field), metadata-only `Debug`, no `Display` (NFR-1, AC-08, #4740). The risk is a regression adding a content-bearing field or a `Display`/`tracing` of delta bytes — leaking conversation content (which may contain secrets). Mitigation: AC-08 structural content-opacity test (mirrors `test_candidates_structurally_absent`). No injection/path/deserialization surface — the fold neither parses structure nor touches the filesystem.
- **`[transcript_signals]` config patterns (regex)** — operator-supplied regex compiled into a shared `RegexSet`. Risk: a catastrophic-backtracking ("ReDoS") pattern run per-delta on the hot path. `RegexSet` (Rust `regex`, linear-time, no backtracking) structurally bounds this; `validate()` rejects unparseable patterns loudly at load (AC-11). Residual: an operator-authored pathological-but-valid pattern; bounded by `MAX_SIGNAL_CLASSES` and linear-time matching. Not externally exploitable (config is operator-trusted).
- **`session_id` written to `compaction_events` (Surface A)** — written via the store's parameterized INSERT (no string interpolation); no SQL-injection surface. Content-free row: no payload, no path, no deserialization.

Blast radius if Surface B's accumulator were compromised: a corrupted *count* (informs, never controls — RQ-8); it cannot bill, schedule, or block execution. No content escape given AC-08 holds.

## Failure Modes

| Failure | Designed behavior | Verified by |
|---------|-------------------|-------------|
| Surface A INSERT fails (DB error) | **named counter** (`compaction_events_insert_failed`) increments (ids/counts only, not just a log); handler proceeds; compaction ACK never blocked; missing row = fail-loud absence at crt-055; row-vs-`increment_compaction` drift detectable downstream | R-15, R-03 sc.3, AC-04 |
| Buffer mutex poisoned | `activity_snapshot()` returns empty (#4764); crt-055 sees absence, not a fabricated zero | edge case, AC-12 |
| Invalid / over-cap config | Fail **loud at startup** (`validate()`); no runtime fallback | AC-11, R-10 |
| Wave B handle unwired | Fail **loud at startup** (ADR-010); not a silent degrade | NFR-7, R-02 |
| Undeclared session (fold dies) | Correct fail-loud; no fabricated zero; Surface A row still written | AC-03/AC-12, R-08 |
| Held route stops feeding the fold | Regression test fails **red** (AC-06), not silent zero | R-01 |
| Read ordered after purge | Read-before-purge test fails **red** (AC-07) | R-02 |

The governing failure posture: **every absence is fail-loud, never a believable zero.** The originating defect (#750/#5025) was a silent zero; crt-054's whole reason to exist is to make that class red-on-regression.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 lock ordering at write seam | R-03, R-09 | ADR-007: INSERT after `increment_compaction`, no lock held across it; `high_water` captured then guard dropped (#3753). Verified by AC-04 concurrency test + documented lock-ordering review. |
| SR-02 held-route believable-zero | R-01, R-06 | ADR-001 (fold on both routes by construction — accumulator in the buffer) + ADR-009 (held-route regression guard). Verified by AC-06 (mandatory) + AC-13. |
| SR-03 integer-width truncation | R-07 | ADR-003/§6: producer side is **cast-free** native `u64`/`u32`/`[u32;N]`; crt-055 owns the checked/saturating `→i64` conversion at persist. Verified by AC-14 = producer no-cast assertion (crt-054) + boundary-value test (crt-055 side). |
| SR-04 schema-version sequencing collision | R-04 | ADR-008: distinct sequential versions, disjoint tables, merge-order reconciled at merge (SM gate, #4095). Verified by AC-01 fresh + upgrade paths. |
| SR-05 stale-knowledge residue | R-12, R-10 | Full ADR regeneration; `context_correct` of #5006; #5007 deprecated. Verified by AC-15 structural/grep test + AC-11. |
| SR-06 token/cost exclusion drift | R-12 | ADR-005/ADR-002: bytes-only, no token-named field. Verified by AC-15 (no `token_*` symbol). |
| SR-07 producer-contract coupling | R-05 | §5: contract is single source; field/catalog changes negotiated in crt-055 first; catalog aligned jointly (Open Q2). Verified by AC-08 + AC-10 conformance. |
| SR-08 survival-to-review on the hold | R-02 | ADR-006 (never zero/drop) + ADR-010 (Wave B startup precondition, fail-loud). Verified by AC-07 read-before-purge. |
| SR-09 cycle-declaration coverage gap | R-08 | ADR-004: late-bind, never fabricate a zero; Surface A row declaration-independent. Verified by AC-03 + AC-12. |
| SR-10 vnc-036 shelving / `high_water` reserved | R-13 | ADR-007: `high_water` server-captured, documented reserved, not over-trusted. Accepted for v1 — populated to avoid a second migration; verified by AC-02 value + documented semantics. |

Every SR-01..SR-10 maps to at least one architecture risk and is resolved by an ADR + verified by an AC. No scope risk is dropped.

---

## Coverage Summary

| Priority | Risk Count | Risk IDs | Required Scenarios | Anchor ACs |
|----------|-----------|----------|-------------------|-----------|
| Critical | 4 | R-01, R-02, R-03, R-04 | ~10 (held-route guard, read-before-purge, concurrency/deadlock, fresh+upgrade migration) | AC-06, AC-07, AC-04, AC-01 |
| High | 3 | R-05, R-06, R-15 | ~7 (contract conformance, no single-event dependence, forced INSERT-failure named-counter) | AC-08, AC-10, AC-13, AC-04 |
| Medium | 6 | R-07, R-08, R-09, R-10, R-11, R-12 | ~13 (producer no-cast + width boundary, attribution honesty, config validate, cross-gate boundary classification, residue grep) | AC-14, AC-03, AC-12, AC-11, AC-02, AC-15 |
| Low | 2 | R-13, R-14 | ~3 (high_water value/semantics, multi-row) | AC-02 |

**The four Critical risks are the believable-zero family at two seams (routing: R-01/AC-06; sequencing: R-02/AC-07), the lock graph at the INSERT seam (R-03/AC-04), and the shared schema-version counter (R-04/AC-01).** AC-06 and AC-07 must be held-route, drain→hold→review integration tests on the cumulative crt-052/vnc-025 fixtures — a registered-only or unit-only test does NOT satisfy them and is the exact way the #750/#5025 class gets through; the R-01 guard additionally carries a mandatory negative-mutation check so it cannot degrade into a no-op.

**Open-question resolutions encoded (2026-06-16):** (Q3) the cross-gate boundary integration test (R-11 sc.3, co-owned with crt-055) — `compacted_at` stays seconds, `ts/1000` is crt-055's at the gate; (rec-1) the INSERT-failure named-counter requirement (R-15, raised to High) replacing the prior log-only posture; (R-01/AC-06) reaffirmed held-route exercise on a representative TS-client cycle, no-op-prohibited; (R-07) the producer cast-free contract with the saturating conversion owned by crt-055.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` for believable-zero/held-route lessons, schema-version sequencing, lock-ordering patterns, single-event regression. Load-bearing evidence: #5025 (TS client retired PreToolUse → per-session metrics collapsed to 0 — the originating #750), #4799 (per-turn drain starves review-time consumers — why the held route exists), #4095 (parallel-merge schema-version collision — SR-04/R-04), #760 (independent migration versioning precedent), #3753 (pre-cloned lock snapshot, never re-acquire — SR-01/R-03), #3782 (separate hot-path vs policy locks), #5007 (prior ADR-009 believable-zero guard, now deprecated — reasoning superseded by regenerated ADR-009), #4764/#4740 (poison→empty, content-opacity by construction).
- Stored: nothing novel to store. The recurring patterns this feature exercises are already captured (#4095 schema collision, #3753 lock snapshot, #5025/#4799 believable-zero family). No cross-2+-feature risk pattern emerged that is not already in Unimatrix; storing a feature-specific risk would violate the steward boundary.
