# vnc-030 Architecture — Contractual Cycle Attribution (F4b)

GH Issue: #699 · Inputs: SCOPE.md (approved 2026-06-08), SCOPE-RISK-ASSESSMENT.md, ass-072 FINDINGS (GO design — client-state-lifecycle spec implemented verbatim), cwd probe report (`agents/vnc-030-cwd-probe-report.md`).

## System Overview

Feature attribution today is ~90% inference: client extraction (`topic-signal.js` / `attribution.rs`) → server registry fill (`enrich_topic_signal`) → eager attribution → majority vote at close/sweep. Two server sites let the vote beat the declared feature (`infra/session.rs:628`, `listener.rs` close path), and 20.2% of live observations carry an extracted signal contradicting the session's declared feature.

vnc-030 makes attribution **contractual** for every declared session:

1. **Client** (TS hook client, `lib/hook-client/`): a per-session cycle tracker file written at `context_cycle` interception; every fire-and-forget event carries a `cycle_stamp` while the tracker exists; extraction is suppressed while stamped; a `stamp_miss` canary detects Claude Code inheritance drift.
2. **Wire**: one additive optional field, `ImplantEvent.cycle_stamp: Option<CycleStampPayload>` — frozen-F1-safe, 7th ts-rs export.
3. **Server**: presence-gated precedence chain **stamp → marker → vote-on-NULL**; `FeatureSource::{Declared, Inferred}` precedence classes; both declared-vs-vote inversions flipped; additive `observations.topic_source` column as the F6 (#682) evidence base.
4. **Heuristics demoted, never deleted**: NULL-fill, eager, and vote survive as the floor for never-declare sessions (60% of sessions / 25% of observations).
5. **Protocols**: restart re-declaration line in design/delivery/bugfix protocols.

Mixed stamped/unstamped clients coexist by construction — stamp presence is per-event and self-describing; no feature flag; Rust `hook.rs` untouched.

## Component Breakdown

| # | Component | Change | Responsibility |
|---|-----------|--------|----------------|
| C1 | `lib/hook-client/cycles.js` | **NEW** | Cycle tracker file lifecycle: `cycles/{session_key}.json` read/write/update/delete + 7-day prune; reuses `state.js` atomics/sanitize. ADR-001. |
| C2 | `lib/hook-client/index.js` | extend | FNF-path stamp decoration: cycle lifecycle dispatch (create/update/delete keyed on CYCLE_* frames), stamp attach + extraction suppression on RecordEvent/RecordEvents frames, canary check on miss. ADR-002. |
| C3 | `lib/hook-client/state.js` | extend | `stamp_miss` counter in the `health.json` content-free breadcrumb (zeroed default, field-by-field degrade). ADR-006. |
| C4 | `crates/unimatrix-engine/src/wire.rs` | extend | `CycleStampPayload` struct + `ImplantEvent.cycle_stamp` optional field; 7th ts-rs export wired into the export sentinel test. ADR-003. |
| C5 | `crates/unimatrix-server/src/infra/session.rs` | extend | `FeatureSource` enum + `SessionState.feature_source`; `apply_stamp()`; `sweep_stale_sessions` inversion flip (line 628). ADR-004. |
| C6 | `crates/unimatrix-server/src/uds/listener.rs` | extend | Stamp read at all three record sites (~:719, ~:861, ~:1042 batch); `topic_source` assignment per row; close-path inversion flip (~:1950-1978); `ObservationRow.topic_source`; both local INSERT statements gain the column. ADR-004, ADR-005. |
| C7 | `crates/unimatrix-store/src/migration.rs` | extend | v27→v28: pragma-guarded `ALTER TABLE observations ADD COLUMN topic_source TEXT` (pattern #4092/#1264, v9→v10 `topic_signal` precedent at migration.rs:219-237). ADR-005. |
| C8 | `.claude/protocols/uni/uni-{design,delivery,bugfix}-protocol.md` | extend | One restart re-declaration line each: on re-entering a broken session, the leader's first action is `context_cycle(type:"start", topic:"{feature-id}")` (idempotent — `AlreadyMatches` server-side; recreates the client tracker). AC-09. |
| C9 | `attribution.rs` + `topic-signal.js` docstrings | drive-by | Correct the misleading `{alpha}-{digits}` docstrings (OQ2 resolution: `is_valid_feature_id` has no digit requirement). Comment-only. |

## Component Interactions / Data Flow

```
context_cycle(start) tool call
  → PreToolUse hook → buildCycleEventOrFallthrough (post-validation)   [C-level: unchanged]
  → index.js: CYCLE_START frame detected → cycles.writeCycle(topic, next_phase)   [C1+C2]
  → frame decorated with cycle_stamp, sent FNF                          [C2]

every subsequent FNF RecordEvent/RecordEvents
  → index.js: one cycles/{key}.json read
      file present → attach cycle_stamp{topic,phase} to each ImplantEvent;
                     strip topic_signal from non-CYCLE_* frames (suppression, AC-03)
      file absent  → subagent-context event (depth ≥ 1) whose carried root id has no tracker → state.bumpStampMiss (inheritance drift); depth-0 never-declare → no increment   [C3]

server record path (all 3 sites)                                        [C6]
  event.cycle_stamp Some → topic_signal := stamp.topic; phase := stamp.phase ∥ registry phase;
                           topic_source := 'declared'; registry.apply_stamp(sid, topic);
                           SKIP record_topic_signal tally; SKIP enrich
  event.cycle_stamp None → legacy chain with FeatureSource guard (see ADR-004 decision table)

session close / sweep                                                   [C5+C6]
  feature_source == Declared && feature Some → declared wins (inversion fixed)
  else → vote → content fallback → registry feature   (today's order, now NULL-gated)
```

`context_cycle(phase-end)` → `cycles.updatePhase(next_phase)`. `context_cycle(stop)` → `cycles.deleteCycle`. SessionStart / SessionClose / Stop **never** touch the tracker (Stop fires per assistant turn — copying the offsets delete-on-close lifecycle would kill the stamp after turn 1; binding SCOPE constraint).

## Technology / Design Decisions

| Decision | ADR |
|---|---|
| Tracker file lifecycle, placement, prune, worktree path routing | ADR-001 |
| Stamp attach as FNF-path decoration in index.js (buildRequest stays pure); suppression by strip; delta frames unstamped | ADR-002 |
| `ImplantEvent.cycle_stamp` wire shape, ts-rs export, frozen-F1 additivity, end-to-end round-trip AC (#3486) | ADR-003 |
| `FeatureSource` precedence classes, `apply_stamp`, the two inversion flips, registry-touchpoint fence (SR-05) | ADR-004 |
| `topic_source` value taxonomy + v27→v28 migration (SR-04) | ADR-005 |
| `stamp_miss` canary as a subagent-gated zero-tolerance inheritance-drift invariant (no threshold), pinned CLI version, depth-0 structural-noise non-increment (SR-01) | ADR-006 |
| Cross-feature seam contracts: vnc-027 interception seam survival (SR-09), crt-052 close/sweep interface (SR-10), #574 no-race + expiry (SR-12), marker-recovery follow-up contract (SR-07) | ADR-007 |

## Integration Points

- **vnc-027 (F4a — MERGED 2026-06-08)**: shares `build-request*.js`/`index.js`; owns the redefined size gate (its ADR-005, entry #4806; `check-hook-client-size.js:34-35`, 100,000 stripped / 160,000 raw) that this feature's client additions depend on — now live. Its ADR-004 hook-set reduction narrowed the PreToolUse install matcher to `context_cycle|mcp__unimatrix__context_cycle` (`merge-settings.js:49`) and introduced a `null` no-send sentinel (`build-request-tools.js:326`, short-circuited at `index.js:366`) — vnc-030 rebases onto the merged tree, adding logic to `index.js` orchestration only. It also landed the **UDS transport** (`transport-uds.js`); the stamp must be proven over UDS as well as HTTP (post-merge obligation owed to #699 — ADR-002 §7). Seam-survival contract: ADR-007 §1.
- **crt-052 (delivers after vnc-030)**: consumes the post-fix close/sweep semantics (its session selection relies on the inversion fixes); edits `drain_and_signal_session`/`clear_transcripts_for_feature` in the same files. Citable interface: ADR-007 §2.
- **#574 (cycle_events write relocation)**: no-race confirmation + assumption expiry: ADR-007 §3.
- **Marker recovery (deferred, SR-07)**: named follow-up issue must consume crt-052's transcript snapshot seam (`take_transcripts_for_feature` cycle-review snapshot path), never a second `TranscriptBuffer::contiguous_tail` reader (module doc pin at `infra/session_transcript.rs:10`). ADR-007 §4.
- **F6 (#682)**: `topic_source` distribution is the retirement-gate evidence base; taxonomy fixed in ADR-005.
- **F3 worktree gitdir port** (config.js `walkToProjectRoot`/`resolveGitFile`, landed b2e215fd): verified in production by the cwd probe — hook cwd carries the **worktree** path; the port resolves it to the main root. **All stamp paths route through `config.resolve(cwd)`'s `stateDir` — never hash raw cwd** (probe report implication; AC-08 asserts existing behavior via a stamp-path regression test).

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| Tracker file | `~/.unimatrix/{projectHash}/hook-client/cycles/{sanitizeSessionKey(sid)}.json` → `{"topic": string, "phase": string\|null, "declared_at": secs, "updated": secs}` | ADR-001 (ass-072 Q1 spec) |
| `cycles.js` API | `readCycle(stateDir, sid) -> {topic,phase}\|null` · `writeCycle(stateDir, sid, topic, phase) -> bool` · `updatePhase(stateDir, sid, phase) -> bool` · `deleteCycle(stateDir, sid) -> bool` · `pruneCycles(stateDir)` — all never-throw (F3 C-05) (canary reads the carried root tracker via `readCycle`; the removed `anyOtherCycleFile` concurrent-file check is gone per ADR-006 §2) | ADR-001/002 |
| `state.js` addition | `bumpStampMiss(stateDir) -> bool`; breadcrumb default gains `stamp_miss: 0` | ADR-006 |
| Wire struct | `pub struct CycleStampPayload { pub topic: String, #[serde(default, skip_serializing_if = "Option::is_none")] pub phase: Option<String> }` — ts-rs derive, `export_to = "../bindings/"` | ADR-003 |
| Wire field | `ImplantEvent.cycle_stamp: Option<CycleStampPayload>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; JS attaches `cycle_stamp: {topic, phase?}` (keys omitted when null — `implantEvent` omit-when-null parity) | ADR-003 |
| Transport ride (HTTP+UDS) | Decoration mutates the in-memory `request` before `selectTransport` (`index.js:410`); both transports `JSON.stringify` the same object — HTTP `transport-http.js:74`, UDS `transport-uds.js` `encodeFrame:62`. `cycle_stamp` is byte-identical on both wires + queue replay. UDS-path stamp regression seam: `transport-uds.encodeFrame` (`:55-62`) | ADR-002 §7 |
| Registry enum | `pub enum FeatureSource { Declared, Inferred(InferredOrigin) }` · `pub enum InferredOrigin { Registered, Voted }` · `SessionState.feature_source: FeatureSource` (default `Inferred(Registered)`) | ADR-004 |
| Registry method | `SessionRegistry::apply_stamp(&self, session_id: &str, topic: &str)` — idempotent; sets `feature`+`Declared`; no-op when equal | ADR-004 |
| Inversion fix 1 | `sweep_stale_sessions` (session.rs:628): `if matches!(state.feature_source, FeatureSource::Declared) && state.feature.is_some() { state.feature.clone() } else { majority_vote_internal(..).or_else(\|\| state.feature.clone()) }` | ADR-004 |
| Inversion fix 2 | `process_session_close` (listener.rs:1950-1978): snapshot `feature_source` with the existing `get_state` capture (:1892); declared-and-present short-circuits vote and content fallback | ADR-004 |
| Row struct | `ObservationRow.topic_source: Option<String>`; both listener-local INSERTs (:3015, :3055) gain `topic_source` as `?10` | ADR-005 |
| Column | `observations.topic_source TEXT NULL` — values `'declared' \| 'extracted' \| 'registry-fill' \| 'vote' \| NULL`; migration v27→v28, `CURRENT_SCHEMA_VERSION = 28` | ADR-005 |
| Canary invariant | Zero-tolerance: `stamp_miss == 0` at test-time AND production. Increment **iff** a subagent-context event (depth ≥ 1) carries a root session_id with no `cycles/{session_key}.json` tracker = inheritance drift; depth-0 never-declare is **NOT** counted. No ratio / no `fnf_record_send_count` denominator / no threshold / no per-deployment baseline / no human re-set. Nonzero growth → investigate inheritance drift. Pinned verified CLI: claude 2.1.167 | **ADR-006 (authoritative)** |
| Protocol line | "On re-entering a broken session, the leader's first action is to re-issue `context_cycle(type:"start", topic:"{feature-id}")`" — all three protocol files | AC-09 |

## Client Size Budget (SR-02 — RESOLVED: gate merged, baseline measured against the live tree)

The gate dependency SR-02 flagged is **RESOLVED**: vnc-027 merged 2026-06-08, and its redefined size gate is live at `packages/unimatrix/test/check-hook-client-size.js` — `PRIMARY_LIMIT = 100000` (comment-stripped bytes, `:34`) and `BACKSTOP_LIMIT = 160000` (raw on-disk bytes, `:35`). (The gate's own header labels it the "C-04 hook-client size gate, vnc-027 ADR-005" — entry #4806; earlier notes that called it "C-05" conflated it with F3's fail-open constraint C-05.) Baseline below is the gate's **actual** output run against the merged tree on 2026-06-08, not an estimate:

| Quantity | Raw (B) | Stripped (B) |
|---|---|---|
| Current `lib/hook-client/` total (merged post-vnc-027, gate-measured) | 112,773 | 68,907 |
| **vnc-030 additions** | **~3,900** | **~2,050** |
| — `cycles.js` (new) | ~2,200 | ~1,100 |
| — `index.js` decoration + lifecycle | ~1,400 | ~800 |
| — `state.js` stamp_miss | ~300 | ~150 |
| Projected post-vnc-030 totals | ~116,700 | ~71,000 |
| Live gate limits (`check-hook-client-size.js:34-35`) | 160,000 backstop | 100,000 primary |

Headroom after vnc-030 is comfortable on both axes (~43 KB raw, ~29 KB stripped). **The order pin is now discharged**: vnc-030's ~3,900 B raw additions fit **only** under this redefined gate. They could **not** have landed under the old raw-100,000 B gate (the pre-vnc-027 tree already sat at ~99,997 B raw — 3 bytes of headroom), which is exactly why vnc-027 was pinned to deliver first. With the comment-stripped primary now the governing axis, oracle-citation comments no longer compete with code for the budget. **Documented fallback** if a later vnc-027 follow-up tightens the raw axis: fold `cycles.js` into `state.js` (eliminates the new module's header/exports overhead, ~400 B raw) — the lifecycle helpers already reuse `state.js` atomics/sanitize, so the merge is low-friction; oracle citations can additionally move to OVERVIEW.md for raw-only, stripped-neutral relief.

## Registry Touchpoint Fence (SR-05)

The precedence chain requires exactly four registry touchpoints — anything beyond is out of scope:

1. `SessionState.feature_source` field + `FeatureSource`/`InferredOrigin` enums.
2. Source assignment at existing set sites: `set_feature_force` (cycle_start) → `Declared`; `set_feature_if_absent` (eager, #198) → `Inferred(Voted)`; `register_session` feature param → `Inferred(Registered)`.
3. New `apply_stamp` (idempotent Declared set on stamped events — covers server restart mid-session where no post-restart cycle_start exists).
4. The two `or_else` inversion flips (sweep + close).

Named follow-ups, NOT this feature: per-turn Stop→SessionClose registry drain, `register_session` overwrite on resume/compact (ass-072 out-of-scope discoveries 2/3), `set_feature_force` absent-session no-op semantics (#4140). Note one accepted consequence: `register_session` overwrite on resume/compact resets `feature_source` to `Inferred(Registered)` — the next stamped event re-applies `Declared` via `apply_stamp`, which is precisely the resilience the stamp exists to provide.

## #588 Disposition Mapping (SR-08)

AC-04 resolves: (a) the row-level inversion for stamped traffic — structurally moot, the client emits no extracted `topic_signal` while stamped; (b) the write-time declared-vs-extracted ordering for **unstamped** declared sessions — `Declared` registry feature now wins over extraction (ADR-004); (c) both session-level vote-beats-declared sites. Residue after vnc-030: none of #588's named claims — close #588 via the PR. (Mixed-window extracted rows in sessions that never declared are scenario-6 residue, which #588 never covered.)

## Self-Imposed Constraints Carried From SCOPE

Fail-open client (F3 C-05: never throws, exit 0, no stdout on failure, no secrets); no new npm deps; sync trio (`ContextSearch`/`CompactPayload`/`Ping`) gains **zero** file I/O — the tracker read happens only on the FNF branch (one small JSON read; miss branch adds one `readdir` + health RMW, see ADR-002); Rust `hook.rs` zero changes; migration discipline per #4092 (all pragma checks before any ALTER).
