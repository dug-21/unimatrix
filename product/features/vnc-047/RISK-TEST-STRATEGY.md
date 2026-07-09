# Risk-Based Test Strategy: vnc-047

> `context_cycle` set-once opaque tags → `cycle_tags` junction → surfaced in `context_cycle_review`.
> Tracks GH #940. Mode: architecture-risk. Inputs: SCOPE.md, SCOPE-RISK-ASSESSMENT.md (SR-01…SR-10),
> ARCHITECTURE.md (C1–C11, ADR-001…006), SPECIFICATION.md (FR-1…11, AC-01…08).
> Historical evidence cited by Unimatrix entry ID.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `CURRENT_SCHEMA_VERSION` 30→31 cascade incomplete — a schema path (fresh-create, migration, or pinned test) missed; `cycle_tags` absent on one DB-init route | High | High | **Critical** |
| R-02 | `SUMMARY_SCHEMA_VERSION` 5→6 cascade incomplete — constant, three serde/round-trip paths, or pinned test (#4178/#5051) not all updated together | High | High | **Critical** |
| R-03 | Assembled path proven only by store-only structural tests — `cycle_tags` insert/get tested directly but MCP→hook→listener→`cycle_review` chain never driven (SR-08) | High | High | **Critical** |
| R-04 | Tags silently dropped on absent/evicted SM session — fire-and-forget path inherits the #4140/#981/#4134 silent-loss failure mode; #519 pre-register (Step 1b) not exercised | High | Med | **High** |
| R-05 | Transaction not atomic — `cycle_start` event row, the whole-set EXISTS guard, and tag rows in `insert_cycle_start_with_tags` commit separately; a partial failure leaves tags without a start row or vice versa | High | Med | **High** |
| R-06 | Second persistence route regression — a caller reaches the MCP handler (which persists nothing, tools.rs:4062) or someone wires `CycleParams.tags` to persist, diverging from the hook-only route | High | Low | **High** |
| R-07 | GC purges `cycle_tags` — table not omitted from every DELETE in `gc_cycle_activity` (:116) AND `gc_unattributed_activity` (:202); regression test not extended (ADR-005 by-omission) | High | Med | **High** |
| R-08 | Whole-set-once reads as data loss — the first tag-bearing start LOCKS the entire set; any later start (same/subset/superset/different) is a whole-set no-op (SR-05); undocumented/untested it looks like a bug | Med | Med | **Medium** |
| R-09 | 15 untouched `insert_cycle_event` call sites regress — the new primitive branches only Start-with-tags; a routing error sends non-start or no-tag events down the wrong arm | Med | Med | **Medium** |
| R-10 | Version-number collision at merge — v31 or SUMMARY v6 claimed by a parallel feature (#4095); numbers verified at design not implementation | Med | Med | **Medium** |
| R-11 | Empty-tag / opacity leak — empty-string tag not rejected, or engine derives/validates namespace, breaking value-opacity (FR-2, AC-01/07) | Med | Low | **Medium** |
| R-12 | Markdown render divergence — spurious `## Tags` on a tag-less cycle, or tester asserts against the ARCHITECTURE diagram's illustrative `## Tags` string rather than the spec/`render_goal_section` parity (#3337) | Low | Med | **Low** |
| R-13 | Migration not idempotent / not old-DB-safe — re-run or upgrade-from-v30 fails without the `sqlite_master`/`pragma_table_info` guard; tested only on fresh DB (#378, #376) | Med | Low | **Medium** |
| R-14 | No back-fill misread as regression — pre-v6 cached reviews render empty `## Tags` forever (SR-10); staleness advisory-only, no recompute | Low | Low | **Low** |
| R-15 | Whole-set EXISTS-guard TOCTOU — two concurrent starts for the same `feature_cycle` both pass the EXISTS check and both write, or interleave, breaking whole-set-once. `sqlx pool.begin()` is DEFERRED; the guard is race-safe only under `BEGIN IMMEDIATE` (ADR-002 §3) | High | Low | **High** |
| R-16 | Best-effort ack echo drift — the `context_cycle` ack accept/ignored notes or the listener wrote-set/frozen-skip tracing line missing or wrong (FR-12/AC-09). NON-GATING, best-effort SHOULD | Low | Low | **Low** |

---

## Risk-to-Scenario Mapping

### R-01: `CURRENT_SCHEMA_VERSION` 30→31 cascade incomplete
**Severity**: High · **Likelihood**: High · **Impact**: `cycle_tags` missing on one init route → every insert/read for tags fails at runtime on that route (fresh install OR upgraded DB), or the pinned schema-version test fails the gate. This exact class is the recurring gate miss in this codebase (#4153, #4373, #4095).

**Test Scenarios**:
1. Fresh-create: initialize a v31 DB from `create_tables_if_needed`; assert `cycle_tags` table exists with PK `(feature_cycle, tag)` and `idx_cycle_tags_tag` index present (AC-03b).
2. Migration: initialize a v30 DB, run migration to v31; assert `cycle_tags` + index created and `CURRENT_SCHEMA_VERSION` constant reads 31 (AC-03a/c).
3. Pinned schema-version / migration-hygiene test updated and green (AC-03d).
4. Cross-check: fresh-create schema and post-migration schema are structurally identical (same DDL both paths) — guards DDL drift between the two routes (#376).

**Coverage Requirement**: All three schema paths (constant, fresh-create, migration) plus the pinned test proven by discrete assertions — not one lumped "bump" assertion. AC-03a–d each individually asserted.

### R-02: `SUMMARY_SCHEMA_VERSION` 5→6 cascade incomplete
**Severity**: High · **Likelihood**: High · **Impact**: `RetrospectiveReport.tags` fails round-trip through `summary_json` on a path that wasn't bumped; or the pinned `SUMMARY_SCHEMA_VERSION` test (#4178, #5051) fails the gate. This is a **separate cascade** from R-01 (no DB migration — a fidelity stamp) and must not be lumped with it.

**Test Scenarios**:
1. Constant advanced 5→6 at `cycle_review_index.rs:54` (AC-05a).
2. `RetrospectiveReport` with populated `tags` serializes → `summary_json` → deserializes with tags intact across all three schema paths (AC-05b).
3. Pinned `SUMMARY_SCHEMA_VERSION` test updated and green (AC-05c).
4. Backward-compat: a v5 `summary_json` blob (no `tags` key) deserializes via `#[serde(default)]` to an empty `tags` vec without error (guards NFR-8 / AC-08 read path).

**Coverage Requirement**: Constant + all three round-trip paths + pinned test each asserted discretely. The `#[serde(default)]` backward-read case is mandatory (it is what makes no-back-fill non-fatal).

### R-03: Assembled path proven only by store-only structural tests
**Severity**: High · **Likelihood**: High · **Impact**: Tests insert into `cycle_tags` and read via `get_cycle_tags` directly and pass, while the real `context_cycle`→hook→listener→`cycle_review` chain silently carries nothing — the exact SR-08 recurring gate miss (#3935 class). Feature ships broken with green tests.

**Test Scenarios**:
1. Drive assembled `context_cycle(type=start, topic=FC, tags=[...])` through `build_cycle_event_or_fallthrough` → `RecordEvent` → `handle_cycle_event`; then read via `context_cycle_review(FC)` and assert tags appear in BOTH JSON and markdown output (AC-02, AC-05).
2. Assembled non-start event carrying `tags` → assert tags NOT persisted (FR-4 via the assembled path, not a unit stub).
3. Assembled duplicate start → assert no duplicate rows, no error surfaced.

**Coverage Requirement**: AC-02 and AC-05 `done_when`/`proven_by` MUST cite an assembled-path test that drives the MCP-through-review chain. A store-only test may supplement but cannot satisfy these ACs. Reuse existing hook/listener integration fixtures (test infra is cumulative) — do not build isolated scaffolding.

### R-04: Tags silently dropped on absent/evicted SM session
**Severity**: High · **Likelihood**: Med · **Impact**: When the SM session is evicted/absent, the hook path historically no-ops cycle attribution silently (#4140 major, #4134, #981). A `cycle_tags` insert on the same path inherits this: a start's tags vanish with no error, no `## Tags` at review — indistinguishable from "no tags supplied." Directly relevant historical evidence: #4140, #4134, #981; mitigation precedent #4136 (Step 1b pre-register).

**Test Scenarios**:
1. Assembled start with tags on a session that is absent from the registry (evicted) → assert the #519 pre-register (Step 1b) fires and tags still land in `cycle_tags` for the correct `feature_cycle` (NFR-6, ADR-003).
2. Start with tags but NULL/empty `feature_cycle` in payload → assert persistence is gated off (no orphan rows), consistent with `!feature_cycle.is_empty()` (ADR-003) — and that this is the ONLY drop condition.
3. DB error inside the fire-and-forget spawn → assert `tracing::warn` emitted, review still succeeds (degrades), no panic in the spawned task.

**Coverage Requirement**: The absent/evicted-session case MUST be exercised on the assembled path (not asserted by inspection). Empty `feature_cycle` is the single documented silent-drop; every other input must persist.

### R-05: Transaction not atomic (start row + EXISTS guard + tag rows)
**Severity**: High · **Likelihood**: Med · **Impact**: If `insert_cycle_start_with_tags` does not wrap the `cycle_start` event insert, the whole-set EXISTS guard, and per-tag inserts in one transaction (ADR-002), a mid-op failure yields tags without a start row, or a start row without its tags — corrupting the "tags belong to this run" invariant and the durability story. The EXISTS guard and the multi-row insert must be one atomic unit or the whole-set-once lock (R-08) is not enforceable.

**Test Scenarios**:
1. Assert the `cycle_start` `cycle_events` row and all `cycle_tags` rows are visible after one call — same commit boundary.
2. Fault-injection (or code review + a targeted test): a failing tag insert must roll back the start-row insert (or the whole op is one atomic unit) — no half-written state.
3. `ON CONFLICT(feature_cycle, tag) DO NOTHING` applies per-tag within the txn — a duplicate tag in the same call does not abort the transaction.

**Coverage Requirement**: One-transaction boundary (guard + start row + tag rows) verified. At minimum a test asserting co-presence of start row + tag rows; atomicity-under-failure verified by test or explicit code-review sign-off in the coverage report. Concurrency of the guard is R-15.

### R-06: Second persistence route regression
**Severity**: High · **Likelihood**: Low · **Impact**: The bare MCP `context_cycle` handler persists nothing (tools.rs:4062, :4128). If tags are ever wired to persist from `CycleParams` in the handler, OR a second write route is introduced, the set-once/same-txn invariant and idempotency break (SR-03, ADR-002).

**Test Scenarios**:
1. Call the bare MCP `context_cycle` handler directly (no hook) with tags → assert NOTHING persists to `cycle_tags` (the handler is session-unaware by design).
2. Diff/grep review: `cycle_tags` INSERT appears only in `insert_cycle_start_with_tags`, reached only from `handle_cycle_event` Step-5 Start branch — no other writer.

**Coverage Requirement**: Exactly one persistence route asserted. `CycleParams.tags` exists to declare the interface (AC-06); a test must confirm the value stored is read from `tool_input["tags"]` by the hook, not from the handler.

### R-07: GC purges `cycle_tags`
**Severity**: High · **Likelihood**: Med · **Impact**: ADR-005 protects by **omission** — `cycle_tags` must be absent from every DELETE path. Omission is silent: if a future DELETE touches it, or the current design overlooked `gc_unattributed_activity` (:202), tags vanish on the next purge. Load-bearing mitigation = extending `test_gc_protected_tables_regression` (:521).

**Test Scenarios**:
1. Extend `test_gc_protected_tables_regression`: seed `cycle_tags`, make the cycle purgeable (has a `cycle_review_index` row), run a full `gc_cycle_activity` pass → assert `cycle_tags` rows unchanged (AC-04).
2. Cover `gc_unattributed_activity` (:202) as well as `gc_cycle_activity` (:116) — both DELETE surfaces named in the integration surface.
3. Contrast assertion: `sessions` rows for the same cycle ARE purged in the same pass — proves the test actually exercised GC, not a no-op.

**Coverage Requirement**: `cycle_tags` proven surviving BOTH GC DELETE surfaces, with a positive control (something is purged) so the test can't pass vacuously.

### R-08: Whole-set-once reads as data loss
**Severity**: Med · **Likelihood**: Med · **Impact**: The first tag-bearing `cycle_start` LOCKS the entire tag set for the `feature_cycle` via an EXISTS guard inside the cycle_start transaction. Every later start — same, subset, superset, or a completely different set — is a **whole-set no-op**; a *tagless* start does NOT lock (a later tag-bearing start can still set the initial set). No tag is ever added or removed after the lock. Intended (SR-05), but undocumented/untested it looks like data loss. Value-opacity is preserved: the guard is set-existence only, no namespace parsing.

**Test Scenarios**:
1. Assembled: start `{A,B}`, then re-start `{C}` → assert stored set is EXACTLY `{A,B}`, no error (AC-02a).
2. Assembled: start `{A}`, then re-start `{B}` → assert stored set is EXACTLY `{A}` (superset/expansion is a whole-set no-op — the new tag is NOT accumulated).
3. Assembled: subset re-start `{A,B}` then `{A}` → EXACTLY `{A,B}`; superset re-start `{A}` then `{A,B}` → EXACTLY `{A}` (both directions no-op wholesale).
4. Tagless start first, then a tag-bearing start `{A}` → assert `{A}` locks (tagless start did not lock the empty set).

**Coverage Requirement**: Whole-set-once asserted as intended with EXACT stored-set equality (not just "prior tags retained") across changed/subset/superset/different re-starts, and the tagless-start-does-not-lock case. No error surfaced on any no-op re-start. This REPLACES the prior per-row first-write-wins/accumulate semantics.

### R-09: The 15 untouched `insert_cycle_event` call sites regress
**Severity**: Med · **Likelihood**: Med · **Impact**: `Store::insert_cycle_event` signature is UNCHANGED (15 call sites preserved). The listener Step-5 spawn branches Start-with-tags → new primitive, else → old. A routing error (e.g. Start-without-tags going to the new arm, or a signature change leaking) breaks existing cycle-event writes.

**Test Scenarios**:
1. Start WITHOUT tags → routed to `insert_cycle_event` (unchanged arm); assert normal cycle_start behavior, no `cycle_tags` rows.
2. Non-start events (phase/outcome/next_phase) → `insert_cycle_event`; assert unchanged.
3. Existing col-025 `goal` end-to-end tests still green (goal rides the same start row via the new primitive when tags present) — assert goal persists whether or not tags are supplied.

**Coverage Requirement**: Both routing arms covered; the existing cycle-event and `goal` regression suites pass unchanged. `goal`-with-tags and `goal`-without-tags both assert goal persistence.

### R-10: Version-number collision at merge
**Severity**: Med · **Likelihood**: Med · **Impact**: A parallel feature merging first claims v31 or SUMMARY v6 (#4095), forcing retroactive renumber across migration + all artifacts + tests.

**Test Scenarios**:
1. Implementation-start gate check: re-verify `CURRENT_SCHEMA_VERSION == 30` and `SUMMARY_SCHEMA_VERSION == 5` at HEAD before writing the bump (NFR-3). Not a runtime test — a coverage-report checklist item.

**Coverage Requirement**: Documented re-verification at implementation start recorded in the coverage report. If either number is taken, flag for renumber before proceeding.

### R-11: Empty-tag / opacity leak
**Severity**: Med · **Likelihood**: Low · **Impact**: Value-opacity is the parity contract (vnc-045 SD-8). If an empty string is accepted, or the engine derives/validates a namespace, opacity breaks.

**Test Scenarios**:
1. Supply `tags=["workflow:v1.3", "", "foo"]` → empty rejected, `workflow:v1.3` and `foo` stored verbatim (AC-01, FR-2).
2. Colon-prefixed vs colon-free tag stored identically with no prefix-based branching (AC-07) — assert no namespace derivation at the cycle-tag write path.

**Coverage Requirement**: Empty-string rejection + verbatim storage + no prefix derivation all asserted.

### R-12: Markdown render divergence
**Severity**: Low · **Likelihood**: Med · **Impact**: Spurious `## Tags` on a tag-less cycle, or a tester asserting against the ARCHITECTURE.md illustrative `## Tags` header string instead of the spec/`render_goal_section` parity contract (#3337 — architecture diagram strings are illustrative, not authoritative).

**Test Scenarios**:
1. Cycle WITH tags → markdown renders the tag section; JSON includes `tags` (AC-05d).
2. Cycle WITHOUT tags → no spurious section rendered (AC-05d, FR-10).
3. Assert against the spec/`render_goal_section`-parity string, treating the ARCHITECTURE.md `## Tags` sample as illustrative only (#3337).

**Coverage Requirement**: Both present-and-absent render cases covered. Tester derives the expected header from SPECIFICATION/`render_goal_section` parity, not the architecture diagram.

### R-13: Migration not idempotent / not old-DB-safe
**Severity**: Med · **Likelihood**: Low · **Impact**: Migration tested only on a fresh DB fails on a populated v30 DB or on re-run without the `sqlite_master`/`pragma_table_info` guard (#378, #376: migration tests must include old-schema DBs, DDL-before-migration ordering).

**Test Scenarios**:
1. Upgrade from a populated v30 DB → `cycle_tags` created, existing data intact (#378).
2. Re-run migration on an already-v31 DB → no error (guard makes it idempotent, AC-03c).

**Coverage Requirement**: Migration proven against a non-empty old-schema DB AND on re-run, not fresh-create only.

### R-14: No back-fill misread as regression
**Severity**: Low · **Likelihood**: Low · **Impact**: Pre-v6 cached reviews (staleness advisory-only) never recompute; historical `## Tags` renders empty forever (SR-10, NFR-8). Only a documentation/expectation risk.

**Test Scenarios**:
1. A v5 cached review surfaces no `tags` without a recompute (optional confirming test, AC-08).

**Coverage Requirement**: Documented expectation in feature docs; optional confirming test. Not a defect.

### R-15: Whole-set EXISTS-guard TOCTOU (concurrent same-cycle starts)
**Severity**: High · **Likelihood**: Low · **Impact**: The whole-set-once lock (R-08) is an EXISTS check followed by a multi-row insert. `sqlx`'s `pool.begin()` opens a **DEFERRED** transaction — under concurrency two starts for the same `feature_cycle` can both read "no tags exist," both pass the guard, and both write (or interleave), producing a merged/duplicated set and breaking whole-set-once. ADR-002 §3 requires `BEGIN IMMEDIATE` so the guard+insert is serialized (write lock acquired at txn start). This is a TOCTOU race, not a logic error — it only manifests under concurrent same-cycle starts.

**Test Scenarios**:
1. Code/SQL review: confirm `insert_cycle_start_with_tags` opens the transaction with `BEGIN IMMEDIATE` (not the default deferred `pool.begin()`), so the EXISTS guard holds a write lock.
2. Concurrency test: fire two same-`feature_cycle` tag-bearing starts (`{A,B}` and `{C,D}`) concurrently → assert the stored set is EXACTLY one of the two whole sets, never a merge (`{A,B,C,D}`) or a partial mix. Whichever wins the immediate-txn race locks; the other is a whole-set no-op.
3. Assert no error/panic surfaced by the losing concurrent start (fire-and-forget degrade parity).

**Coverage Requirement**: `BEGIN IMMEDIATE` verified (review or test), AND a concurrent same-cycle-start test asserting the stored set is exactly one intact whole set — never a merge or partial. This is the atomicity guarantee that makes R-08 enforceable under load.

### R-16: Best-effort ack echo drift (non-gating)
**Severity**: Low · **Likelihood**: Low · **Impact**: The additive `context_cycle` ack echo (FR-12/AC-09) — a "tags accepted for recording" note on a tag-bearing start, a "tags ignored" note on a non-start event — plus a listener tracing line distinguishing wrote-set vs frozen-skip, are best-effort SHOULD affordances for operator visibility. If missing or worded wrong the feature still works; this is NOT a correctness gate.

**Test Scenarios**:
1. Unit/handler test: a start-with-tags ack contains the accept-for-recording note; a non-start-with-tags ack contains the "tags ignored" note (verify the echo strings).
2. Listener emits a tracing line for the wrote-set path and for the frozen-skip path (log-assertion or manual observation).

**Coverage Requirement**: LOW severity, NON-GATING. Verify the echo strings and tracing line; do NOT block delivery on them. The frozen-skip outcome is NOT caller-returnable (the write is fire-and-forget) — do NOT require an assembled-path proof for the frozen-skip case; a listener tracing line is the only observation point.

---

## Integration Risks

- **Hook → listener payload contract (C4→C5):** `payload["tags"]` is a JSON array parity with `payload["goal"]` (hook.rs:877-880). Type mismatch (object vs array, or missing key) must degrade to "no tags," never panic. Covered by R-03 scenario 1 and R-04 scenario 2.
- **Listener Step-5 routing (C5):** the Start-with-tags-vs-else branch is the single decision point that keeps 15 `insert_cycle_event` call sites untouched — R-09.
- **Transaction boundary (C2):** the new primitive folds a whole-set EXISTS guard + a `cycle_events` write + `cycle_tags` writes into one `BEGIN IMMEDIATE` transaction — a cross-table atomicity seam (R-05) that is also the concurrency-safety seam for whole-set-once (R-15).
- **Review read degrade (C8):** `get_cycle_tags` error → `report.tags = []` + warn (parity with `get_cycle_start_goal` degrade at tools.rs:3425); review must never fail on tag read — R-04 scenario 3.
- **`summary_json` mirror vs source of truth:** tags read fresh from `cycle_tags` each review and folded into `report`; the `summary_json` copy is display-only. A read that trusts a stale `summary_json` over `cycle_tags` would be a source-of-truth inversion — assert review reads `cycle_tags`, not the prior mirror.

## Edge Cases

- Empty-string tag in a mixed array (rejected, others stored) — R-11.
- Duplicate tag within one start call (`ON CONFLICT` per-tag, no txn abort) — R-05 scenario 3.
- Re-issued start, changed/subset/superset/different tag set (whole-set no-op, exact-equality) — R-08.
- Tagless start first, later tag-bearing start locks the set — R-08 scenario 4.
- Two concurrent same-cycle tag-bearing starts (exactly one whole set wins, no merge) — R-15.
- Tags on non-start event (ignored) — R-03 scenario 2, R-09 scenario 2.
- NULL/empty `feature_cycle` (single documented drop) — R-04 scenario 2.
- Absent/evicted session (must still persist via #519 pre-register) — R-04 scenario 1.
- Very large tag / many tags — value-opacity means no length cap; assert no truncation (parity with entry-tag opacity; note `goal` has `MAX_GOAL_BYTES` but tags do not).
- Unicode / colon-only / whitespace-only tag — stored verbatim if non-empty (whitespace-only is non-empty → stored; confirm this is intended per FR-2).
- Tag-less cycle at review (no spurious section) — R-12 scenario 2.
- v5 cached review (empty tags, no recompute) — R-14.

## Security Risks

Untrusted input: the `tags: Vec<String>` array supplied on `context_cycle`.
- **Injection:** tags flow into `INSERT … ON CONFLICT` — MUST use parameterized binds (parity with `add_tag` `write.rs:281`), never string interpolation. Assert bound parameters. Because value-opacity forbids validation, parameterization is the *only* SQLi defense — this is load-bearing.
- **LIKE-metacharacter surface (absent by design):** entry tags use `like_escape` for `replace_tag`'s `LIKE 'ns:%'`. vnc-047 ships NO replace/namespace query, so `LIKE`/`like_escape` must NOT appear on the cycle-tag write path. If a future reviewer adds prefix querying, `like_escape` becomes mandatory — flag for the deferred mutation home (ADR-006).
- **Authorization:** single `Capability::Write` gate (FR-7); `agent_id` is audit-only and must NOT authorize or scope the write (parity vnc-045 SD-9). Assert a caller with Write can tag any `feature_cycle` and `agent_id` does not gate.
- **Blast radius:** a compromised/malformed tag input is confined to `cycle_tags` rows for one `feature_cycle`; opaque storage means no code path interprets tag content, so no deserialization/eval surface. Namespace is reader-only convention — no engine parsing = no parser-exploit surface. Blast radius is bounded to display of attacker-controlled strings in `cycle_review` markdown/JSON (a downstream-renderer concern, out of engine scope).
- **DoS:** no length/count cap on tags (value-opacity). A caller could supply enormous/many tags. Accepted risk under the opacity contract + Write-gate; note it so it is a decision, not a surprise.

## Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| DB error during tag insert (fire-and-forget) | `tracing::warn`, spawned task does not panic; no caller-visible error (set-and-forget, ADR-003). Cycle start still recorded per txn semantics (R-05). |
| Absent/evicted session | #519 pre-register (Step 1b) ensures tags still persist to the correct `feature_cycle` (R-04). |
| Empty/NULL `feature_cycle` | Persistence gated off — no orphan rows; the single documented silent drop (ADR-003). |
| `get_cycle_tags` read error at review | `report.tags = []` + warn; review succeeds (degrade parity with `get_cycle_start_goal`). |
| Empty-string tag supplied | Rejected (non-empty check); other tags in the array still stored. |
| Re-issued start (any set) after lock | Whole-set no-op via EXISTS guard — stored set unchanged, no error, no add/remove (R-08). |
| Concurrent same-cycle starts | `BEGIN IMMEDIATE` serializes; exactly one whole set locks, no merge (R-15). |
| Duplicate tag within one start | `ON CONFLICT(feature_cycle, tag) DO NOTHING` — no dup rows, txn not aborted. |
| Pre-v6 cached review | Empty `tags`, no recompute, empty `## Tags` — by design (no back-fill). |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (double schema-version discipline) | R-01, R-02 | Split into two independent cascades (ADR-001 v31 real migration; ADR-004 SUMMARY v6 fidelity stamp). Each proven by discrete per-path line-items (AC-03a–d, AC-05a–d) + pinned test. |
| SR-02 (parallel version collision) | R-10 | Implementation-start re-verification of both numbers at HEAD (NFR-3); coverage-report checklist item, flag renumber. |
| SR-03 (hook-path only; bare MCP persists nothing) | R-06 | ADR-002 forbids a second route; test asserts bare handler persists nothing and stored value is read from `tool_input["tags"]`. |
| SR-04 (payoff external/deferred) | — | Accepted / out of scope. Coverage validates store + surface only; `(tag)` index shaped for deferred query (NFR-7). No cross-run test required. |
| SR-05 (whole-set-once ignores a later changed/expanded set wholesale) | R-08, R-15 | Documented as intended whole-set-once (EXISTS guard inside the cycle_start txn; `BEGIN IMMEDIATE` for race-safety). AC-02a assembled test asserts EXACT stored-set equality across changed/subset/superset/different re-starts (`{A,B}` then `{C}` → `{A,B}`; `{A}` then `{B}` → `{A}`), tagless-start-does-not-lock, and a concurrent-start test (exactly one whole set, no merge). No error on any no-op. |
| SR-06 (one-tag-model parity drift) | R-09, R-11 | Port `add_tag`-style primitive re-keyed to `feature_cycle` (FR-5); opacity tests (R-11) enforce parity with entry-tag semantics; deferred mutation reserved on `context_tag` (ADR-006). |
| SR-07 (fire-and-forget silent-failure history) | R-04, R-05 | ADR-003 + #519 pre-register (Step 1b); absent/evicted-session assembled test mandatory; #4140/#4134/#981 evidence elevates likelihood. |
| SR-08 (structural-only tests pass, real path untested) | R-03 | AC-02/AC-05 marked [assembled-path]; `proven_by` MUST cite a test driving MCP→hook→listener→`cycle_review`. |
| SR-09 (GC-protection registration easy to miss) | R-07 | ADR-005 protection-by-omission; extend `test_gc_protected_tables_regression` across BOTH GC DELETE surfaces with a positive control. |
| SR-10 (pre-existing reviews never show tags) | R-14 | Documented no-back-fill (NFR-8); `#[serde(default)]` makes v5→read non-fatal (R-02 scenario 4); optional confirming test. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 11 (incl. mandatory assembled-path AC-02 + AC-05) |
| High | 5 (R-04, R-05, R-06, R-07, R-15) | 14 |
| Medium | 5 (R-08, R-09, R-10, R-11, R-13) | 11 |
| Low | 3 (R-12, R-14, R-16) | 6 |

**Load-bearing coverage obligations (gate-critical):**
1. Assembled-path tests for AC-02 and AC-05 — `proven_by` must cite the MCP→hook→listener→`cycle_review` chain, not a store getter (R-03/SR-08).
2. Two independent version cascades each proven by discrete per-path assertions + pinned test (R-01/R-02/SR-01).
3. `test_gc_protected_tables_regression` extended across BOTH GC DELETE surfaces with a positive control (R-07/SR-09).
4. Absent/evicted-session persistence exercised on the assembled path (R-04/SR-07).
5. Whole-set-once proven by EXACT stored-set equality across changed/subset/superset/different re-starts + tagless-start-does-not-lock, with `BEGIN IMMEDIATE` and a concurrent same-cycle-start test (no merge) (R-08/R-15/SR-05).

**Explicitly NON-gating:** the best-effort ack echo + listener tracing (R-16/FR-12/AC-09) — verify strings, do not block; the frozen-skip outcome is not caller-returnable, so no assembled-path proof is required for it.
