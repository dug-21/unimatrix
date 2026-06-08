# Risk-Based Test Strategy: vnc-030

GH Issue: #699 · Mode: architecture-risk · Date: 2026-06-08 (rev2 — post vnc-027 merge, ADR-006 canary rescope, ADR-002/007 rebase onto merged tree, AC-10/FR-29 added)
Inputs: SCOPE.md (vnc-027 merged, AC-10 added), ARCHITECTURE.md, ADR-001..007 (006 rev2, 002/007 rebased), SPECIFICATION.md (FR-01..29; FR-09/10/22/28 reworked, FR-29 new, OQ-E crux), SCOPE-RISK-ASSESSMENT.md (SR-01..12), ass-072 FINDINGS, cwd-probe report.
Historical evidence consulted (Unimatrix): #3486, #3499, #1469, #754, #1268, #4372, #4092, #4358, #924.

This strategy identifies what could fail in the *designed* system — the cycle-tracker lifecycle, the three-site stamp read, the FeatureSource precedence flip, the migration, the canary invariant, the fail-open client, the cross-feature seams, and the uncontracted Claude Code behaviors. The tester translates each risk into concrete tests; every risk below carries at least one scenario.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|------------------|----------|------------|----------|
| R-01 | Three server stamp-read sites (~listener.rs:719, :861, batch :1042) drift — a site forgets the `cycle_stamp` read (the #3486 failure class: field extracted/present but not consumed at one construction site) | High | Med | **Critical** |
| R-02 | Cycle tracker copies the offsets delete-on-SessionClose lifecycle; Stop maps to SessionClose and fires every assistant turn, so the stamp dies after turn 1 (ass-072 precondition 5; C-03) | High | Med | **Critical** |
| R-03 | A new client fs touchpoint (`readCycle`/`writeCycle`/`updatePhase`/`deleteCycle`/`readdir`/health RMW) throws or writes to stdout on a failure path, violating fail-open — blast radius is every hook fire, i.e. Claude Code itself stalls | High | Low | **Critical** |
| R-04 | The `enrich_topic_signal` FeatureSource guard is mis-implemented: extraction still beats a `Declared` registry feature (reintroduces #588), OR a declared feature wrongly beats extraction for `Inferred` sessions (over-correction, never-declare regression) | High | Med | **Critical** |
| R-05 | Extraction suppression strip is wrong: strips `topic_signal` from CYCLE_* frames too (server loses the declaration), or fails to strip on a non-CYCLE_* frame class (stamped event carries both stamp + extracted signal → double attribution / tally pollution) | High | Med | **Critical** |
| R-06 | The `RecordEvents` batch/replay path is not iterated in decoration — only single `RecordEvent` frames get stamped; queued-on-send-failure batches replay partially stamped | High | Med | **High** |
| R-07 | vnc-027 MERGED — seam now concrete; a later vnc-027 follow-up drifts the merged matcher (`merge-settings.js:49`) or null sentinel (`build-request-tools.js:326`) so `context_cycle` PreToolUse routes through the no-send sentinel instead of yielding a CYCLE_* frame → tracker never written, whole mechanism silently dead | High | Med | **High** |
| R-08 | `--resume` session_id reuse and root-session-id inheritance are uncontracted Claude Code behaviors pinned to claude 2.1.167; a CLI upgrade silently degrades declared sessions to vote/NULL with no error path | High | Med | **High** |
| R-09 | Heuristic-demotion regression: extraction/NULL-fill/eager/vote-on-NULL is the *only* attribution for 60% of sessions / 25% of observations; a precedence-chain change silently drops never-declare attribution | High | Med | **High** |
| R-10 | `stamp.topic` is written from `payload.feature_cycle` free-form; a non-canonical topic (description, not feature-id) is stamped as `'declared'` and silently mis-attributes the whole session with high confidence (#1469 class, now contractual not advisory) | Med | Med | **High** |
| R-11 | Migration schema-version collision: vnc-030 bumps `CURRENT_SCHEMA_VERSION` 27→28; a parallel feature (crt-052/vnc-027) also targeting 28 produces a duplicate version stamp or out-of-order block | Med | Med | **High** |
| R-12 | A record-path `INSERT INTO observations` site is missed when adding `topic_source` (#4372 multi-surface class) — rows land NULL-source where they should be attributed, poisoning the F6 evidence base; or a non-record-path INSERT is wrongly extended | Med | Med | **High** |
| R-13 | `register_session` overwrite on resume/compact resets `feature_source` to `Inferred(Registered)`; if close/sweep fires in the window before the next stamped event re-applies `Declared`, a declared session is vote-inverted | Med | Low | Medium |
| R-14 | depth>1 subagent carries an intermediate (not root) session_id → stamp miss → silent mis-attribution; canary is the *sole* tripwire and is unverifiable until Claude Code lifts the constraint | Med | Low | Medium |
| R-15 | A stamp path hashes raw cwd instead of routing through `walkToProjectRoot`/`resolveGitFile`; a worktree session writes the tracker under a worktree hash, main-checkout threads of the same session miss it (no persisted raw-cwd discriminator exists to debug it) | Med | Low | Medium |
| R-16 | Wire non-additivity: a `deny_unknown_fields` exists on some struct in the deserialize path, OR a pre-existing parity fixture changes bytes, OR the 7th ts-rs binding drifts CI — old-server/new-client tolerance breaks | Med | Low | Medium |
| R-17 | `apply_stamp` is not truly idempotent (logs `Overridden` per stamped event, or churns `feature_source`), adding per-event mutex contention or log noise that masks real overrides | Low | Med | Medium |
| R-18 | crt-052 adjacency in `infra/session.rs`/`uds/listener.rs`: a non-minimal inversion-fix diff complicates crt-052's rebase of `drain_and_signal_session`/`clear_transcripts_for_feature`, or changes the close/sweep semantics crt-052's selection cites | Med | Low | Medium |
| R-19 | Canary residual after ADR-006 rev2 rescope: the original noisy-tripwire risk (0.20 ratio inflated by concurrent never-declare sessions) is RESOLVED by design — no threshold, no concurrent-file rule, no baseline. NEW residual: the subagent-gated canary depends on a *second* uncontracted behavior — client-side subagent-context detection — whose independence from root-id inheritance is unproven (ADR-006 §7 / OQ-E). Under Branch B (detection co-dependent, breaks together) the production canary is dropped to test-time-only | Med | Med | Medium |
| R-20 | `topic_source` no-backfill means historical rows are NULL; SR-06's before/after distribution comparison windows over the wrong (pre-migration) rows and draws a false F6 conclusion | Low | Low | Low |
| R-21 | #574 relocates `cycle_events` writes server-side and merges before vnc-030 delivers, changing `load_cycle_observations` windowing inputs or the interception timing the tracker hangs on | Low | Low | Low |
| R-22 | `updatePhase` no-ops on a missing tracker file (phase-end without start); the stamp then carries a stale/wrong phase on subsequent events, mis-recording the `phase` column | Low | Low | Low |
| R-23 | Transport-specific stamp loss: decoration is upstream of `selectTransport` (ADR-002 §7) so the stamp *should* be transport-agnostic, but the UDS path (`transport-uds.encodeFrame`) is unproven — a serialization divergence between `transport-uds.js:55-62` and the HTTP body could drop/alter `cycle_stamp` only over UDS (the merged vnc-027 transport), passing HTTP round-trip tests (AC-10/FR-29 wire/round-trip family with R-06/R-16) | Med | Low | Medium |

## Risk-to-Scenario Mapping

### R-01: Three server stamp-read sites drift (#3486 class)
**Severity**: High · **Likelihood**: Med · **Impact**: A stamped event arriving at the un-updated site falls through to the legacy chain — declared attribution silently lost for an entire event class (e.g. batched events), inverting the feature this whole effort exists to guarantee.
**Test Scenarios**:
1. End-to-end round-trip (FR-13): a stamped event through each of the single-record path, the second single site, and the batch path lands a row with `topic_signal=stamp.topic`, `topic_source='declared'` — asserted independently per site, not once.
2. A stamped `RecordEvents` batch of N events yields N declared rows (catches the batch site specifically).
3. Negative: an unstamped Rust-hook frame through all three sites still takes the legacy chain.
**Coverage Requirement**: One assertion per record site that the stamp was read AND applied to the row; the shared `apply_stamp_to_row` helper (ADR-003 mandate) is exercised by all three. Field-exists-on-struct is insufficient evidence.

### R-02: Delete-on-close lifecycle kills the stamp after turn 1
**Severity**: High · **Likelihood**: Med · **Impact**: The tracker is deleted on the first Stop/SessionClose; turns 2..N go unstamped and degrade to vote/NULL — the exact silent failure ass-072 designed against, invisible without multi-turn testing.
**Test Scenarios**:
1. Fire SessionStart (startup/resume/clear/compact), SessionClose, and Stop events with a tracker present; assert the file is byte-unchanged after each (FR-04).
2. Multi-turn simulation: cycle_start → 3× (Stop + RecordEvent) → assert all post-Stop events still find and attach the stamp.
3. Only CYCLE_STOP deletes the file (FR-03); Stop does not.
**Coverage Requirement**: Lifecycle dispatch keys exclusively on CYCLE_START/CYCLE_PHASE_END/CYCLE_STOP frames; no lifecycle event (SessionStart/Close/Stop) is wired to write or delete the tracker.

### R-03: Fail-open violation in a new fs touchpoint
**Severity**: High · **Likelihood**: Low · **Impact**: A throw on the hook's hot path is the highest blast radius in the feature — the hook exits non-zero, no stdout contract is broken, and Claude Code's tool flow is disrupted for every event, not just attribution.
**Test Scenarios**:
1. Failure injection per new fs call: EACCES/ENOENT/EROFS on `readCycle`/`writeCycle`/`updatePhase`/`deleteCycle`/`readdir`/health RMW — each returns the never-throw degrade value (`null`/`false`), exit 0, no stdout, no secret/path in stderr.
2. Corrupt/mistyped tracker JSON → `readCycle` returns `null`, event sent unstamped, no throw (FR-06).
3. Disk-full on `writeCycle` during cycle_start → degrade `false`, event still sent.
**Coverage Requirement**: Every new fs/readdir touchpoint has a failure-injection test; NFR-03 holds on all new client code paths.

### R-04: FeatureSource guard mis-implemented at enrich_topic_signal
**Severity**: High · **Likelihood**: Med · **Impact**: Either #588 is reintroduced (extraction beats declared for unstamped declared sessions) or the floor regresses (declared wrongly beats extraction for `Inferred` sessions, mis-attributing never-declare traffic).
**Test Scenarios**:
1. Unstamped event, registry feature with `FeatureSource::Declared` + contradicting extracted signal → row attributes to the declared feature, `topic_source='declared'` (the unstamped-window #588 remedy, FR-15).
2. Unstamped event, registry feature `Inferred(Registered)` + extracted signal → extraction wins, `topic_source='extracted'` ("explicit wins" survives only against Inferred/absent registry).
3. Registry `Inferred(Voted)` + no extraction → `registry`/`vote` source per FR-21, not `declared`.
4. The inverted debug-forensics log fires (now logs declared-overrides-extraction, ADR-004 consequence).
**Coverage Requirement**: The full ADR-004 §4 decision tree has one case per branch; the `matches!(src, Declared)` guard is the only precedence determinant.

### R-05: Extraction suppression strip is wrong
**Severity**: High · **Likelihood**: Med · **Impact**: Over-strip removes the declaration from CYCLE_* frames (server can't attribute the cycle); under-strip leaves both stamp and extracted `topic_signal` on a frame, double-feeding attribution and polluting the vote tally a stamped session must never feed.
**Test Scenarios**:
1. Same prompt content with/without a tracker file: tracker present → outgoing frame carries `cycle_stamp` and no `topic_signal`; absent → carries `topic_signal`, no stamp (FR-08).
2. CYCLE_* frame with a tracker present: keeps `topic_signal = topic` (byte-identical to Rust-hook cycle frames), gets no extra stamp churn (ADR-002 §3, §5).
3. Server side: a stamped event skips `record_topic_signal` tally and `enrich_topic_signal` (FR-14) — verified the vote tally does not grow on stamped traffic.
**Coverage Requirement**: Suppression is strip-at-decoration on non-CYCLE_* frames only; extraction code stays byte-unchanged for unstamped sessions.

### R-06: Batch/replay frame class missed in decoration
**Severity**: High · **Likelihood**: Med · **Impact**: Single events stamp but `RecordEvents` batches (queue replay shapes) stamp partially or not at all — intermittent, load-dependent attribution loss that unit tests on single frames won't catch.
**Test Scenarios**:
1. A `RecordEvents` batch of mixed CYCLE_*/RecordEvent frames: every ImplantEvent in the batch carries the stamp; `topic_signal` stripped on every non-CYCLE_* member.
2. Send-failure → enqueue → replay: the replayed batch carries the stamp that was true at event time (post-decoration enqueue, ADR-002 §2.5).
**Coverage Requirement**: The decoration loop covers both single-frame and batch shapes; AC-02 round-trip fixtures include a batch case.

### R-07: vnc-027 interception-seam drift (post-merge regression)
**Severity**: High · **Likelihood**: Med · **Impact**: vnc-027 (F4a) MERGED 2026-06-08, so the seam is no longer a rebase-order future risk — it is concrete and testable against real anchors: the narrowed matcher `merge-settings.js:49` (`PRETOOLUSE_CYCLE_MATCHER = "context_cycle|mcp__unimatrix__context_cycle"`) and the null sentinel at `build-request-tools.js:326`. The residual risk is a *post-rebase regression*: a later vnc-027 follow-up drifts the matcher or sentinel so `context_cycle` routes through the no-send sentinel, no CYCLE_* frame is produced, the tracker is never written, and the stamp mechanism is silently inert — passing cycles-module unit tests while production attribution is dead.
**Test Scenarios**:
1. Binding seam-survival (ADR-007 §1, FR-28), now testable as written against the merged tree: a `context_cycle(start)` PreToolUse through the rebased `index.js` pipeline creates the tracker (`cycles.writeCycle`) AND sends a CYCLE_START `RecordEvent` with `cycle_stamp` attached (`request !== null`) — not the `build-request-tools.js:326` sentinel.
2. Control: a non-`context_cycle` PreToolUse returns the null sentinel, `index.js:366` exits 0, no tracker touch, no canary bump (`state.bumpStampMiss` not called), no network.
**Coverage Requirement**: The binding seam-survival test (FR-28) is R-07's mitigation; it gates *before any vnc-030 server-work validation* and is the regression tripwire if a vnc-027 follow-up drifts the matcher/sentinel — both branch points now pinned to real `file:line`.

### R-08: Uncontracted Claude Code behavior drift
**Severity**: High · **Likelihood**: Med · **Impact**: `--resume` id-reuse and root-id inheritance are empirical on one CLI version; a dev-container CLI bump silently breaks crash recovery and subagent stamping with no error — attribution regresses to the floor invisibly.
**Test Scenarios**:
1. Crash + `--resume` simulation: tracker persists, first post-resume event (same session_id per pinned CLI) finds it and stamps with zero gap (AC-01 crash-simulation).
2. Subagent-gated canary fixtures (R-19) assert the drift detector works; test-module docs and the IMPLEMENTATION-BRIEF pin claude 2.1.167. The canary is a zero-tolerance invariant (`stamp_miss == 0`), not a tuned rate signal (ADR-006 rev2).
3. Re-run AC-06 fixtures as the drift check on any post-ship CLI bump (cheap, part of the standard suite) — the pinned-CLI re-run-on-bump check.
**Coverage Requirement**: Pinned-version statement present in test docs + brief; canary unit tests reference the version and assert `stamp_miss == 0` on the healthy declared-subagent fixture; drift surfaces as a nonzero counter (zero-tolerance), not silent loss (NFR-08). Note Branch B residual (R-19/OQ-E): if subagent-context detection is co-dependent with inheritance, the production canary is dropped to test-time-only.

### R-09: Never-declare floor regression
**Severity**: High · **Likelihood**: Med · **Impact**: Silent attribution loss for the 60%/25% of traffic (uni-zero, research, ad-hoc) that *only* the heuristic floor serves — a product-level regression that an accuracy metric on declared sessions cannot see.
**Test Scenarios**:
1. Never-declare session (no tracker, no stamp): extraction → fill/vote attributes exactly as today (FR-19); rows carry `extracted`/`registry-fill`/`vote`/NULL.
2. Fallback regression sample with multiple never-declare shapes — at least one uni-zero, one research-spike, one ad-hoc (SR-06 strengthening, AC-07).
3. Before/after `topic_source` distribution comparison on the live DB (windowed on post-migration rows — see R-20), not accuracy alone.
**Coverage Requirement**: AC-07 fallback sample is broader than one session; demotion changes only precedence order, deletes no heuristic.

### R-10: Non-canonical stamped topic mis-attributes contractually (#1469)
**Severity**: Med · **Likelihood**: Med · **Impact**: A free-form `feature_cycle` value (description not feature-id) is now stamped `'declared'` with full contractual weight — worse than the advisory misfire #1469 described, because the stamp suppresses the extraction that might otherwise have corrected it.
**Test Scenarios**:
1. cycle_start with a free-form topic → if validation (`validateCycleParams`) rejects it, no CYCLE_* frame and no tracker (FR-01 invalid-params-no-file); if it passes, the row is `declared` with that exact topic — assert validation is the gate.
2. Verify `writeCycle` stores `payload.feature_cycle` verbatim and the server does not normalize it (mis-attribution is a declaration-quality/protocol concern, not a mechanism defect — document the boundary).
**Coverage Requirement**: Tracker write is gated on the same validation that gates CYCLE_* frame construction; topic provenance is the declared payload, unmodified.

### R-11: Migration schema-version collision
**Severity**: Med · **Likelihood**: Med · **Impact**: Three features deliver in sequence touching the schema; a duplicated `CURRENT_SCHEMA_VERSION = 28` or out-of-order migration block corrupts version stamping — a migration that no-ops or double-applies on real DBs.
**Test Scenarios**:
1. Migration idempotence (FR-20): fresh DB, already-migrated DB (re-run is a no-op via the pragma guard), pre-migration DB at v27 → v28.
2. At delivery (post-rebase): confirm no other landed migration claims v28; the version stamp lands at the end of `run_main_migrations` in one transaction.
**Coverage Requirement**: pragma_table_info check precedes the ALTER; the version number is unique against the rebased main at delivery time.

### R-12: A topic_source INSERT site missed (#4372 class)
**Severity**: Med · **Likelihood**: Med · **Impact**: A record-path INSERT not extended writes NULL-source rows that should be attributed → the F6 retirement gate decides on incomplete evidence; or a non-record-path INSERT wrongly extended writes a meaningless source.
**Test Scenarios**:
1. One integration case per `topic_source` value (declared/extracted/registry-fill/vote/NULL) asserting the column matches the code path that wrote it (FR-21).
2. Both listener-local INSERTs (:3015, :3055) gain `?10`; grep-audit `INSERT INTO observations` confirms every record-path site is covered and non-record-path sites (store-crate `insert_observation`, analytics/export/background) stay NULL-source by design (ADR-005 §4).
**Coverage Requirement**: Delivery performs the grep-audit; the source value is computed by the same decision tree that sets `topic_signal` so source/signal cannot disagree.

### R-13: register_session overwrite window vote-inverts a declared session
**Severity**: Med · **Likelihood**: Low · **Impact**: After resume/compact re-register resets `feature_source` to `Inferred(Registered)`, a close/sweep firing before the next stamped event re-applies `Declared` resolves via vote — a narrow but real inversion the stamp is supposed to prevent.
**Test Scenarios**:
1. Simulate re-register (feature_source → Inferred(Registered)) then immediate sweep with a contradicting vote and no intervening stamped event → assert behavior matches the documented accepted-consequence (degrades, then next stamp restores) and is not a regression beyond it.
2. Re-register → one stamped event (`apply_stamp` restores Declared) → sweep → declared wins.
**Coverage Requirement**: The accepted-consequence boundary (ADR-004 §3, Registry Touchpoint Fence) is asserted, not just asserted-away; the window is documented for crt-052.

### R-14: depth>1 grandchild silent stamp miss
**Severity**: Med · **Likelihood**: Low · **Impact**: A subagent-spawned subagent (when Claude Code lifts the constraint) may carry an intermediate session_id; the grandchild's events miss the root tracker and mis-attribute, with the canary the only signal.
**Test Scenarios**:
1. Forward-compat fixture (ADR-006 §5): a grandchild-id event with no matching tracker, while the root tracker exists, lands in `stamp_miss` — asserting silent loss is impossible (it always trips the canary).
2. depth-1 control: subagent event with root session_id S joins S and is stamped from `cycles/{S}.json` (FR-22a).
**Coverage Requirement**: AC-06 wording is "root session's session_id at any nesting depth"; the canary increments on any non-root-keyed miss.

### R-15: Raw-cwd hashing splits the worktree tracker
**Severity**: Med · **Likelihood**: Low · **Impact**: A worktree-issued cycle_start writes the tracker under a worktree hash; other threads (resolving to main-root) miss it. The probe found no persisted raw-cwd discriminator, so this is undebuggable post-hoc.
**Test Scenarios**:
1. Worktree regression (FR-23, AC-08): a `.git` *file* pointing at the main gitdir; a worktree cycle_start writes the tracker under the main-root hash; a subsequent worktree event is stamped from it.
2. Grep/assert no stamp path hashes raw cwd — all route through `config.resolve(cwd).stateDir` (C-11).
**Coverage Requirement**: Every tracker path derives from the project-root walk; AC-08 asserts existing F3 behavior over the gitdir port.

### R-16: Wire non-additivity / binding drift
**Severity**: Med · **Likelihood**: Low · **Impact**: A `deny_unknown_fields` anywhere on the deserialize path breaks old-server/new-client tolerance; a changed parity fixture or drifted 7th binding breaks the frozen-F1 contract and CI.
**Test Scenarios**:
1. All pre-existing wire fixtures pass byte-unmodified (the field never appears via `skip_serializing_if`) (NFR-04).
2. Serde tolerance both directions: stamped frame deserialized by a struct without the field (old-server simulation); unstamped Rust-hook frame → `cycle_stamp: None` → legacy chain (FR-12).
3. ts-rs export sentinel renamed/recounted to seven; `git diff --exit-code bindings/` clean except the expected additive `ImplantEvent.ts`/new `CycleStampPayload.ts` diff.
4. Serde trio: None-absent / Some-present / null-tolerant (mirroring col-017 topic_signal at wire.rs:1345-1367).
**Coverage Requirement**: No `deny_unknown_fields` introduced; fixtures and other bindings byte-identical.

### R-17: apply_stamp not idempotent
**Severity**: Low · **Likelihood**: Med · **Impact**: Per-event `Overridden` log noise or `feature_source` churn masks real overrides and adds needless mutex traffic on every stamped record.
**Test Scenarios**:
1. Two stamped events same session same topic → `apply_stamp` no-ops on the second (no `Overridden` log, feature+source unchanged).
2. Stamped event after a contradicting declared topic → last-writer-wins is logged once as a genuine override.
**Coverage Requirement**: Idempotency = no-op when feature+source already match.

### R-18: crt-052 adjacency / non-minimal diff
**Severity**: Med · **Likelihood**: Low · **Impact**: A non-minimal close/sweep refactor complicates crt-052's rebase of adjacent functions in the same files, or changes the semantics crt-052's session selection cites.
**Test Scenarios**:
1. Diff review at gate: both inversion fixes are one guard around the existing `or_else` + one short-circuit before the existing vote chain, nothing else (FR-18, C-10).
2. The post-fix close/sweep semantics are documented as ADR-007 §2's citable interface; an integration test asserts a declared session's `feature_cycle` is no longer vote-flippable at close or sweep.
**Coverage Requirement**: Minimal-diff verified; vnc-030 makes zero changes to `drain_and_signal_session`/`clear_transcripts_for_feature`/transcript buffer.

### R-19: Canary residual after subagent-gated rescope (ADR-006 rev2)
**Severity**: Med · **Likelihood**: Med · **Impact**: The original false-signal risk is RESOLVED at its root by the rescope — there is no 0.20 threshold, no `fnf_record_send_count` denominator, no concurrent-file / `anyOtherCycleFile` rule, and no per-deployment baseline tuning, so concurrent never-declare sessions can no longer inflate a ratio or desensitize a tripwire (ADR-006 §5 Consequences). The NEW residual risk: the subagent-gated canary now depends on a *second* uncontracted Claude Code behavior — client-side subagent-context detection (depth ≥ 1 / SubagentStart) — whose independence from root-id inheritance is unproven (ADR-006 §7 crux / spec OQ-E). Under **Branch B** (subagent-context detection is co-dependent with inheritance and breaks together), the inheritance-break case is observationally identical to a never-declare session client-side, so the production canary cannot detect it and is honestly dropped to **test-time-only**.
**Test Scenarios**:
1. Subagent-inheritance-drift fixtures (FR-09 / FR-22 / AC-06 (b)) are now the canary's coverage — NOT a rate-threshold soak: depth-0 never-declare → no increment (structural noise); depth ≥ 1 subagent with inherited root tracker present → no increment; depth ≥ 1 subagent carrying a non-inherited id while the root tracker exists → exactly one increment; depth>1 grandchild id with no tracker → lands in `stamp_miss`, not silent loss.
2. Zero-tolerance invariant: the single-declared-session-with-subagent fixture asserts `stamp_miss == 0` end-to-end (ships either branch).
3. SessionStart/SessionRegister/SessionClose frames never reach decoration → never increment (verified by frame class); the vnc-027 null sentinel short-circuits non-cycle PreToolUse before decoration.
4. Delivery probe (OQ-E, gating the production canary only): inspect whether SubagentStart / depth indicators survive on hook stdin under a simulated/aged CLI where root-id inheritance is absent — Branch A ships the production canary, Branch B narrows to test-time invariant.
**Coverage Requirement**: The canary is a zero-tolerance invariant (`stamp_miss == 0`), not a rate signal; coverage is the subagent-inheritance-drift fixture set (positive + negative + forward-compat), not a noise-baseline measurement. The test-time invariant ships regardless of the OQ-E branch.

### R-20: topic_source measurement windows over pre-migration NULLs
**Severity**: Low · **Likelihood**: Low · **Impact**: SR-06's before/after distribution check includes pre-migration NULL-source rows and draws a false F6 conclusion.
**Test Scenarios**:
1. The distribution comparison windows on post-migration rows only (no backfill exists by design, ADR-005 §3).
2. Migration leaves existing rows NULL (FR-20).
**Coverage Requirement**: Measurement methodology documented to window post-migration; no historical-source invention.

### R-21: #574 lands before vnc-030 delivers
**Severity**: Low · **Likelihood**: Low · **Impact**: Relocated `cycle_events` writes change windowing inputs or interception timing under vnc-030's feet.
**Test Scenarios**:
1. Assumption-expiry check at delivery (ADR-007 §3): if #574 merged, re-verify cycle_events rows carry the same windowing timestamps AND client-side PreToolUse interception of `context_cycle` still fires.
**Coverage Requirement**: The no-race argument has an explicit expiry condition checked at delivery, not silently assumed stale-proof.

### R-22: Stale phase from updatePhase no-op
**Severity**: Low · **Likelihood**: Low · **Impact**: A phase-end without a prior start no-ops; subsequent stamps carry a stale phase, mis-recording the `phase` column.
**Test Scenarios**:
1. updatePhase on a missing file → no-op `false`, no recreate (ADR-001); phase-end-after-stop degrades cleanly.
2. Normal start → phase-end → assert the stamp's phase reflects the latest `next_phase`.
**Coverage Requirement**: Phase desync degrades fail-open; no silent recreate.

### R-23: Transport-specific (UDS) stamp loss
**Severity**: Med · **Likelihood**: Low · **Impact**: vnc-027's UDS transport merged 2026-06-08; vnc-030 owes #699 the proof that the stamp rides it byte-equivalently (AC-10/FR-29). Decoration mutates the in-memory `request` upstream of `selectTransport` (ADR-002 §7), so the stamp *should* be transport-agnostic — but a serialization divergence in `transport-uds.encodeFrame` (`transport-uds.js:55-62`) vs the HTTP body (`transport-http.post`) could drop or alter `cycle_stamp` only over UDS, slipping past any HTTP-only round-trip test.
**Test Scenarios**:
1. UDS regression (FR-29, AC-10): drive one stamped FNF `RecordEvent` (tracker present) through `index.js` `runFireAndForget` with `config.mode = "uds"`; assert the bytes from `transport-uds.encodeFrame` decode to a JSON payload containing `cycle_stamp`.
2. Cross-transport equivalence: the same input over HTTP and UDS yields a byte-equivalent `cycle_stamp` payload (decoration is strictly upstream of the transport fork at `index.js:410`).
3. Replay: a queued (post-decoration) frame replayed over UDS carries the same stamp (queue stores the decorated `request`).
**Coverage Requirement**: The stamp regression is proven over UDS, not just HTTP; the test seam is pinned at `transport-uds.encodeFrame`. Folds into the wire/round-trip family (R-06 batch/replay, R-16 wire tolerance).

## Integration Risks

- **Three-site lockstep (R-01)** is the highest-value integration risk — the #3486 lesson is that one of N construction/read sites is forgotten. A shared `apply_stamp_to_row` helper (ADR-003) collapses three sites to one; the round-trip AC must still assert all three.
- **Wire seam, both directions (R-16)**: new-server/old-Rust-hook is the steady-state production mix (hook.rs untouched, no flag); old-server/new-client is the deploy-window mix. Both must be tested, not assumed.
- **Client↔server stamp contract (R-05/R-04)**: the client's strip-and-stamp must exactly match the server's skip-tally/skip-enrich expectation; a mismatch double-counts or drops. The boundary is the `cycle_stamp`-present branch on both ends.
- **vnc-027 interception seam (R-07)** and **crt-052 file adjacency (R-18)**: three features, two files (pattern #924). vnc-027 MERGED 2026-06-08 — the seam-survival test (ADR-007 §1, FR-28) is now testable against real anchors (matcher `merge-settings.js:49`, sentinel `build-request-tools.js:326`) and gates before any vnc-030 server work; it is the post-merge regression tripwire if a later vnc-027 follow-up drifts the matcher/sentinel. Minimal-diff discipline (ADR-007 §2) governs the crt-052 adjacency (still sequenced last).
- **Transport fork (R-23)**: decoration is strictly upstream of `selectTransport` (`index.js:410`); the stamp must be proven byte-equivalent over the merged vnc-027 UDS transport (`transport-uds.encodeFrame`), not only HTTP — the AC-10/FR-29 obligation owed to #699.
- **Registry lifecycle interaction (R-13)**: the stamp's resilience depends on `apply_stamp` re-establishing `Declared` after every re-register overwrite; the gap between overwrite and next stamp is the residual integration seam with the (out-of-scope) registry-drain family.

## Edge Cases

- Multi-turn session: Stop fires every turn — tracker must survive (R-02).
- Crash + `--resume` mid-cycle: tracker persists, same id continues (R-08).
- Fork (`--fork-session`): new id → tracker miss → falls to vote/NULL (marker tier deferred, R-14 family).
- Empty/corrupt/mistyped tracker JSON → null, unstamped, no throw (R-03).
- Concurrent declared + never-declare sessions in one repo → no canary signal: depth-0 never-declare is structural noise and never increments (ADR-006 rev2 subagent-gating; R-19).
- UDS-transport stamp must be byte-equivalent to HTTP (R-23).
- Worktree checkout with `.git` file → main-root hash routing (R-15).
- 7-day-idle session resumes → tracker pruned → unstamped, vote floor catches it (accepted, ADR-001).
- `RecordEvents` batch + send-failure replay → every member stamped (R-06).
- phase-end before start / after stop → no-op (R-22).
- depth>1 grandchild → canary trips, no silent loss (R-14).
- Last-writer-wins on concurrent declarations (subagent clobber) → `Overridden` log, protocol concern not mechanism defect (R-17).

## Security Risks

The client accepts two externally-influenceable inputs on this path: the **`session_id`** (from hook stdin) and the **cycle topic** (`payload.feature_cycle`, from any actor able to issue a `context_cycle` tool call).

- **Path traversal via session_id** — the tracker filename is `cycles/{sanitizeSessionKey(sid)}.json`. Raw `session_id` is passed in and sanitized *inside* `cycles.js` (pattern #4772, never pre-sanitize at call sites). Risk: a malformed session_id (`../../`, absolute path, null bytes) escapes the state dir. **Scenario**: feed adversarial session_ids through `writeCycle`/`readCycle`/`deleteCycle` and assert all writes/reads/deletes stay within `cycles/`; assert `sanitizeSessionKey` neutralizes traversal sequences. Blast radius if unmitigated: arbitrary file write/delete under the user's home with the hook's privileges.
- **Topic content injection** — `stamp.topic` flows to `topic_signal` and into the DB. (a) SQL: must traverse the parameterized `?10` bind, never string interpolation — **scenario**: a topic containing SQL metacharacters lands as a literal column value. (b) `health.json` content-free contract: the canary must store a *count only* — **scenario**: assert no topic/session-id/path is ever written to `health.json` (ADR-006 §1), so a malicious topic cannot poison the breadcrumb.
- **Blast radius if the client is compromised** — fail-open bounds it: every fs/network call wrapped, exit 0, no stdout on failure, no secrets in stderr/breadcrumbs (NFR-03). The tracker file is non-executable JSON under the project hash; corrupt content degrades to `null` (R-03), it is never `eval`'d or trusted for control flow.
- **No deserialization of untrusted server input on this path** — the new wire field is server-*inbound* only; the client never deserializes a `cycle_stamp` from the network. `deny_unknown_fields` absence (R-16) is a compatibility requirement, not an injection surface, because the server validates the typed `CycleStampPayload` and binds it parameterized.

## Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| fs error on any tracker op | Degrade to `null`/`false`, event sent unstamped, exit 0, no throw (NFR-03) |
| Corrupt tracker JSON | `readCycle` → `null`, unstamped, no throw |
| CLI inheritance/resume drift | `stamp_miss` becomes nonzero (zero-tolerance invariant, not a trend/threshold); investigate before trusting new attribution (FR-10). Branch B residual: if subagent-context detection breaks with inheritance, production canary is test-time-only (R-19/OQ-E) |
| Stamped event at an un-updated server site (regression) | Caught by per-site round-trip AC, not shipped (R-01) |
| Re-register overwrite before next stamp | Degrades to floor for the gap; next stamped event restores `Declared` (R-13, accepted) |
| No declaration (never-declare session) | Floor attributes via extraction → fill → vote-on-NULL exactly as today (R-09) |
| Fork / fresh restart without re-declaration | Falls to vote/NULL; protocol re-declaration line (AC-09) recovers; marker tier deferred |
| Migration re-run | Idempotent no-op via pragma guard (R-11) |
| Worktree session | Main-root hash routing; one shared tracker across threads (R-15) |
| Event over UDS transport | Stamp byte-equivalent to HTTP (decoration upstream of `selectTransport`); proven by the `transport-uds.encodeFrame` regression (R-23) |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|-------------------|------------|
| SR-01 (uncontracted CLI behavior, no error path) | R-08, R-14, R-19 | ADR-006 rev2 makes `stamp_miss` a **zero-tolerance invariant** (`stamp_miss == 0`) — no 0.20 trigger, no denominator, no baseline ritual — plus pinned claude 2.1.167 and the re-run-AC-06-fixtures-on-CLI-bump check; drift surfaces as a nonzero counter. The canary is no longer a tuned rate signal. Residual: depth>1 unverifiable (R-14); the subagent-gating itself rests on a second uncontracted behavior (subagent-context detection) — under Branch B the production canary is test-time-only (R-19/OQ-E). |
| SR-02 (3-byte client size budget) | — (RESOLVED, largely discharged) | vnc-027's C-04 gate redefinition MERGED 2026-06-08 (100,000 B stripped / 160,000 B raw); merged tree measures ~68,907 B stripped / ~112,773 B raw; vnc-030's additions ~3,900 B raw / ~2,050 B stripped fit with headroom; named fallback = fold `cycles.js` into `state.js`. The stacked-external-dependency risk is discharged by the merge. Residual only: re-measure vnc-030's own byte additions post-rebase once they land (NFR-01/C-04 at delivery). |
| SR-03 (#3486 field-not-inserted class) | R-01 | ADR-003 binds the end-to-end round-trip AC at all three server read sites + a shared helper; field-exists is insufficient evidence. |
| SR-04 (topic_source taxonomy poisons F6) | R-12, R-20 | ADR-005 fixes one write site per value via the shared decision tree; no backfill; grep-audit of INSERT sites. |
| SR-05 (registry escape-hatch open-ended) | R-13, R-17, R-18 | ADR-004 Registry Touchpoint Fence enumerates exactly four touchpoints; everything else is a named follow-up. Tested via R-13's accepted-consequence boundary. |
| SR-06 (never-declare floor regression, thin sample) | R-09, R-20 | AC-07 strengthened: multiple never-declare shapes + before/after `topic_source` distribution on the live DB (windowed post-migration). |
| SR-07 (MARKER tier hole) | R-14 (fork/restart fallthrough) | Accepted/deferred: AC-04 wording "marker when present" is normative; named follow-up issue (C-13) with crt-052 snapshot-seam dependency must exist before design-gate exit (ADR-007 §4). Not implemented here. |
| SR-08 (#588 "residue" undefined) | — (disposition, not a test risk) | ADR-004 / FR-26 enumerate resolved claims vs named residue (historical extracted rows, Rust-hook per-row tallies, scenario-16 hookless); close decision is mechanical via the PR. |
| SR-09 (three features, shared `build-request.js`) | R-07, R-23 | vnc-027 MERGED — ADR-007 §1 interception-seam-survival contract (FR-28) now binds against real anchors (`merge-settings.js:49`, `build-request-tools.js:326`) and gates server work; it is the post-merge regression tripwire. vnc-030 adds zero logic to `build-request*.js` (rebase surface = `index.js` orchestration). The merged UDS transport adds the AC-10/FR-29 stamp-equivalence obligation (R-23). |
| SR-10 (`infra/session.rs` adjacency with crt-052) | R-18 | ADR-007 §2 citable close/sweep interface + minimal-diff inversion fixes (FR-18). |
| SR-11 (worktree cwd dump open) | R-15 | Resolved: cwd probe ran (hook cwd = worktree path); AC-08 asserts existing F3 gitdir-port behavior; risk reduces to no-raw-cwd-hashing (C-11). |
| SR-12 (#574 relocation may land mid-stream) | R-21 | ADR-007 §3 no-race argument + explicit assumption-expiry check at delivery. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01..05) | ~16 — per-site round-trip ×3, multi-turn lifecycle, fail-open injection per fs touchpoint, full FeatureSource decision tree, suppression both directions |
| High | 7 (R-06..12) | ~16 — batch/replay, seam-survival tripwire, CLI-drift + canary, never-declare fallback (multi-shape), validation-gated topic, migration idempotence ×3, per-value topic_source + grep-audit |
| Medium | 8 (R-13..19, R-23) | ~17 — re-register window, depth>1 forward-compat, worktree regression, wire tolerance both ways + fixture-unchanged + binding drift, apply_stamp idempotency, minimal-diff review, subagent-gated canary fixture set (positive + negative + forward-compat, zero-tolerance) + OQ-E Branch-B probe, UDS-transport stamp regression + cross-transport equivalence |
| Low | 3 (R-20..22) | ~5 — measurement windowing, #574 expiry check, phase no-op |

## Knowledge Stewardship
- Queried: context_search for the #3486 field-not-inserted class, multi-surface migration/insert lessons, and parallel-file risk patterns — found #3486 (payload construction gap), #3499 (optional-field if-Some guard mirror), #1469 (non-canonical topic silent mis-attribution), #754 (write-time metadata breaks read paths), #4372 (schema extension = N surfaces), #4092 (idempotent ALTER guard), #924 (parallel agents grouped by file). All elevated specific risks (R-01, R-04/R-17, R-10, R-12, R-11, R-07/R-18).
- Stored: nothing novel to store — the governing patterns (#3486, #4372, #4092, #924) already exist; the vnc-030-specific recurrence ("contractual write-time field repeats the field-not-inserted class across N read sites; require per-site round-trip evidence, not field-exists") is one feature's instance, not yet a cross-feature (2+) pattern. Re-evaluate at retro if vnc-027/crt-052 hit the same shape.
