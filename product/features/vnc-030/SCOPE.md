# vnc-030 — Contractual Cycle Attribution: Stamp Primary, Precedence Chain, Heuristic Demotion (F4b)

GH Issue: #699. Split from #680 (uni-zero decision, 2026-06-08) — the attribution half of the former F4 bundle. Transport half is vnc-027 (F4a, #680), MERGED 2026-06-08; vnc-030 is now next-up in the pinned sequence (vnc-030 → crt-052 remain). The stamp rides `build-request.js` on both transports.

## Problem Statement

Feature attribution is ~90% inference. Today's primary mechanism is heuristic: client-side topic extraction (`topic-signal.js` / `attribution.rs`), server registry fill (`enrich_topic_signal`), eager attribution, and majority-vote at close. Live-DB measurement (ass-072): 20.2% of observations carry an extracted signal that contradicts the session's declared feature (#588 inversion surface); two server sites let the vote **beat** the declared feature at close/sweep (`infra/session.rs:628`, listener close path). ass-072 (#694) returned **GO** on a stronger write-time design: a client-side cycle stamp that makes attribution contractual for every declared session.

Why now: F2 (vnc-025) shipped the transcript buffer; ass-072 findings are fresh, decision-complete, and on main (`00cc90c8`). The `topic_source` column this feature adds is the evidence base the F6 (#682) hook.rs retirement gate decides on — sequencing it early opens the soak window.

## Goals

1. **Contractual cycle attribution (stamp primary, per ass-072 GO)** — client-side cycle tracker `cycles/{session_key}.json` (create on `cycle_start` interception, update on `phase-end`, delete on `cycle_stop`, **never** on SessionStart/SessionClose/Stop); stamp carried as new additive `ImplantEvent.cycle_stamp: Option<CycleStampPayload{topic, phase}>`; client topic extraction suppressed while a stamp is active.
2. **Server precedence chain: stamp → marker → vote-on-NULL** — fix both declared-vs-vote inversions (close path + `sweep_stale_sessions`); `FeatureSource::{Declared,Inferred}` for the unstamped window (#588 residual remedy); additive `observations.topic_source` column so the F6 retirement gate decides on data.
3. **Heuristic demotion — demote, do not delete** — `enrich_topic_signal` NULL-fill kept as the write-time floor; eager attribution unchanged (already NULL-only); majority vote demoted to NULL-only permanently (60% of sessions / 25% of observations never declare — vote is their only attribution).
4. **Attribution robustness preconditions (ass-072)** — root-session-id contract AC + integration test; `stamp_miss` canary in `health.json`; verify (not re-implement) the F3 worktree gitdir port covers the stamp path.
5. **Carry-item (#680 comment, 2026-06-08)** — protocol restart re-declaration line in all three protocols: on re-entering a broken session, the leader's first action is to re-issue `context_cycle(type:"start", topic:"{feature-id}")`.

## Non-Goals

- **Building the TS UDS transport, parity corpus, hook-set reduction, size-gate/offset-delete carry-items** — vnc-027 (F4a, #680, MERGED 2026-06-08). vnc-030 does not BUILD UDS; it only proves the stamp rides the merged UDS transport (AC-10).
- **Deleting any heuristic** — extraction, NULL-fill, eager, and vote all survive as the degraded floor for never-declare sessions (uni-zero, research spikes, ad-hoc: 42/70 retained sessions). Full vote retirement never clears the evidence bar, including at F6.
- **Marker-recovery implementation** — the precedence chain's MARKER tier is review-time. Inventory complete (OQ1, resolved 2026-06-08): it does **not** exist — deferred with a named follow-up issue. The follow-up must consume crt-052's transcript snapshot seam, not add a second buffer reader (see OQ1 resolution + Parallel-Design Coordination).
- **Server registry lifecycle redesign** — per-turn Stop→SessionClose registry drain, `register_session` overwrite on resume/compact, and related mid-session-amnesia bugs (ass-072 out-of-scope discoveries 2/3) are named, not fixed here, except where the precedence chain requires it.
- **Rust `hook.rs` changes** — zero. Mixed stamped/unstamped clients coexist by construction (per-event self-describing stamp, tolerant wire both ways).
- **#578 audit-log retention** — deferred post-OSS-cloud-v1 (vnc-029).
- **MCP-only hookless clients** (ass-072 scenario 16) — unattributable by any mechanism; named, not solved.
- **Giving uni-zero/research sessions declarations** — protocol-side decision, separate change if wanted.
- **Depth>1 subagent inheritance verification** — unverifiable until Claude Code lifts the constraint; the `stamp_miss` canary is the tripwire.

## Background Research

All claims verified in this workspace 2026-06-08 (branch `main`, post-F3-merge). ass-072 FINDINGS: `product/research/ass-072/FINDINGS.md` (on main, `00cc90c8`).

### Attribution (ass-072 FINDINGS — primary design input)
- **GO: stamp as primary.** Scenario × mechanism matrix (17 scenarios): stamp covers every declared session including the three server-state-loss events (per-turn Stop drain, compaction re-register, server restart); `--resume` reuses the session_id (empirical, claude 2.1.167; 86/86 corpus) so the stamp survives crashes with zero gap; fork-resume is MARKER-covered (forked transcript carries full history).
- **Wire shape**: new additive `ImplantEvent.cycle_stamp` (`#[serde(default, skip_serializing_if)]`, no `deny_unknown_fields` anywhere) — 7th ts-rs export, frozen-F1-safe, old fixtures untouched. Not `topic_signal` reuse (server couldn't tell contract from guess — the #588 failure class) and not a payload key.
- **Stamp is declaration-source-agnostic** (amendment, scenario 17): keyed by inherited root session_id; any thread may declare, last-writer-wins, protocol authorizes who. AC wording is "root session's session_id at any nesting depth".
- **Two inversion bugs to fix regardless of the stamp**: `sweep_stale_sessions` (`infra/session.rs:628`) and the close path both resolve `vote.or_else(declared)` — declared must win when the feature source is Declared.
- **Note**: the original #680 issue body (older) said markers are primary; ass-072 (newer) makes the **stamp** primary with marker as review-time recovery and vote as NULL-only floor. This SCOPE follows ass-072.

### TS client (F3, what F4b extends)
- `state.js` provides the atomic-write/sanitize/prune machinery the cycle tracker reuses (~30-line addition per ass-072).
- `config.js::walkToProjectRoot` + `resolveGitFile` **already contain the worktree gitdir-resolution port** (landed in F3, commit b2e215fd, PR #696, empirically verified at parity) — ass-072 F4-precondition #1 is satisfied; F4b only needs a stamp-path regression test over it.
- The stamp attaches in `build-request.js` — transport-agnostic; works over HTTP and, since F4a merged (2026-06-08), over the merged UDS transport. AC-10 proves the stamp rides UDS byte-equivalently (vnc-027's post-merge obligation owed to #699).
- **Size**: RESOLVED — the external dependency is closed. vnc-027's C-04 size-gate redefinition (ADR-005) merged 2026-06-08: 100,000 B comment-stripped primary / 160,000 B raw backstop. The merged tree measures ~68,907 B stripped / ~112,773 B raw. vnc-030's measured client additions (cycles tracker, `cycle_stamp` attach, extraction suppression, `stamp_miss` canary) are ~3,900 B raw / ~2,050 B stripped — they fit with headroom. Documented fallback if tight: fold `cycles.js` into `state.js`.

### Server attribution surfaces (OQ1/OQ2 inventory, verified 2026-06-08)
- **Review-time attribution is window-based, not marker-based**: `load_cycle_observations` (`crates/unimatrix-server/src/services/observation.rs:308`) builds time windows from server-recorded `cycle_events` DB rows (written at hook interception), discovers sessions via `topic_signal` match per window, then window-filters observations. It never reads transcript content.
- **No code reads the F2 transcript buffer for attribution.** The buffer's only content-bearing output is `TranscriptBuffer::contiguous_tail` (`infra/session_transcript.rs:179`; module doc at `:10` pins this), and its sole production consumer is the PreCompact transcript-block path (`uds/listener.rs:1646` → `extract_transcript_block_from_bytes`). No cycle_start-marker / protocol-header parsing exists anywhere server-side.
- **The extractor is more permissive than its docs claim** (drives the OQ2 resolution): `is_valid_feature_id` (`crates/unimatrix-observe/src/attribution.rs:15-23`) requires a hyphen and `[A-Za-z0-9-_.]` chars but **no digits**, despite the `{alpha}-{digits}` docstrings in `attribution.rs` and `topic-signal.js`. Any hyphenated ASCII token validates (`uni-zero`, `SHA-256`, …).
- Unimatrix constraints consulted: #4092 (idempotent ALTER TABLE guard via `pragma_table_info` — run all checks before any ALTER; governs the `topic_source` migration), #4726 (ts-rs codegen with drift-checked bindings — governs the 7th `CycleStampPayload` export), #4140 (`set_feature_force` silently no-ops for absent sessions — evicted-session total attribution loss; the disk-persisted stamp removes this registry-presence dependence), #1067 (eager attribution immutable-by-design), #3382 (registry NULL-fill pattern, retained as floor).

### Parallel-Design Coordination (vnc-027 MERGED 2026-06-08; crt-052 designing)
- **Delivery order pinned (human decision, 2026-06-08): vnc-027 → vnc-030 → crt-052.** vnc-027 (F4a) MERGED 2026-06-08; vnc-030 is now next-up (vnc-030 → crt-052 remain). vnc-027 owned the C-04 size-gate redefinition (its AC-09, the *first* change in its Proposed Approach), which has merged; the land-first contingency is closed — vnc-030 delivers next and rebases `build-request.js` onto the merged tree. Note: vnc-027's OQ2 lean (server-side preformatted UDS sync responses, avoiding a `format_injection` JS port) is also the size-budget outcome vnc-030's client additions need.
- **vnc-027 OQ5 worktree-cwd dump has NOT been run** (still open in its SCOPE; its only agent artifact is the pre-split researcher report). Both features consume the result (our AC-08). One design session must run it and share the result — currently unassigned.
- **Shared client surface**: both features edit `build-request.js`. vnc-027 AC-08 retires standalone PreToolUse *observation* but preserves `context_cycle` interception — which is exactly the seam vnc-030's tracker create/update/delete hangs on. Compatible by design; vnc-030 rebases onto the merged tree, and vnc-030's architecture should assert the interception seam survives the reduction.
- **crt-052 consumes vnc-030's attribution quality**: its transcript-snapshot session selection is `state.feature == feature_cycle` (registry) plus attributed observations; the stamp + the two declared-vs-vote inversion fixes are what make that selection reliable (its Delivery Ordering rationale 2 — written pre-split, citing "vnc-027"). crt-052 also warns that any review-time marker recovery must consume its snapshot seam rather than adding a second buffer reader — folded into our OQ1 deferral.
- **Code adjacency with crt-052**: vnc-030 fixes `sweep_stale_sessions` (`infra/session.rs:628`) and the listener close path; crt-052's continuity remedy (its OQ-1 Option B) modifies `drain_and_signal_session` / `clear_transcripts_for_feature` in the same files. Sequential delivery advised; crt-052's constraint 12 names vnc-027 but post-split the adjacent feature is vnc-030.
- **crt-052 OQ-1 resolved to Option B** (human decision, 2026-06-08, recorded on #689): server-only transcript hold, no close-reason wire field — the Option A routing contingency is moot; nothing rides either F4 wire surface for crt-052.
- **`topic_source` and the learning layer**: crt-052 does not consume `topic_source` today (its candidates are response-transient). Informational only — no coordination needed beyond F6 (#682) as the named consumer.

## Proposed Approach

1. **Client**: implement the ass-072 client-state-lifecycle spec verbatim (tracker beside `offsets/`, `state.js` atomics, no-clear-on-SessionClose); add `cycle_stamp` to `ImplantEvent` + ts-rs binding; suppress extraction when stamped.
2. **Server**: read `cycle_stamp` at the record paths, `FeatureSource` flag, flip the two inversion sites, `topic_source` column (additive migration after v10's `topic_signal` precedent). The stamp applies on **both** transports (HTTP frames carry it identically).
3. **Protocols**: one re-declaration line in design/delivery/bugfix protocols.
3b. **#574 interaction check (human decision, 2026-06-08)**: design confirms the #574 relocation (cycle_events writes move server-side to the MCP handler) cannot race `load_cycle_observations` windowing or the client-side `context_cycle` interception seam the tracker hangs on — expected no-race (server-side write vs client-side interception), one line in the architecture records it.
4. **Pre-baseline**: uni-zero attribution provenance (ass-072 UQ-4) resolved at design time (see Open Questions) — AC-07 baseline restricted to declared protocol sessions accordingly.

## Acceptance Criteria

- AC-01: Cycle tracker `cycles/{session_key}.json` — created on intercepted `context_cycle(start)` (post-validation), phase updated on `phase-end`, deleted on `stop`, never touched by SessionStart/SessionClose/Stop; survives crash + `--resume`; 7-day age prune; atomic writes.
- AC-02: `ImplantEvent.cycle_stamp: Option<CycleStampPayload{topic, phase}>` — additive serde field + new ts-rs binding; all pre-existing wire fixtures pass unmodified; old-server/new-client and new-server/old-client combinations tolerated.
- AC-03: Client topic extraction emits no `topic_signal` while a stamp file exists for the session; extraction unchanged when none does.
- AC-04: Server precedence is presence-gated stamp → marker → vote-on-NULL: stamped events attribute from the stamp; `enrich_topic_signal` NULL-fill retained; declared feature wins over majority vote at session close AND `sweep_stale_sessions` (`FeatureSource::{Declared,Inferred}`); vote consulted only when no declared feature exists.
- AC-05: Additive `observations.topic_source` column (migration) recording the attribution source (`declared`/`extracted`/`registry-fill`/`vote`/NULL) per row.
- AC-06: Root-session-id contract: integration test asserting (a) a subagent-context event with stdin session_id = S joins session S and is stamped from `cycles/{S}.json`; (b) the `stamp_miss` canary in `health.json` (rescoped per ADR-006 rev, human decision 2026-06-08) no longer counts unattributed sessions — an unknown-session-id event still produces an unstamped row, but the canary increments only on SUBAGENT context: a depth≥1 subagent event whose inherited root tracker is missing counts as inheritance drift (the real SR-01 signal), while a depth-0 never-declare session with no tracker is NOT counted (structural noise, the normal case in this repo). The 0.20 threshold and concurrent-file rule are removed; the canary is a zero-tolerance test-time invariant. Delivery crux: confirm "I am a subagent" survives the same CLI drift that breaks inheritance — if not, narrow the canary to test-time-only.
- AC-07: Attribution accuracy on a protocol-session sample ≥ the current heuristic baseline; heuristic fallback still attributes marker-less ad-hoc sessions (regression sample includes at least one never-declare session). Baseline methodology per the OQ2 resolution: accuracy denominator = declared protocol sessions only (declaration = ground truth); never-declare sessions (uni-zero, research, ad-hoc) appear only in the fallback regression sample — their vote matches are token-mention recall, not accuracy evidence.
- AC-08: Stamping works from a worktree checkout: a `cycle_start` intercepted in a git worktree writes the tracker under the main-root hash, and subsequent worktree events are stamped (regression test over the F3 gitdir port). Cross-reference: vnc-027 OQ5 (worktree cwd dump) answers whether hook stdin `cwd` carries the worktree path — both features consume that result; whichever design session runs the dump shares it.
- AC-09: All three protocols (design/delivery/bugfix) contain the restart re-declaration line: on re-entering a broken session, the leader's first action is to re-issue `context_cycle(type:"start", topic:"{feature-id}")`.
- AC-10: UDS-path stamp regression — the stamp decoration is proven byte-equivalent over the merged vnc-027 UDS transport, not just HTTP (vnc-027's explicit post-merge obligation owed to #699). The architecture pins the test seam at `transport-uds.encodeFrame`.

## Constraints

- **Frozen F1 wire contract — additive only.** No field renames/removals; `skip_serializing_if` on new optionals; no `deny_unknown_fields`; existing parity fixtures and ts-rs bindings must pass byte-unchanged.
- **Rust hook untouched.** Mixed stamped/unstamped clients against one server, no feature flag.
- **Cycle tracker must NOT copy the offsets delete-on-close lifecycle** — Stop fires per assistant turn; delete-on-close kills the stamp after turn 1 (ass-072 precondition 5).
- **Fail-open client contract** (F3 C-05): never throws, exit 0 always, no stdout on failure paths, no secrets in stderr/breadcrumbs, every fs/network call wrapped.
- **Size budget**: RESOLVED — client additions fit under the redefined C-04 gate (100,000 B stripped primary / 160,000 B raw backstop, merged in vnc-027 ADR-005). vnc-030's ~3,900 B raw / ~2,050 B stripped additions clear the ~68,907 B stripped / ~112,773 B raw merged tree with headroom; fallback if tight is folding `cycles.js` into `state.js`.
- **Sync-path budget**: the sync trio gains no extra file I/O; cycle-tracker reads are one small JSON read on FNF builds.
- **Migration discipline**: `topic_source` follows the v9→v10 `topic_signal` pattern (pragma check + `ALTER TABLE`, idempotent).
- **No new npm dependencies** (F3 pure-TS architecture decision stands).

## Open Questions

Resolved at design time (2026-06-08, code-level — see Background → Server attribution surfaces):

1. **Marker recovery inventory** (former F4 OQ4) — **RESOLVED: does not exist; defer confirmed.** `context_cycle_review` attribution is `load_cycle_observations` (`observation.rs:308`), which windows over `cycle_events` DB rows — never transcript content. The F2 buffer's only content-bearing reader is the PreCompact block path (`listener.rs:1646` via `contiguous_tail`). Per the uni-zero-review default: F4b ships stamp + precedence chain + vote demotion only; marker recovery gets a named follow-up issue, which must pin a dependency on crt-052's transcript snapshot seam (crt-052 SCOPE interaction warning) instead of adding a second buffer reader.
2. **uni-zero attribution provenance** (ass-072 UQ-4) — **RESOLVED: ordinary extraction → eager/vote; no hidden mechanism.** ass-072's premise ("the alpha-digits extractor cannot produce `uni-zero`") was wrong: the `{alpha}-{digits}` docstring overstates the filter — `is_valid_feature_id` (`attribution.rs:15-23`) has **no digit requirement**, so any hyphenated token validates. `extract_feature_id_pattern` (`attribution.rs:45-55`) splits on whitespace/quotes/parens and trims non-alphanumeric edges: the prompt token `/uni-zero` trims to `uni-zero`; the Skill tool_input JSON (`{"skill":"uni-zero"}`) splits on `"` and yields it directly. Same permissiveness class as the live-DB `SHA-256` misfire. **AC-07 implication**: uni-zero "vote successes" are self-fulfilling (any session *mentioning* uni-zero matches) and measure token recall, not attribution accuracy — so the accuracy baseline is measured on protocol sessions with declared features (declaration = ground truth); never-declare sessions appear only in the fallback regression sample, never in the accuracy denominator. Drive-by for delivery: correct the misleading `{alpha}-{digits}` docstrings in `attribution.rs` and `topic-signal.js`.

3. **Worktree cwd dump** (shared with vnc-027 OQ5, consumed by AC-08) — **RESOLVED (empirical probe, 2026-06-08): hook cwd carries the WORKTREE path.** Live `/proc` capture: hook processes for a worktree-isolated subagent spawn with `cwd={worktree path}` and `CLAUDE_PROJECT_DIR={main checkout}` (control: main-checkout session hooks spawn with main path); `resolve_cwd` (`hook.rs:352-365`) yields the worktree under every branch (no `--project-dir`, stdin cwd / process cwd both worktree). Events landed under the main-root hash with no worktree-hash dir created — the F3 gitdir port already resolves worktree cwd → main root in production, so AC-08 asserts existing behavior. Negative finding: no persisted raw-cwd discriminator exists (tool-event payloads exclude the `cwd` serde field; `ImplantEvent` has no cwd; only `SessionRegister` carries it and SessionStart never fires for subagents). Design implication: stamp paths must route through `walkToProjectRoot`/`detect_project_root`, never hash raw cwd. Report: `agents/vnc-030-cwd-probe-report.md`. Result cross-posted to #680.

No open questions remain.

## Tracking

GH Issue: #699 (will be updated with session links).

**#588 disposition (human decision, 2026-06-08)**: AC-04's inversion fixes resolve #588's surface — close #588 via vnc-030's PR, or convert any residue to a named follow-up.
