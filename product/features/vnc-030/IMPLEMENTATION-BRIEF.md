# vnc-030 Implementation Brief — Contractual Cycle Attribution (F4b)

GH Issue: #699 · Delivery order (pinned): **vnc-027 (MERGED 2026-06-08) → vnc-030 (next-up) → crt-052** · Regenerated 2026-06-08 (rev2 — post vnc-027 merge; ADR-006 canary rescope; ADR-002/007 rebased onto the merged tree; AC-10/FR-29 added)

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-030/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-030/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-030/architecture/ARCHITECTURE.md |
| ADR-001 Cycle tracker lifecycle | product/features/vnc-030/architecture/ADR-001-cycle-tracker-lifecycle.md |
| ADR-002 Stamp decoration (FNF path) | product/features/vnc-030/architecture/ADR-002-stamp-decoration-fnf-path.md |
| ADR-003 `cycle_stamp` wire field | product/features/vnc-030/architecture/ADR-003-cycle-stamp-wire-field.md |
| ADR-004 FeatureSource precedence | product/features/vnc-030/architecture/ADR-004-feature-source-precedence.md |
| ADR-005 topic_source taxonomy + migration | product/features/vnc-030/architecture/ADR-005-topic-source-taxonomy-migration.md |
| ADR-006 stamp_miss canary (rev2) | product/features/vnc-030/architecture/ADR-006-stamp-miss-canary.md |
| ADR-007 Cross-feature seam contracts | product/features/vnc-030/architecture/ADR-007-cross-feature-seam-contracts.md |
| Specification | product/features/vnc-030/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/vnc-030/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-030/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-030/ACCEPTANCE-MAP.md |
| cwd probe report | product/features/vnc-030/agents/vnc-030-cwd-probe-report.md |

## Goal

