# FINDINGS: Contractual Cycle Attribution — Client-Side Stamp Lifecycle Across Session Scenarios

**Spike**: ass-072 (#694)
**Date**: 2026-06-06
**Approach**: investigation + targeted empirical checks (resume/fork run live against claude 2.1.167; worktree resolution run against real git worktrees on both the TS client and the Rust engine; live-DB measurement of attribution noise)
**Confidence**: empirical for Q3 (--resume / fork) and Q2 scenario 8 (worktree); code-traced + DB-measured for the rest
**Consumer**: F4 (#680) attribution redesign

---

## Executive Summary

**GO — the stamp should be F4's primary attribution mechanism.** It covers every scenario where a declaration exists (75% of observation volume in the live DB), survives the three lifecycle events that destroy today's server-side registry state (per-turn Stop-drain, compaction re-register, server restart), and structurally absorbs #588 — including a **second, previously under-named inversion**: at session close/sweep, the majority vote wins over the declared registry feature (`infra/session.rs:628`, `listener.rs` close path "majority vote wins").

Two hard findings temper the design:

1. **The worktree trap is real and worse than suspected** (empirical): the TS client's root walk stops at the worktree directory → per-worktree state dir (stamp file miss → unstamped) **and** per-worktree config lookup (`.claude/settings.local.json` is gitignored → absent in worktrees → remote-mode events from worktree agents are silently dropped entirely). The Rust hook does not have this trap — it resolves worktree `.git` files to the main repo root (verified by test run). F4 must port the gitdir resolution.
2. **Voting cannot be retired — only demoted to NULL-only.** 42/70 retained sessions (60% by count, 25% of observation volume) never declare a cycle (uni-zero ×25, research spikes, ad-hoc). For them the extraction+vote path is the only attribution, and it demonstrably works (this very spike session was correctly vote-attributed to `ass-072` with no declaration). It also demonstrably misfires (`SHA-256` exists as a feature_cycle in the live sessions table — the `alpha-digits` extractor matched a hash algorithm name).

---

## Findings

### Q1: Tracker mechanics and state lifecycle

**Answer**: One new per-session JSON file in the existing F3 state dir, written at `cycle_start` interception, updated at `phase-end`, deleted at `cycle_stop` — and **never** touched by SessionStart or SessionClose. The stamp rides a **new optional top-level `ImplantEvent` field**, not `topic_signal` and not a payload key. Client-side extraction **stops when a stamp is active** and continues unchanged when none is.

**State location and lifecycle**:

```
~/.unimatrix/{hash}/hook-client/
  offsets/{session_key}.json     # F3 (exists)
  queue/{ts}-{pid}-{seq}.json    # F3 (exists)
  cycles/{session_key}.json      # NEW: { "topic": "...", "phase": "...?"|null,
  health.json                    #        "declared_at": secs, "updated": secs }
```

| Event | Tracker action |
|---|---|
| PreToolUse intercepting `context_cycle(type=start)` (after `validate_cycle_params` passes — same validation the Rust hook runs, `hook.rs:782-797`) | create/overwrite `cycles/{session_key}.json` with `{topic, phase: next_phase, declared_at}` via the existing `atomicWrite` (temp+rename, `state.js:81-96`) |
| PreToolUse intercepting `type=phase-end` | update `phase` to `next_phase` (same file, atomic) |
| PreToolUse intercepting `type=stop` | delete the file |
| Every other event built by `build-request.js` | read the file (missing/corrupt → no stamp, never throws — `state.js` module contract) and attach the stamp |
| **SessionClose / Stop** | **no action** — see below |
| Crash / kill -9 | file survives on disk; `--resume` keeps the session_id (Q3) so stamping continues |
| Compaction | file untouched; stamping continues |
| Prune | age-based, 7 days since `updated` (same policy as `offsets/`) |

**Critical divergence from the offsets lifecycle**: F3 deletes `offsets/{key}.json` on successful SessionClose send (FR-16, `state.js:127-135`). The cycle tracker **must not copy this**, because the Rust hook maps the per-turn `Stop` event to `SessionClose` (`hook.rs:478` `"Stop" | "TaskCompleted" => SessionClose`) and the F3 client mirrors it. Claude Code fires Stop at the end of *every assistant turn* — copying delete-on-close would kill the stamp after turn 1. (This same per-turn close is what drains the server registry every turn — see scenario 12 — and is a large part of why the stamp is needed.) Clear is keyed to `cycle_stop` interception + age prune only.

**Stamp carriage — new optional `ImplantEvent` field** (recommended shape):

```rust
/// F4: client-declared cycle attribution (contract, not inference).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cycle_stamp: Option<CycleStampPayload>,   // { topic: String, phase: Option<String> }
```

- **Additive under the frozen F1 contract**: `ImplantEvent` already carries optional `topic_signal`/`provider` with `#[serde(default, skip_serializing_if)]` (`wire.rs:225-251`); the wire enums have **no `deny_unknown_fields`**, so an old server ignores the field and an old client omits it → `None`. New ts-rs export (7th binding) is the same additive move vnc-024 used for `TranscriptDeltaPayload` and ass-071 recommended for `SubagentTranscriptPayload`. Existing bindings/fixtures untouched.
- **Why not reuse `topic_signal` (zero wire change)**: the server could not tell contract from guess — which is the entire #588 failure class. `topic_signal` stays as the *advisory/extracted* channel; `cycle_stamp` is the *declared* channel. Presence of the field IS the declared flag.
- **Why not a payload key**: `payload` is event-type-specific and is consumed by `extract_observation_fields` per event type; a typed top-level field gives the server one read point and a contract fixture.

**Extraction disposition**: when `cycles/{session_key}.json` exists, the client **omits `topic_signal` entirely** (extraction short-circuits — nothing exists to be inverted; this is the structural #588 fix at the client edge). When no stamp file exists, extraction continues exactly as today (it feeds the vote for never-declare sessions, which Q2/Q5 show must survive). No separate advisory field is needed — `topic_signal` already is that field once it can no longer coexist with a stamp.

**Evidence**: `wire.rs:225-251`; `hook.rs:734-862` (interception: tool-name equality guard, `validate_cycle_params`, `topic_signal = topic` on cycle events); `state.js` (atomicWrite, sanitizeSessionKey, FR-16 delete-on-close); vnc-026 ADR-003 (state dir layout, F4 inherits unchanged); vnc-026 ARCHITECTURE.md `build-request.js` row (F3 ports interception + extraction, so the tracker is a ~30-line addition to an already-planned module).

**Recommendation**: implement the tracker as `cycles/{session_key}.json` beside `offsets/`, reusing `state.js` atomic-write/sanitize/prune machinery; carry the stamp as `ImplantEvent.cycle_stamp: Option<CycleStampPayload>`; suppress client extraction when stamped; do **not** clear the tracker on SessionClose.

---

### Q2: Scenario × mechanism matrix (primary deliverable)

Mechanisms: **STAMP** (write-time, contractual) → **MARKER** (review-time transcript recovery) → **VOTE** (extraction + tally, fires only on NULL) → **UNATTRIBUTED**.

| # | Scenario | Attributing mechanism | Failure mode / residue |
|---|---|---|---|
| 1 | Happy path — `cycle_start` declared, subagents stem from session | **STAMP** | None. Subagent hook events carry the parent session_id (`hook.rs:449-453`, `:712` WA-2 comment; ass-071 empirical 328/328) → same `cycles/{key}.json`. |
| 2 | Design/delivery session split | **STAMP** (each session declares its own start) | None. Verified: both protocols declare before any spawning (design protocol L58-70: "Before spawning any agents"; delivery L68: "before any agent spawning"; bugfix L70). Declarations reach interception: PreToolUse matcher is `"*"` (settings.json) and the equality guard accepts `mcp__unimatrix__context_cycle`. Design leaves the cycle open (no stop); delivery's second `cycle_start` is `AlreadyMatches`/`Overridden` server-side and a fresh tracker file client-side. `load_cycle_observations` already closes a double-start window at the second start (E-03, `observation.rs:347-355`). |
| 3 | Broken session — env failure mid-protocol, restart | **STAMP** (resume path) / **MARKER** (fresh-session path) / **VOTE** (residue) | See Q3. `--resume` keeps the session_id (empirical) → stamp file survives → no gap. Fresh session without re-declaration → marker recovery only if the restart prompt names the feature; protocols currently guarantee nothing here (gap — Q3 recommendation). |
| 4 | Concurrent sessions, different features | **STAMP** | None. State files keyed by sanitized session_id; server keying is the ass-069-validated string-keyed map (0 cross-talk at 128 concurrent sessions). |
| 5 | Sequential cycles in one session | **STAMP** per cycle | Inter-cycle gap: events between `cycle_stop` and the next `cycle_start` are unstamped → NULL → registry fill (note: server registry keeps the *stale* feature after stop — only phase is cleared, `handle_cycle_event` Step 3) or vote. Same as today; bounded because `load_cycle_observations` windows end at the stop timestamp. Acceptable residue. |
| 6 | Ad-hoc session, never declares | **VOTE** (NULL-only) or UNATTRIBUTED | The permanent residue class: 42/70 retained sessions, 8,846/35,495 observations (25%) in the live DB. Vote works here (this spike's session correctly vote-attributed to ass-072) and misfires here (`SHA-256` session in the live DB). This class is why vote is demoted, not deleted. |
| 7 | uni-zero / research sessions | Same class as 6 — **VOTE** or UNATTRIBUTED | uni-zero is the single largest session population (25/70). If signal noise in these sessions matters, the fix is protocol-side (give uni-zero a declaration), not mechanism-side. |
| 8 | Worktree-isolated agents | **STAMP after an F4 fix; today: broken on the TS client** | **Empirical, two-layer trap.** (a) `walkToProjectRoot` (config.js:54-75) stops at the dir containing `.git` *file*: worktree `/tmp/ass072-wt/wt` → hash `381ddc99…` vs main `f4e1529d…` → different state dir → stamp file miss → unstamped. (b) Worse: config resolution from the worktree returned `ok=false reason=missing` (no `.claude/settings.local.json` — gitignored, absent in worktrees) → in remote mode **every hook event from a worktree agent is silently dropped**, stamp or no stamp. The Rust hook is immune: `detect_project_root` resolves the gitdir pointer to the main root (throwaway cargo test: both paths → `/tmp/ass072-wt/repo`, same hash) — which is why worktree-agent observations (e.g. `.claude/worktrees/agent-a663dd9e…` commands, 629 rows) appear in the main live DB today. **F4 must port the Rust gitdir resolution into the TS root walk** (replacing the divergence documented at config.js:46-50). |
| 9 | Compaction mid-session | **STAMP** | session_id persists across compaction (corpus: 86/86 local transcripts have a single internal sessionId equal to the filename, including long compacted sessions) → tracker file untouched. Server-side today actually *degrades* at compaction: Claude Code fires SessionStart(source=compact) → `SessionRegister` → `register_session` **overwrites** state — feature, phase, topic tallies all reset (`infra/session.rs:193-223`). The stamp is indifferent; this scenario favors it outright. |
| 10 | Server restart mid-session | **STAMP** (confirmed superior) | Registry is in-memory only; after restart every record path's registry lookup is a silent no-op and `enrich_topic_signal` returns None → today's rows go NULL/extracted until eager vote re-accumulates. Observation inserts themselves don't need the registry, so stamped events keep attributing correctly from the first post-restart event. Confirmed: the scenario favors the stamp over registry enrichment. |
| 11 | Subagent spawned before `cycle_start` lands | Pre-declaration events: **VOTE/NULL**; post-declaration: **STAMP** | Race is protocol-violation-shaped: all three protocols order declaration before spawning. Client-side ordering is safe: the stamp file is written at PreToolUse of `context_cycle` — i.e. before the tool even executes — and reads see old-or-new atomically (rename). Residue: only events emitted before the declaration instant, identical to today. |
| **12** | **(found) Multi-turn session — per-turn Stop drain** | **STAMP** | Missed by the SCOPE list and the strongest single argument for the stamp. The Rust hook maps `Stop` (fires at the end of *every* assistant turn) to `SessionClose`; `process_session_close` → `drain_and_signal_session` **removes the registry entry** every turn (Unimatrix lesson #4134 confirms: "cycle_start fires for a session already drained… drain_and_signal_session ran on a prior Stop"). Turn N+1 events hit an empty registry until the #519 pre-register or eager vote re-fires. The disk-persisted stamp is immune to turn boundaries. |
| **13** | **(found) `/clear` or fresh session, same checkout** | VOTE or UNATTRIBUTED | New session_id, no declaration → scenario-6 class until the human/protocol re-declares. |
| **14** | **(found) `--resume --fork-session`** | **MARKER** | Empirical: fork mints a new session_id (`0abd4d20…` from `d081b517…`) but the forked transcript contains the **full prior conversation rewritten under the new id** — including any `context_cycle` tool_use blocks. Stamp file misses (new key), but review-time marker recovery has everything it needs, and F3's delta streaming from offset 0 even puts those markers in the server's F2 buffer. |
| **15** | **(found) Subagent itself calls `context_cycle`** | STAMP — but it **hijacks the parent's stamp** | Because subagent events inherit the parent session_id, a subagent's declaration overwrites `cycles/{parent_key}.json` for the whole session (exact analogue of today's `set_feature_force` hijack). Protocols only let the SM declare; F4 should state this as a contract note. No mechanism fix recommended — same exposure as today, now at least visible in the `Overridden` log. |
| **16** | **(found) MCP-only client, no hooks (e.g. bare remote Gemini)** | UNATTRIBUTED — by construction | The MCP `context_cycle` handler is session-unaware: "Attribution is applied via the hook path (fire-and-forget)" (`mcp/tools.rs:3284-3400`). With no hook client there are no events to attribute at all; neither stamp nor marker nor vote can run. Named, not solved (W2 exposure from #588 stands until such clients run the TS hook client). |
| **17** | **(found, amendment) SM-as-subagent — parent session spawns SM-subagent which declares and spawns workers** | **STAMP** (declaration-source-agnostic) | The same mechanism as scenario 1, re-framed. The declaring thread is now a subagent, but its `context_cycle` PreToolUse carries the **root** session_id (Claude Code inheritance), so the interception writes `cycles/{root_key}.json` exactly as a main-thread declaration would. No wire/precedence/verdict change. Residual exposure (worker clobbering the SM-subagent's stamp) is the scenario-15 hijack, now the intended-vs-rogue distinction. Depth>1 inheritance is unverifiable today (constraint not lifted) → Q4 AC reworded "root session's session_id" + `stamp_miss` canary covers the drift. See Amendment. |

**Live-DB scale of what the stamp fixes** (sessions attributed to declared cycles): of 26,686 observations, **20.2% (5,403) carry an extracted `topic_signal` differing from the session's feature** (the #588 row-level inversion surface), 22.3% are NULL (registry-fill dependent), 57.5% match. Worst sessions invert outright: e.g. session `2f21bc5c…` (vnc-021) has 292 mismatching vs 43 matching signals — only the declaration saved its session-level attribution; the vote would have lost it.

---

### Q3: Broken-session capture paths (empirical)

**Answer**: `--resume` keeps the session_id — the stamp survives a crash with **zero gap**. Fresh-session restart is coverable by marker recovery only if the restart names the feature; the protocols currently guarantee nothing, which is the one real gap to close.

**Path 1 — `--resume` (empirical, claude 2.1.167)**:
- `claude -p` session → `session_id d081b517-dee8-460b-813b-e5d98d953dc1`; `claude -p --resume d081b517…` → **same session_id**, appended to the **same transcript file** (19 lines, single internal sessionId).
- `claude --help` confirms this is the contract, not an accident: `--fork-session — When resuming, create a new session ID **instead of reusing the original**`. Reuse is the default.
- Corroboration at corpus scale: all 86 local project transcripts have exactly one internal `sessionId`, equal to the filename — zero forked/mismatched sessions in real usage.
- Consequence: `cycles/{session_key}.json` (disk) is found on the first post-resume event; stamping continues. Note the asymmetry: the **server** does *not* survive the same crash+resume — SessionStart(source=resume) re-registers and wipes the registry feature (`register_session` overwrite), so today's enrichment path breaks exactly where the stamp doesn't.

**Path 2 — fresh-session marker recovery**:
- `--fork-session` (empirical): new id, but the fork's transcript carries the complete prior conversation — `cycle_start` tool_use blocks and protocol headers included — rewritten under the new id. Marker recovery at review time attributes it; F3's offset-0 delta stream even delivers those markers into the F2 server buffer for live recovery.
- Genuinely-fresh restart session: the transcript contains only what the human types. The protocols define **no restart prompt format** (verified: no restart/re-entry section in design/delivery/bugfix protocols). Today this works by convention ("continue delivery of vnc-026" → extraction picks up `vnc-026` → vote), i.e. by luck.
- **What the protocol must guarantee**: a one-line addition to all three protocols — *on re-entering a broken session, the leader's first action is to re-issue `context_cycle(type:"start", topic:"{feature-id}", …)`*. This is idempotent server-side (`AlreadyMatches`) and recreates the client tracker, converting the entire fresh-restart class from MARKER/VOTE back to STAMP. Cost: one tool call.

**Residue — neither path**: a fresh session (no resume) + no re-declaration + restart prompt that never names the feature. With the protocol guarantee above, this residue requires *two* simultaneous protocol violations; without it, it is any restart where the human's prompt omits the feature-id and the conversation never mentions one extractable. Not directly measurable from the retained corpus (no broken-session ledger exists); bounded qualitatively as small-but-real → it lands in the VOTE/UNATTRIBUTED bucket by design, which is exactly what NULL-only voting is kept for.

**Recommendation**: rely on `--resume` id-persistence as a verified property (add it to F4's test assumptions, with a canary — see Q4); add the restart re-declaration line to the three protocols as an F4-adjacent docs change; do not build any client-side crash-recovery machinery beyond the existing disk file.

---

### Q4: Precedence chain design

**Answer — the chain**:

```
1. STAMP   (write-time)  ImplantEvent.cycle_stamp present
                         → observations.topic_signal := stamp.topic, phase := stamp.phase
                         → registry: set_feature_force-equivalent (declared)
                         → extraction/vote do not run for this event (client already omitted topic_signal)
2. MARKER  (review-time) no stamp on the rows: cycle_review reads cycle_start/protocol
                         headers from the F2 transcript buffer / cycle_events windows
3. VOTE    (write+close) fires ONLY where the attribution is NULL:
                         - write-time: enrich_topic_signal NULL-fill (survives)
                         - eager: already NULL-only (check_eager_attribution returns None
                           when feature.is_some() — session.rs:495-498; set_feature_if_absent
                           no-ops on set features)
                         - close/sweep: vote consulted ONLY when no declared feature exists
4. UNATTRIBUTED — accepted for scenario 6/7/13/16 residue
```

By contract the vote can never override a stamp or marker because it is only consulted at NULLs — precedence is structural (presence-gated), not ordered (re-orderable).

**Server-side consequences, mechanism by mechanism**:

| Mechanism | Disposition at F4 |
|---|---|
| `enrich_topic_signal` NULL-fill (`listener.rs:148-180`) | **Survives** — it is the write-time floor for unstamped sessions (Rust hook until F6, never-declare sessions forever). Its AC-08 "extracted wins" arm is the part that dies: implement #588's `FeatureSource::{Declared,Inferred}` flag so declared registry features win over extraction for unstamped clients during the mixed window. |
| `check_eager_attribution` + `set_feature_if_absent` (#198) | **Unchanged** — already NULL-only by construction. Shrinks naturally as stamp coverage grows. |
| `majority_vote_internal` at close/sweep | **Demote to NULL-only.** Two inversion sites found: `sweep_stale_sessions` resolves `majority_vote_internal(...).or_else(|| state.feature.clone())` (`infra/session.rs:628`) and `process_session_close` ("majority vote wins, else fallback to feature") — **the vote currently beats the declared feature at session close**. Flip both to `declared_feature.or_else(vote)` when the feature source is Declared. This is the session-level #588 twin and must land with F4 regardless of the stamp. |
| #588 priority ordering | **Becomes structurally moot for stamped sessions** — the client emits no extracted `topic_signal` when a stamp is active, so there is no extracted/declared pair left to mis-order at any server site (write-time or close-time). For unstamped sessions in the mixed window, the FeatureSource fix above is the residual #588 remedy. Confirmed structurally, not by re-ordering. |
| Topic-tally accumulation (`record_topic_signal`) | Keep; it only receives signals from unstamped events, so it self-scopes to the residue class. |

**Parent-session-id contract — the AC + test F4 must carry** (the stamp's load-bearing dependency; today emergent Claude Code behavior, silently wrong if it changes):

> **AC**: Every hook event emitted from any subagent context (PreToolUse/PostToolUse/SubagentStop fired inside a Task), **at any nesting depth**, carries the **root session's** `session_id` — i.e. the session_id of the top-level interactive session, not of an intermediate subagent parent — and its observation row joins to the root session row. (Today Claude Code's one-level constraint makes "root" and "parent" identical; the AC is worded to "root" so it stays correct if subagent-spawns-subagent is ever enabled — see scenario 17.)
>
> **Test (integration, F4 parity layer)**: register session S; simulate a subagent tool event whose stdin `session_id` = S (fixture mirroring real Claude Code SubagentStart/PostToolUse captures); assert (a) the observation row's `session_id` = S, (b) the event was stamped from `cycles/{S}.json`, (c) a *control* event with an unknown session_id produces a row with **no stamp** and increments a client-side `stamp_miss` counter. (If depth>1 ships, add a fixture whose stdin carries a grandchild's `session_id` ≠ S and assert it lands in `stamp_miss`, not silent loss — the canary is the depth>1 tripwire.)
>
> **Canary (runtime)**: the client increments a content-free `stamp_miss` field in `health.json` whenever a non-SessionStart event finds no `cycles/` file while at least one cycles file exists for another session in the same state dir. A drift in Claude Code's inheritance behavior — including a future depth>1 world where a grandchild event carries an intermediate (non-root) session_id — surfaces as a climbing counter instead of silent attribution loss.

**Evidence**: `hook.rs:449-453` (+`:712`), ass-071 Q1 (328/328 sidechain join via parent id), `infra/session.rs:415-444/486-517/628`, `listener.rs:148-180/811-890/1890-1950`, GH #588 body (corrected-impact section), live-DB inversion measurements (Q2).

---

### Q5: Transition compatibility — mixed clients until F6

**Answer**: The chain holds with mixed clients **by construction**, because stamp presence is per-event and self-describing — no deployment-wide mode, no feature flag needed.

- **Stamped sessions (TS client, F4+)**: write-time contract; vote never consulted.
- **Unstamped sessions (Rust `hook.rs` local UDS, until F6)**: exactly today's pipeline — cycle interception still happens (the Rust hook intercepts and the server still runs `set_feature_force`), registry NULL-fill still fills, vote still resolves at close — but with the two declared-vs-vote inversions fixed (Q4), so even unstamped declared sessions stop being vote-overridable. The Rust hook needs **zero changes**.
- **Wire safety both directions**: old server + stamping client → unknown field ignored (no `deny_unknown_fields`); new server + Rust hook → `cycle_stamp: None` → falls through to the legacy chain. The frozen F1 fixtures never see the field (skip_serializing_if).

**Verdict — what retires when**:

| What | F4 | F6 gate | Ever deleted? |
|---|---|---|---|
| AC-08 "extracted wins over declared" rule | **Retired** (replaced by FeatureSource precedence) | — | Yes — it is a precedence bug, not a mechanism |
| Vote-beats-declared at close/sweep | **Retired** (declared wins) | — | Yes — same class |
| Client topic extraction | Suppressed when stamped; active otherwise | Rust hook's copy retires with hook.rs | No (TS keeps it for undeclared sessions) |
| `enrich_topic_signal` NULL-fill | Kept | Kept (undeclared sessions still exist post-F6) | No |
| Eager attribution + majority vote | **Demoted to NULL-only** (mostly already are; close-path fix makes it total) | Re-assess surface size; scenario 6/7 residue (60% of sessions, 25% of observations today) means the answer is almost certainly *keep* | No — demote-don't-delete holds on the evidence |
| Registry-enrichment as *primary* for declared sessions | Demoted to fallback behind the stamp | Becomes residual (only undeclared sessions) | No |

**Config/flag**: none. The only knob worth adding is observability — count rows by attribution source (`declared` / `extracted` / `registry-fill` / `vote` / NULL) so the F6 retirement gate has data instead of vibes; an additive `observations.topic_source` column (schema, not wire) is the cheap way to get it.

---

## Client State Lifecycle Spec (consolidated)

```
CREATE   cycle_start interception (PreToolUse, post-validation)  → cycles/{key}.json atomic write
UPDATE   phase-end interception                                  → phase := next_phase
CLEAR    cycle_stop interception                                 → delete file
READ     every built event                                       → attach cycle_stamp if file present
IGNORE   SessionStart (startup/resume/clear/compact), SessionClose/Stop  → never touch the file
CRASH    file persists; --resume reuses session_id → stamping continues (empirical)
FORK     new session_id → file miss → marker recovery (forked transcript carries full history)
COMPACT  same session_id → unaffected (empirical corpus: 86/86 single-id transcripts)
WORKTREE broken until F4 ports gitdir resolution into walkToProjectRoot (empirical — scenario 8)
PRUNE    7 days since `updated` (offsets policy); sanitizeSessionKey for the filename
```

---

## Go/No-Go

**GO.** The stamp is contractual for every scenario with a declaration (1, 2, 3-resume, 4, 5, 8-post-fix, 9, 10, 11-post-declaration, 12), strictly stronger than today's registry enrichment at the three state-loss events (per-turn drain, compaction re-register, server restart), and structurally closes both #588 inversion sites for stamped traffic. Preconditions F4 must carry: (1) the worktree gitdir-resolution port, (2) the parent-session-id AC + canary, (3) the close/sweep declared-wins fix, (4) the protocol restart re-declaration line, (5) the no-clear-on-SessionClose tracker rule.

**Human directive (2026-06-07)**: TS-client worktree support at **minimum parity with the current Rust hook** is an absolute MUST-have, not a negotiable precondition. The parity bar is what the Rust hook does today: worktree-agent events resolve to the main project root and are captured (629 worktree-agent rows in the live DB). This has two components with different homes:
- **Event capture parity (F3-blocking)**: the remote-mode worktree event loss (Out-of-Scope Discovery 1 — missing gitignored `.claude/settings.local.json` → events silently dropped; plus per-worktree state/queue/offsets dir) is a *regression vs. the Rust hook* and must be fixed in the F3 client, not deferred to F4. Without it the TS client ships below parity.
- **Stamp coverage (F4 precondition 1)**: the gitdir-resolution port into `walkToProjectRoot` — unchanged, but now anchored to the parity mandate rather than only to attribution coverage.

---

## Unanswered Questions

1. **Does the hook stdin `cwd` for worktree-isolated subagent events actually carry the worktree path?** The trap analysis assumes yes (Claude Code documents cwd as the invoking process's cwd, and worktree agents run in the worktree). The *resolution divergence* is empirically proven either way, but the live exposure rate depends on this. One-line stderr dump in a worktree agent at F4 design time settles it.
2. **Interactive-mode `--resume` parity** — the empirical resume test ran headless (`-p`). The CLI help text states reuse-by-default for both modes, and the 86-transcript corpus shows zero forked ids in real interactive usage, but a direct interactive verification was not possible from inside this session.
3. **Fraction of broken sessions with neither recovery path** — no broken-session ledger exists to measure against; bounded qualitatively (requires fresh session + no re-declaration + feature never named). Becomes measurable once the `topic_source` column ships.
4. **uni-zero attribution provenance** — 25/70 sessions carry `feature_cycle = "uni-zero"`, which the `alpha-digits` extractor cannot produce. Something attributes them (SessionStart `feature_cycle` extra? agent-declared?); not chased — irrelevant to the stamp design but worth knowing before F6 retirement metrics treat them as "vote successes."

## Out-of-Scope Discoveries

1. **Remote-mode worktree event loss is a live F3 bug class, independent of attribution**: worktrees have no `.claude/settings.local.json` (gitignored) → `config.resolve()` returns `missing` → every event from a worktree agent is dropped (breadcrumb-only). Even with env-var config, state/queue/offsets land in a per-worktree dir. Carry to F3 review or F4 scope — it silently blinds remote observation for exactly the delivery sessions that matter most.
2. **The per-turn Stop→SessionClose drain** wipes registry state every assistant turn (lesson #4134 corroborates). Today's attribution survives multi-turn sessions only via DB persistence + re-votes. Independent of the stamp, this seems worth a deliberate design pass — several SessionState consumers (injection history, co-access, rework events) presumably reset per turn too.
3. **SessionStart(source=compact/resume) `register_session` overwrite** resets feature/phase/tallies/goal mid-session. The col-025 goal-resume lookup paper-covers goals only when a feature was passed at register (it usually isn't). A `source`-aware register (preserve state on resume/compact) would fix a family of mid-session amnesia bugs.
4. **`extract_feature_id_pattern` matches any `alpha-digits` token** — live DB contains `SHA-256` as a session feature_cycle. A denylist or structural tightening would cut vote noise for the residue class cheaply.
5. **F3's delete-offset-on-SessionClose (FR-16) fires per turn** (Stop→SessionClose), forcing a full re-stream from offset 0 each turn. Idempotent but wasteful; F3/F4 may want the offset delete keyed to TaskCompleted or age-prune only.

## Recommendations Summary

- **Q1**: Tracker = `cycles/{session_key}.json` beside F3's `offsets/`, reusing `state.js` atomics; create on `cycle_start` interception, update on `phase-end`, delete on `cycle_stop`, **never** on SessionStart/SessionClose. Carry the stamp as a new additive `ImplantEvent.cycle_stamp: Option<CycleStampPayload{topic, phase}>` (7th ts-rs export; old bindings untouched). Suppress client topic extraction while a stamp is active; keep it otherwise.
- **Q2**: Stamp attributes scenarios 1/2/4/5/9/10/12 outright and 3/11 post-declaration; scenario 8 requires F4 to port Rust gitdir resolution into `walkToProjectRoot` (empirically proven trap, plus a remote-mode event-loss bug); scenarios 6/7/13 remain VOTE-or-unattributed by design; 14 (fork) is MARKER; 16 (MCP-only, hookless) is unattributable by any mechanism. Five scenarios found beyond the SCOPE list (12-16); 12 (per-turn registry drain) is the strongest pro-stamp evidence.
- **Q3**: Empirical GO on resume — `--resume` reuses the session_id (verified live; corpus-corroborated 86/86), so the stamp survives crashes with zero gap; fork-resume is covered by marker recovery (full history under the new id, verified). Close the fresh-restart gap with one protocol line: re-issue `context_cycle(start)` on re-entry (idempotent).
- **Q4**: Chain = stamp → marker → vote-on-NULL, presence-gated (structurally un-invertable). Keep `enrich_topic_signal` NULL-fill; eager is already NULL-only; **fix the second inversion** — majority vote currently beats the declared feature at close/sweep (`session.rs:628` + close path) — declared must win. #588 goes structurally moot for stamped traffic; ship the FeatureSource flag for the unstamped window. Carry the parent-session-id AC + integration test + `stamp_miss` canary.
- **Q5**: Mixed clients coexist by construction (per-event self-describing stamp; tolerant wire both directions); no feature flag. Retire at F4 only the two precedence bugs; demote vote to NULL-only permanently — scenario 6/7 residue (60% of sessions, 25% of observations) means full retirement never clears the evidence bar, including at F6. Add an additive `observations.topic_source` column so the F6 gate decides on data.
- **Go/no-go**: **GO** — stamp as F4's primary attribution mechanism, with the five named preconditions.

---

## Amendment: SM-as-Subagent Scenario (scenario 17)

**Date**: 2026-06-07 · **Trigger**: human Phase-3 gap · **Approach**: code-traced against existing findings (no re-investigation) · **Confidence**: code-traced for the source-agnosticism claim; explicitly *unverifiable* for nesting depth>1 (the Claude Code constraint that would create it is not lifted today).

**The scenario.** Unimatrix is a generic platform, not bound to today's protocol shapes. Today the primary (root) session *is* the SM, because a Claude Code subagent cannot spawn a subagent. If that constraint is lifted, the protocols invert: parent session spawns a **subagent-SM**, which spawns worker subagents — so `context_cycle(start/phase-end/stop)` would be issued primarily by a **subagent**, not the root thread. Question: does this change any analysis or decision in this document? **Verdict up front: no decision changes — wire shape, precedence chain, voting verdict, preconditions, and go/no-go all stand. The framing changes, and one AC word changes (`parent` → `root`).** The six traces:

**1 — The stamp mechanism is already declaration-source-agnostic (verified).** Scenario 15 already established that a subagent's `context_cycle` interception writes `cycles/{parent_key}.json` via parent-session-id inheritance. Re-verified the interception path fires identically for a subagent-issued MCP call: `build_request` resolves `session_id` once from `input.session_id` (`hook.rs:449-453`) and the `PreToolUse` → `build_cycle_event_or_fallthrough` path (`hook.rs:687-690`, `:734-797`) **never branches on whether the caller is the main thread or a subagent** — it keys solely off the supplied `session_id` and the `tool_name` equality guard. A subagent's PreToolUse for `mcp__unimatrix__context_cycle` carries the root session_id (Claude Code inheritance, ass-071 328/328) and is stamped to `cycles/{root_key}.json` exactly as a main-thread declaration is. So a subagent-SM declaration stamps the **whole** session correctly with zero code change. The mechanism was never "main-thread declares"; it was always "whatever thread declares, keyed by the inherited session_id." This is the property that makes the platform protocol-shape-independent.

**2 — Scenario 15's "hijack" becomes the intended path; the F4 contract note inverts.** In today's world the contract note reads "only the SM declares; a subagent declaring is a scenario-15 hijack." In the lifted-constraint world the SM *is* a subagent, so subagent declaration is **intended**. The contract note should be worded mechanism-first, not role-first:

> **F4 contract note (reworded):** Any thread in a session may issue `context_cycle`; the stamp is keyed by the inherited (root) session_id, so the declaration stamps the entire session regardless of which thread issued it. **Last declaration wins** (client overwrites `cycles/{root_key}.json` atomically; server logs `Overridden`). The protocol — not the mechanism — designates which thread is authorized to declare (today: the SM, whether root or subagent).

Residual exposure is unchanged in *kind*, only in *framing*: a **worker** subagent issuing `context_cycle` would clobber the SM-subagent's stamp, exactly as a worker clobbers the root SM's stamp today. It is the same last-writer-wins surface, now visible in the `Overridden` server log, with no mechanism fix recommended — it is a protocol-authorization concern (which threads are permitted to declare), not an attribution-mechanism defect. The platform's job is to attribute whatever was declared; policing *who* may declare is the protocol's job.

**3 — Nesting depth>1: no evidence today; canary covers the drift; AC reworded.** All ass-071 evidence (328/328 parent-id inheritance) is **one level deep** — subagent → root. If the SM is itself a subagent spawning grandchild workers, whether a **grandchild** hook event carries the *root* session_id or an *intermediate* (SM-subagent's) session_id is **unverifiable today**: the constraint that would create grandchildren is not lifted, so no such events exist in any corpus or live DB to measure. Stated plainly as unverifiable rather than assumed. Consequence for Q4: the AC as originally worded ("parent session's session_id") is **ambiguous at depth>1** — at depth>1 "parent" could mean the immediate subagent parent (wrong) rather than the root session (right). **Reworded** (applied above): "**root session's** session_id … at any nesting depth," with a note that root≡parent under today's one-level constraint. The `stamp_miss` canary already covers the failure mode: if a future grandchild event carries an intermediate session_id, no `cycles/{that_id}.json` exists → the event lands in `stamp_miss` (a climbing counter) rather than silent misattribution. So depth>1 drift is **detected, not silently absorbed**, even though it cannot be tested until the constraint lifts. The Q4 test gains one forward-compat fixture (depth>1 grandchild id → asserted into `stamp_miss`), also applied above.

**4 — Worktree interaction: priority unchanged, exposure rises.** If the subagent-SM itself runs under `isolation:worktree`, its declaration's PreToolUse fires from the **worktree cwd**. Under the scenario-8 TS-client trap (today, pre-F4-fix): `walkToProjectRoot` stops at the worktree dir → the `cycles/` file is written into the **per-worktree** state dir (`cycles/{key}.json` under the worktree hash, not the main-root hash), and in remote mode the missing-config layer drops the event entirely. Either way the **whole session goes unstamped** — and now it is *more* damaging, because the unstamped declaration is the SM's own session-defining `cycle_start`, not a stray worker event. This does **not** change the **priority** of the F4 gitdir-resolution precondition — it was already a hard precondition (precondition #1, "GO with named preconditions"). It does **sharpen its rationale**: in a subagent-SM world the worktree trap can silence the single most important declaration in the session, so the gitdir-resolution port is not merely a coverage fix but a correctness prerequisite for running the SM itself in a worktree. The F4 precondition fully covers this (porting Rust gitdir resolution into `walkToProjectRoot` lands the worktree-issued declaration in the main-root state dir, where every other thread in the session reads it). No new precondition; same precondition, elevated rationale.

**5 — Ordering: declaration moves one spawn later; scenario-11 race unchanged.** The declaration now happens one spawn deeper (root → spawn SM-subagent → SM declares → SM spawns workers) instead of (root SM declares → spawns workers). Scenario 11's analysis is unaffected: the race is "events emitted *before* the declaration instant are unstamped," and that boundary is defined by the declaration timestamp wherever it occurs in the spawn tree. The client write is at PreToolUse of `context_cycle` (before the tool executes) with atomic rename, so reads see old-or-new cleanly regardless of spawn depth. The only shift is that the **root session's own pre-SM-spawn events** (anything the parent thread emits before the SM-subagent declares) join the pre-declaration residue — identical in kind to today's "events before the SM declares," just attributable to the root thread instead of the SM thread. Same VOTE/NULL residue bucket, same bound (`load_cycle_observations` windows from the declaration). No change to the race analysis.

**6 — Verdict.** **No decision changes; framing changes.** Wire shape (`ImplantEvent.cycle_stamp`), precedence chain (stamp→marker→vote-on-NULL, presence-gated), voting verdict (demote-not-delete), the five preconditions, and the GO all stand unchanged. What changes: (a) the F4 contract note is reworded from role-first ("only the SM declares") to mechanism-first ("any thread may declare, keyed by root session_id, last-writer-wins; protocol authorizes which thread"); (b) the Q4 AC word "parent" → "root" with a depth>1 forward-compat fixture and the explicit note that the canary is the depth>1 tripwire; (c) scenario 15's "hijack" is recast as the intended path with the worker-clobber as the residual, same-kind exposure; (d) the worktree precondition's rationale is elevated (it can now silence the SM's own declaration) without changing its priority. The platform is already protocol-shape-agnostic because the stamp keys off the inherited session_id, not off a thread's role — which is exactly the generic-platform property the human's scenario was probing for.

**New unanswered question (depth>1):** Does a *grandchild* subagent hook event carry the **root** session_id or an **intermediate** subagent's session_id? Unverifiable today (subagent-spawns-subagent is not enabled in Claude Code, so no such events exist to measure). If the constraint lifts, this must be re-measured before relying on the stamp for grandchild-emitted events; until then the `stamp_miss` canary converts any drift into a detected counter rather than silent loss. This is a forward-compat watch item, not a current gap.