Make feature attribution contractual for every declared session: a client-side cycle stamp (disk tracker + additive wire field) becomes the primary attribution mechanism; the server enforces a presence-gated precedence chain (stamp → marker-when-present → vote-on-NULL) that fixes both declared-vs-vote inversions; and the heuristic pipeline is demoted — never deleted — to remain the floor for the ~60% of sessions that never declare. An additive `observations.topic_source` column gives the F6 (#682) `hook.rs`-retirement gate its evidence base. Replaces ~90%-inference attribution (20.2% of live observations carry a contradicting extracted signal — the #588 surface) with a write-time contract that rides both the HTTP and the merged vnc-027 UDS transports byte-equivalently (AC-10).

## Component Map

| Component | Source file (change) | ADR | Pseudocode | Test Plan |
|-----------|----------------------|-----|-----------|-----------|
| cycles (client tracker module) | `packages/unimatrix/lib/hook-client/cycles.js` (NEW) | ADR-001 | pseudocode/cycles.md | test-plan/cycles.md |
| index decoration (client) | `packages/unimatrix/lib/hook-client/index.js` (extend) | ADR-002 | pseudocode/index-decoration.md | test-plan/index-decoration.md |
| state / stamp_miss canary (client) | `packages/unimatrix/lib/hook-client/state.js` (extend) | ADR-006 | pseudocode/state-canary.md | test-plan/state-canary.md |
| wire `cycle_stamp` | `crates/unimatrix-engine/src/wire.rs` (extend) | ADR-003 | pseudocode/wire-cycle-stamp.md | test-plan/wire-cycle-stamp.md |
| registry FeatureSource | `crates/unimatrix-server/src/infra/session.rs` (extend) | ADR-004 | pseudocode/feature-source.md | test-plan/feature-source.md |
| listener stamp-read + topic_source | `crates/unimatrix-server/src/uds/listener.rs` (extend) | ADR-004, ADR-005 | pseudocode/listener-stamp-read.md | test-plan/listener-stamp-read.md |
| topic_source migration | `crates/unimatrix-store/src/migration.rs` (extend) | ADR-005 | pseudocode/topic-source-migration.md | test-plan/topic-source-migration.md |
| protocol re-declaration line | `.claude/protocols/uni/uni-{design,delivery,bugfix}-protocol.md` (extend) | AC-09 | pseudocode/protocol-redeclaration.md | test-plan/protocol-redeclaration.md |
| docstring drive-by | `crates/unimatrix-observe/src/attribution.rs` + `lib/hook-client/topic-signal.js` | ADR-004 §C9 / FR-25 | pseudocode/docstring-driveby.md | test-plan/docstring-driveby.md |
| UDS-path stamp regression (AC-10/FR-29) | TEST ONLY — seam at `lib/hook-client/transport-uds.js` `encodeFrame` (`:55-62`, vnc-027-merged; no vnc-030 source change) | ADR-002 §7 | pseudocode/index-decoration.md (decoration upstream of `selectTransport`) | test-plan/uds-stamp-regression.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |
| Seam-survival + round-trip + UDS-stamp gate tests | test-plan/seam-and-roundtrip.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a; the paths above are the expected component decomposition from the architecture (ARCHITECTURE.md C1–C9). Actual file paths are confirmed during delivery. AC-10/FR-29 is a regression test against the vnc-027-merged `transport-uds.encodeFrame` seam — it adds no vnc-030 production source.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Cycle tracker module placement & lifecycle | New `cycles.js`; create on `cycle_start`, update on `phase-end`, delete on `stop`; never on SessionStart/SessionClose/Stop; 7-day prune; `state.js` atomics; path via `config.resolve(cwd).stateDir`, never raw-cwd hash | ass-072 GO, AC-01/AC-08 | ADR-001-cycle-tracker-lifecycle.md |
| Stamp attach point | FNF-path decoration in `index.js` between `buildRequest` and dispatch; `build-request*.js` gets zero vnc-030 logic (rebase surface = `index.js` orchestration only); suppression by strip on non-CYCLE_* frames; delta frames unstamped; enqueue post-decoration | AC-01/02/03, SR-09 | ADR-002-stamp-decoration-fnf-path.md |
| Stamp rides both transports (HTTP + merged UDS) | Decoration mutates the in-memory `request` upstream of `selectTransport` (`index.js:410`); both transports `JSON.stringify` the same object, so `cycle_stamp` is byte-identical on the UDS frame (`transport-uds.encodeFrame`, `:55-62`) and the HTTP body — proven by AC-10/FR-29, discharging vnc-027's post-merge obligation owed to #699 | AC-10, SR-09 | ADR-002 §7 |
| Wire shape | Additive `ImplantEvent.cycle_stamp: Option<CycleStampPayload{topic, phase}>`, `#[serde(default, skip_serializing_if)]`, no `deny_unknown_fields`; 7th ts-rs export; end-to-end round-trip AC at all 3 read sites (#3486) | AC-02, SR-03 | ADR-003-cycle-stamp-wire-field.md |
| Server precedence | `FeatureSource::{Declared, Inferred(Registered\|Voted)}`; `apply_stamp` (idempotent Declared set); two `or_else` inversion flips (sweep + close); enrich guard so declared registry beats extraction; demote-not-delete | AC-04, SR-05/SR-10 | ADR-004-feature-source-precedence.md |
| topic_source taxonomy & migration | `declared/extracted/registry-fill/vote/NULL`, one write site per value via shared decision tree; v27→v28 pragma-guarded ALTER, no backfill; `CURRENT_SCHEMA_VERSION = 28` | AC-05, SR-04 | ADR-005-topic-source-taxonomy-migration.md |
| stamp_miss canary (rescoped, rev2) | Subagent-gated zero-tolerance invariant: increments **only** when a depth≥1 subagent-context event finds no tracker for its inherited **root** session_id (inheritance drift). Depth-0 never-declare sessions are NOT counted (structural noise). `stamp_miss == 0` is the contract at test time AND production. **Removed**: 0.20 threshold, `fnf_record_send_count` denominator, `anyOtherCycleFile` concurrent-file rule, live-baseline measurement, human re-set ritual. Pinned CLI claude 2.1.167. Production canary's existence is delivery-probe-gated (OQ-E); test-time invariant ships either way | AC-06, SR-01 | ADR-006-stamp-miss-canary.md |
| Cross-feature seams | vnc-027 interception-seam survival asserted against **merged** anchors (matcher `merge-settings.js:49`, null sentinel `build-request-tools.js:326`) + post-rebase seam-survival test (R-07/FR-28) that gates before any server work; crt-052 citable close/sweep interface + minimal-diff; #574 no-race + expiry; marker-recovery follow-up = **#700** | SR-07/09/10/12 | ADR-007-cross-feature-seam-contracts.md |
| Marker (tier 2) recovery | Named hole; NOT implemented. Deferred to follow-up issue **#700**, which must consume crt-052's transcript-snapshot seam (no second buffer reader). AC-04 wording "marker when present" is normative | OQ1/C-13/OQ-D | ADR-007 §4 |
| AC-07 accuracy denominator | Declared protocol sessions only (declaration = ground truth); never-declare sessions appear only in the fallback regression sample; canary is **decoupled** from the AC-07 baseline step (rev2) | OQ2 (human, 2026-06-08) | — |
| uni-zero provenance | Ordinary extraction; `is_valid_feature_id` has no digit requirement. Drive-by: fix misleading `{alpha}-{digits}` docstrings | OQ2 | ADR-004 §C9 / FR-25 |
| #588 disposition | AC-04 resolves all named claims (stamped row-level moot, unstamped-window remedied, both vote inversions fixed); residue (historical extracted rows, Rust-hook per-row tallies, scenario-16 hookless) is what #588 never covered → close #588 via this PR | SR-08, human 2026-06-08 | ADR-004 / ARCHITECTURE.md §#588 |

## Files to Create / Modify

Create:
- `packages/unimatrix/lib/hook-client/cycles.js` — cycle tracker module (read/write/updatePhase/delete/prune/anyOtherCycleFile), all never-throw.

Modify (client):
- `packages/unimatrix/lib/hook-client/index.js` — FNF decoration: lifecycle dispatch, stamp attach, extraction strip, subagent-gated canary check on miss; rebase surface vs the merged vnc-027 tree is `index.js` orchestration only.
- `packages/unimatrix/lib/hook-client/state.js` — `bumpStampMiss`; `stamp_miss: 0` default in `health.json` breadcrumb.
- `packages/unimatrix/lib/hook-client/topic-signal.js` — docstring correction only (no behavior change).

Modify (server/engine/store):
- `crates/unimatrix-engine/src/wire.rs` — `CycleStampPayload` struct + `ImplantEvent.cycle_stamp`; 7th ts-rs export sentinel.
- `crates/unimatrix-server/src/infra/session.rs` — `FeatureSource`/`InferredOrigin` enums, `SessionState.feature_source`, `apply_stamp`, sweep inversion flip (:628).
- `crates/unimatrix-server/src/uds/listener.rs` — stamp read at 3 record sites (~:719, ~:861, batch ~:1042); `topic_source` per row; close-path inversion flip (~:1950-1978); `ObservationRow.topic_source`; both local INSERTs (:3015, :3055) gain `?10`; enrich FeatureSource guard.
- `crates/unimatrix-observe/src/attribution.rs` — docstring correction only.
- `crates/unimatrix-store/src/migration.rs` — v27→v28 pragma-guarded ALTER; `CURRENT_SCHEMA_VERSION = 28`.

Modify (protocols):
- `.claude/protocols/uni/uni-design-protocol.md`, `uni-delivery-protocol.md`, `uni-bugfix-protocol.md` — restart re-declaration line (AC-09).

No vnc-030 source change to `transport-uds.js` / `transport-http.js` / `build-request*.js` / `merge-settings.js` — these are vnc-027-merged; vnc-030 consumes them unmodified. AC-10/FR-29 adds a regression test against the `transport-uds.encodeFrame` seam only.

## Data Structures

```rust
// wire.rs (ADR-003)
pub struct CycleStampPayload {
    pub topic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}
// ImplantEvent gains, appended after `provider`:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cycle_stamp: Option<CycleStampPayload>,

// infra/session.rs (ADR-004)
pub enum FeatureSource { Declared, Inferred(InferredOrigin) }
pub enum InferredOrigin { Registered, Voted }
// SessionState gains: pub feature_source: FeatureSource  // default Inferred(Registered)

// ObservationRow (ADR-005): topic_source: Option<String>
// observations.topic_source TEXT NULL — 'declared'|'extracted'|'registry-fill'|'vote'|NULL
```

```text
Tracker file (ADR-001): ~/.unimatrix/{projectHash}/hook-client/cycles/{sanitizeSessionKey(sid)}.json
  { "topic": string, "phase": string|null, "declared_at": secs, "updated": secs }
```

## Function Signatures

```js
// cycles.js (ADR-001) — all never-throw (F3 C-05)
readCycle(stateDir, sessionId)            -> {topic, phase} | null
writeCycle(stateDir, sessionId, topic, phase) -> bool   // full file via state.atomicWrite, last-writer-wins
updatePhase(stateDir, sessionId, phase)   -> bool        // RMW; missing file → no-op false, never recreate
deleteCycle(stateDir, sessionId)          -> bool
pruneCycles(stateDir)                                    // age > 7d since `updated`, piggybacks queue.prune
anyOtherCycleFile(stateDir, sessionId)    -> bool        // one readdir, miss-branch only (canary support)

// state.js (ADR-006)
bumpStampMiss(stateDir)                   -> bool        // content-free RMW, count only

// infra/session.rs (ADR-004)
SessionRegistry::apply_stamp(&self, session_id: &str, topic: &str)  // idempotent Declared set
```

Sanitization happens **inside** `cycles.js` (pattern #4772 — never pre-sanitize at call sites). Server record path uses one shared `apply_stamp_to_row`-style helper across all three sites (ADR-003 mandate). Topic content traverses the parameterized `?10` bind, never interpolation; `health.json` stores a count only (no topic / session-id / path).

## Constraints

- **C-01 Frozen F1 wire contract** — additive only; `skip_serializing_if` on the new optional; no `deny_unknown_fields` anywhere; existing parity fixtures + ts-rs bindings pass byte-unchanged.
- **C-02 Rust `hook.rs` untouched** — mixed stamped/unstamped clients against one server, no feature flag.
- **C-03 Tracker never copies offsets delete-on-close** — Stop fires per assistant turn; delete-on-close would kill the stamp after turn 1 (ass-072 precondition 5).
- **C-04 Fail-open client** — never throws, exit 0 always, no stdout on failure paths, no secrets in stderr/breadcrumbs, every fs call wrapped.
- **C-05 Client size budget — RESOLVED (gate merged).** vnc-027's C-04 gate redefinition (its ADR-005, entry #4806) merged 2026-06-08: 100,000 B comment-stripped primary / 160,000 B raw backstop, live at `packages/unimatrix/test/check-hook-client-size.js:34-35`. The merged tree measures ~68,907 B stripped / ~112,773 B raw; vnc-030's additions ~3,900 B raw / ~2,050 B stripped fit with comfortable headroom (~29 KB stripped, ~43 KB raw). Fallback if a later vnc-027 follow-up tightens the raw axis: fold `cycles.js` into `state.js`; move oracle citations to OVERVIEW.md. Re-measure vnc-030's own additions post-rebase at delivery.
- **C-06 Sync-path budget** — sync trio (ContextSearch/CompactPayload/Ping) gains zero file I/O; tracker read is one small JSON read on FNF builds only; miss branch adds one readdir + one health RMW.
- **C-07 Migration discipline** — pragma-guarded idempotent ALTER, all checks before any ALTER (#4092); version stamp at end of `run_main_migrations` in one transaction.
- **C-08 No new npm dependencies** — F3 pure-TS decision stands.
- **C-09 Registry escape-hatch fence (SR-05)** — precedence chain touches exactly four registry touchpoints (`feature_source` field + enums; source assignment at existing set sites; new `apply_stamp`; the two `or_else` flips). Everything else in the per-turn-drain / re-register family is a named follow-up, not this feature.
- **C-10 Minimal-diff inversion fixes** — one guard around the existing `or_else` + one short-circuit before the existing vote chain; post-fix close/sweep semantics documented as a citable interface for crt-052 (ADR-007 §2). Zero changes to `drain_and_signal_session` / `clear_transcripts_for_feature` / transcript buffer.
- **C-11 No raw-cwd hashing** — stamp paths route through the project-root walk (`config.resolve(cwd).stateDir`) only.
- **C-12 Pinned CLI** — claude 2.1.167 for `--resume` id-reuse, depth-1 inheritance, worktree cwd; canary is the drift detector; any CLI bump re-runs the AC-06 fixtures.
- **C-13 Marker-recovery follow-up issue must exist before design-gate exit** — SATISFIED: **#700** filed with the crt-052 snapshot-seam dependency.

## Dependencies

- **vnc-027 (F4a, #680) — MERGED 2026-06-08.** Owns the C-04 size-gate redefinition (ADR-005, entry #4806) and the hook-set reduction (its ADR-004) that narrowed the PreToolUse install matcher to `context_cycle|mcp__unimatrix__context_cycle` (`merge-settings.js:49`) and introduced the `null` no-send sentinel (`build-request-tools.js:326`, short-circuited at `index.js:366`). Also landed the **UDS transport** (`transport-uds.js`). vnc-030 rebases `index.js` orchestration onto the merged tree (zero changes to `build-request*.js`) and discharges vnc-027's post-merge UDS-stamp obligation owed to #699 (AC-10/FR-29). **Post-rebase seam-survival assertion (R-07, ADR-007 §1, FR-28) gates before any vnc-030 server work is validated**: an intercepted `context_cycle(start)` must still reach the tracker write path and yield a CYCLE_START frame with `cycle_stamp` — not the null sentinel; a non-cycle PreToolUse must yield the sentinel with no tracker touch / canary bump / network.
- **F3 client (vnc-026, shipped)** — `state.js` atomics/sanitize/prune; `config.js` `walkToProjectRoot`/`resolveGitFile` gitdir port (landed b2e215fd / #696, cwd-probe-verified at parity); `build-request.js` stamp-attach surface; `health.json` breadcrumb.
- **F2 (#670, shipped)** — transcript buffer (context only; vnc-030 adds no buffer reader).
- **Migration precedent** — v9→v10 `topic_signal` column (migration.rs:219-237).
- **ts-rs codegen** — drift-checked bindings, 7th export (constraint #4726 / vnc-024 ADR-001).
- **crt-052 (#689, delivers AFTER)** — consumes the post-fix close/sweep semantics; edits adjacent functions (`drain_and_signal_session` / `clear_transcripts_for_feature`) in the same server files. Keep inversion fixes minimal-diff.
- **F6 (#682)** — `topic_source` distribution is the retirement-gate evidence base.
- **Pinned CLI** — claude 2.1.167.

External services/crates: none new. No new npm deps (C-08).

## NOT in Scope

- **Building** the TS UDS transport, parity corpus, hook-set reduction, size-gate/offset-delete carry-items — vnc-027 (F4a, MERGED). vnc-030 does NOT build UDS; it only proves the stamp rides the merged UDS transport byte-equivalently (AC-10/FR-29).
- Deleting any heuristic (extraction, NULL-fill, eager, vote) — all survive as the never-declare floor.
- **Marker-recovery implementation** — deferred to **#700** (must consume crt-052's transcript-snapshot seam, never a second buffer reader). The MARKER tier ships as a named hole, not a TBD.
- Server registry lifecycle redesign (per-turn drain, re-register overwrite, mid-session amnesia, #4140) — except the four fenced touchpoints (C-09).
- Any Rust `hook.rs` change.
- #578 audit-log retention — deferred post-OSS-cloud-v1 (vnc-029).
- MCP-only hookless clients (scenario 16) — unattributable by construction; named, not solved.
- Giving uni-zero/research sessions declarations — protocol-side decision, separate change.
- Depth>1 subagent inheritance verification — unverifiable until Claude Code lifts the constraint; the canary is the tripwire.
- Tightening the extractor's permissive filter — only its docstrings are corrected here (drive-by); behavior change is out of scope.

## Alignment Status

ALIGNMENT-REPORT.md (regenerated 2026-06-08, rev2): **5 PASS / 1 WARN / 0 VARIANCE / 0 FAIL** across the six checks (Vision, Milestone, Scope Gaps, Scope Additions, Risk Completeness = PASS; Architecture Consistency = WARN). Reviewed against the reworked artifacts: AC-01..AC-10, FR-01..FR-29, and **23 architecture risks (R-01..R-23)**. Two ratified simplifications (MARKER tier deferred; AC-07 accuracy denominator = declared sessions only) plus the canary rescope, all ratified by recorded human decisions on 2026-06-08.

**Prior canary watch item (0.20-threshold noisy tripwire) — RESOLVED BY DESIGN.** ADR-006 rev2 retires the 0.20 threshold, the `fnf_record_send_count` denominator, the `anyOtherCycleFile` concurrent-file rule, and the per-deployment baseline ritual, replacing them with a subagent-gated zero-tolerance inheritance-drift invariant that counts only drift within the declared population (never depth-0 never-declare sessions). R-19 confirms the false-signal source is removed.

Watch items from the regenerated report:

1. **Architecture Consistency WARN (ADR-001 §1 + ADR-002 §2.4 cited the superseded `anyOtherCycleFile` canary) — RESOLVED.** Doc-sync fix applied 2026-06-08: ADR-001 corrected (now aligned to #4836) and ADR-002 corrected (now aligned to #4837); `anyOtherCycleFile` removed from both. The ADR set now matches the authoritative ADR-006 rev2, ARCHITECTURE.md Integration Surface, and SPEC FR-09. No delivery action required.
2. **New canary residual (OQ-E / ADR-006 §7 / R-19) — ACCEPT.** The production canary depends on subagent-context detection ("depth ≥ 1 / SubagentStart") being **independent** of root-id inheritance. Delivery crux OQ-E: Branch A (independent) → ship the production canary; Branch B (co-dependent) → narrow to the test-time invariant only and drop the production signal. The test-time zero-tolerance invariant (`stamp_miss == 0`, FR-10) ships either branch; only the production canary's existence is probe-gated. Correctly bounded and disclosed.
3. **C-13 / OQ-D — marker-recovery follow-up issue — VERIFIED.** Filed as **#700**, carrying the binding crt-052 snapshot-seam dependency (consume `take_transcripts_for_feature`, never a second `contiguous_tail` reader). ADR-007 §4 cites it concretely. Tracking action complete.
4. **SR-01 / R-08 — uncontracted Claude Code behavior pinned to claude 2.1.167 — ACCEPT.** Attribution rests on `--resume` session_id reuse and depth-1 root-id inheritance, empirical on claude 2.1.167 only; mitigated by the zero-tolerance `stamp_miss` invariant (drift surfaces as a nonzero counter, no threshold/denominator/baseline) and the re-run-AC-06-fixtures-on-CLI-bump check. The compounding size-gate external dependency (SR-02) is now **RESOLVED** by the vnc-027 merge. Hold the pinned delivery order; treat the canary as a release-blocking observable.

Net: no FAIL, no VARIANCE requiring human approval; the lone WARN (ADR doc-sync) is already resolved; remaining live residuals are the OQ-E delivery probe and the SR-01 CLI-pinning accept.

## Open Items for Delivery to Resolve (do NOT invent answers)

- **OQ-A — `topic_source='vote'` row-level write site.** Majority vote today resolves the **session** feature at close/sweep, not rows. Delivery must pin, at the exact code site, whether any path writes vote-derived attribution at row level — or record that `vote` rows are reachable only via the `Inferred(Voted)` registry-fill path (ADR-005 taxonomy maps `vote` to extracted-None + `Inferred(Voted)` fill). Confirm FR-21's one-source-per-write-site rule holds before closing Gate 3.
- **OQ-E — canary independence crux (ADR-006 §7 / spec OQ-E, delivery-probe-resolved).** The subagent-gated production increment (FR-09) assumes "I am a subagent" (depth≥1 / subagent-context detection) is an **independent** client-side signal from root-id inheritance. Delivery probes whether SubagentStart / depth indicators survive on hook stdin under a simulated/aged CLI where root-id inheritance is absent. **Branch A** (signals independent) → ship the client-side production canary (primary recommendation). **Branch B** (co-dependent, break together) → narrow the canary to the **test-time invariant only** and DROP the production signal (do not ship a tripwire noisy by construction). The **test-time zero-tolerance invariant (FR-10) ships either way**; only the production canary's existence is gated on this probe.
- **#574 expiry check** — if #574 merges before vnc-030 delivers, re-verify (a) `cycle_events` rows still carry the windowing timestamps and (b) client-side `context_cycle` PreToolUse interception still fires (ADR-007 §3).
- **Migration version collision (R-11)** — at delivery, confirm no other landed migration claims v28; the version number must be unique against the rebased main.
- **#588 close decision** — close #588 via vnc-030's PR if the enumerated residue list (historical extracted rows, Rust-hook per-row tallies, scenario-16 hookless) is accepted as named non-goals; otherwise convert residue to a named follow-up. PR description must carry the resolved/residue list.
