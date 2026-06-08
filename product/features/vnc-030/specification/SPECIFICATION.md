# SPECIFICATION — vnc-030: Contractual Cycle Attribution (F4b)

GH Issue: #699 · Scope: `product/features/vnc-030/SCOPE.md` (approved 2026-06-08) · Primary design input: `product/research/ass-072/FINDINGS.md` (GO) · Probe: `agents/vnc-030-cwd-probe-report.md`

## Objective

Make feature attribution contractual for every declared session: a client-side cycle stamp (disk tracker + additive wire field) becomes the primary attribution mechanism, the server enforces a presence-gated precedence chain (stamp → marker-when-present → vote-on-NULL) fixing both declared-vs-vote inversions, and the heuristic pipeline is demoted — never deleted — to remain the floor for never-declare sessions. An additive `topic_source` column gives the F6 (#682) retirement gate its evidence base.

## Functional Requirements

### Client — cycle tracker (AC-01)

- **FR-01** — On PreToolUse interception of `mcp__unimatrix__context_cycle(type:"start")`, after `validate_cycle_params`-equivalent validation passes, the client creates/overwrites `cycles/{session_key}.json` in the F3 state dir (beside `offsets/`) with `{topic, phase, declared_at, updated}` via the existing `state.js` atomic write (temp+rename). *Test: unit — intercepted start event produces the file atomically; invalid params produce no file.*
- **FR-02** — On interception of `type:"phase-end"`, the client updates `phase` in the same file (atomic), bumping `updated`. *Test: unit.*
- **FR-03** — On interception of `type:"stop"`, the client deletes the file. *Test: unit.*
- **FR-04** — SessionStart (any source: startup/resume/clear/compact), SessionClose, and Stop never touch the tracker file. The tracker MUST NOT copy the offsets delete-on-close lifecycle (Stop fires per assistant turn; delete-on-close would kill the stamp after turn 1). *Test: unit — fire each lifecycle event, assert file unchanged.*
- **FR-05** — Tracker files are pruned by age: 7 days since `updated` (same policy as `offsets/`); filenames use the existing `sanitizeSessionKey`. *Test: unit — prune sweep removes only stale files.*
- **FR-06** — Every event built by `build-request.js` reads the tracker for its session_key; if present, attaches `cycle_stamp: {topic, phase}`; if missing or corrupt, attaches nothing and never throws (fail-open). *Test: unit — present/missing/corrupt-JSON cases.*
- **FR-07** — Declaration is source-agnostic: the tracker is keyed by the inherited **root** session_id, so any thread (main or subagent) issuing `context_cycle` stamps the whole session; last writer wins (atomic overwrite). The mechanism does not police which thread declares — the protocol does. *Test: covered by AC-06 integration test (subagent-context event with root session_id).*

### Client — extraction suppression (AC-03)

- **FR-08** — While `cycles/{session_key}.json` exists for the session, the client emits **no** `topic_signal` (extraction short-circuits). When no tracker file exists, extraction behaves exactly as today. *Test: unit — same prompt content produces `topic_signal` without a tracker file and omits it with one.*

### Client — stamp_miss canary (AC-06)

- **FR-09** — The client increments a content-free `stamp_miss` counter in `health.json` (`state.bumpStampMiss`, read-modify-write, never-throw, content-free — count only) on the FNF decoration miss branch **iff the event is in subagent context** (the client sees SubagentStart / the hook event is at depth ≥ 1) **AND** no `cycles/{session_key}.json` is found, where `session_key` is the expected-inherited **root** session_id the subagent event carries. This is unambiguous inheritance drift: a declared root established `cycles/{root_session_id}.json`, and a depth ≥ 1 event that should inherit that id but finds no tracker means inheritance broke (the real SR-01 signal). A depth-0 top-level session with no tracker is a never-declare session (uni-zero, research, ad-hoc — the normal mode in this repo) and is **NOT** counted (structural noise). SessionStart/SessionRegister/SessionClose frames never reach decoration; the vnc-027 null sentinel short-circuits non-cycle PreToolUse before decoration. The `anyOtherCycleFile` / concurrent-file condition and the `fnf_record_send_count` denominator are **removed entirely**. *Test: unit — depth-0 never-declare (no increment); depth≥1 subagent with inherited root tracker present (no increment); depth≥1 subagent carrying a non-inherited id while the root tracker exists (exactly one increment); depth>1 forward-compat — a grandchild id with no tracker lands in `stamp_miss`, not silent loss (ADR-006 §5 fixtures).*
- **FR-10** — Canary invariant (SR-01): the canary is a **zero-tolerance test-time invariant that carries to production**, not a thresholded rate signal. `stamp_miss == 0` is the contract at test time AND in production; any nonzero growth is real inheritance drift to investigate before trusting new attribution data. **No ratio, no `fnf_record_send_count` denominator, no 0.20 threshold, no per-deployment baseline measurement, no human re-set ritual** (all removed per ADR-006 rev2) — and the canary is decoupled from the AC-07 baseline-measurement step. Test assumptions pin the verified CLI version (claude 2.1.167) for `--resume` session_id reuse and depth-1 root-id inheritance; any CLI bump after vnc-030 ships re-runs the AC-06 canary fixtures as the drift check (part of the standard suite). The F6 gate review MUST consult the counter before relying on stamp coverage data. *Verification: zero-tolerance invariant stated in test-module doc comments and the implementation brief; canary unit tests reference the pinned version and assert `stamp_miss == 0` on the healthy declared-subagent fixture.*

### Wire (AC-02)

- **FR-11** — `ImplantEvent` gains `cycle_stamp: Option<CycleStampPayload>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; `CycleStampPayload { topic: String, phase: Option<String> }` is the 7th ts-rs export with drift-checked bindings (constraint #4726). No `deny_unknown_fields` is introduced anywhere. *Test: fixture — all pre-existing wire fixtures pass byte-unmodified; new round-trip fixture for the field.*
- **FR-12** — Mixed-version tolerance: old-server/new-client (field ignored) and new-server/old-client (`None` → legacy chain) both work, no feature flag. *Test: integration — replay a stamped frame against deserialization without the field defined (simulated old server tolerance via serde contract test) and an unstamped Rust-hook-shaped frame against the new server path.*
- **FR-13** — End-to-end round-trip (SR-03, regression class #3486 — field extracted but never inserted): a stamped client event produces an observation row whose attribution came from the stamp. The test asserts **both ends**: client attaches the field into the serialized frame, and the server reads it into the row. *Test: integration, parity layer.*

### Transport — UDS-path stamp regression (AC-10)

- **FR-29** — The stamp decoration is transport-agnostic by construction (decoration mutates the in-memory `request` upstream of `selectTransport`, ADR-002 §7) and MUST be proven byte-equivalent over the **merged vnc-027 UDS transport**, not only HTTP. A stamped FNF `RecordEvent` (tracker file present) driven through `index.js` `runFireAndForget` with a UDS-mode config (`config.mode = "uds"`) MUST carry `cycle_stamp` in the JSON payload decoded from the bytes produced by `transport-uds.encodeFrame` (`transport-uds.js:55-62`), byte-equivalent to the `transport-http.post` body for the same input. This discharges vnc-027's explicit post-merge obligation owed to #699. *Test: regression — drive one stamped `RecordEvent` through both transports for identical input; assert `transport-uds.encodeFrame` output decodes to a payload containing `cycle_stamp` and that it is byte-equivalent to the HTTP body.*

### Server — precedence chain (AC-04)

- **FR-14** — At every observation record path, a present `cycle_stamp` attributes the row: `topic_signal := stamp.topic`, `phase := stamp.phase`, `topic_source := 'declared'`; extraction enrichment, registry fill, and vote do not apply to that row; the session registry feature is set with `FeatureSource::Declared`. *Test: integration — stamped event lands with declared attribution regardless of registry state (including post-restart empty registry).*
- **FR-15** — `FeatureSource::{Declared, Inferred}` is added to registry session state. For unstamped clients in the mixed window, a `Declared` registry feature wins over extracted `topic_signal` at the enrichment sites (the #588 residual remedy); `Inferred` features retain today's behavior. *Test: unit on the enrichment decision; integration with an unstamped declared session.*
- **FR-16** — Inversion fix 1: `sweep_stale_sessions` (`crates/unimatrix-server/src/infra/session.rs:628`) resolves `declared_feature.or_else(vote)` when the feature source is `Declared` (today: `majority_vote_internal(...).or_else(state.feature)`). *Test: unit — declared session with a contradicting majority vote sweeps to the declared feature.*
- **FR-17** — Inversion fix 2: the listener session-close path applies the same rule — declared feature wins; vote is consulted only when no declared feature exists. *Test: unit, mirror of FR-16 on the close path.*
- **FR-18** — Both inversion fixes are minimal-diff (flip resolution order + FeatureSource guard, nothing else — SR-10); the post-fix close/sweep semantics are documented as a citable interface for crt-052. Registry touchpoints beyond the FeatureSource flag and the two flips are out of scope (SR-05 fence); anything else discovered becomes a named follow-up beside ass-072 discoveries 2/3. *Verification: architecture doc section + diff review at gate.*
- **FR-19** — Heuristic demotion: `enrich_topic_signal` NULL-fill is retained unchanged as the write-time floor; eager attribution (`check_eager_attribution`/`set_feature_if_absent`) is untouched (already NULL-only); majority vote is permanently demoted to NULL-only (consulted only when no declared feature exists) at both close and sweep. No heuristic is deleted. *Test: integration — never-declare session still attributes via extraction → fill/vote exactly as today.*

### Server — topic_source column (AC-05)

- **FR-20** — Additive `observations.topic_source` column via an idempotent migration following the v9→v10 `topic_signal` precedent: run **all** `pragma_table_info` checks before any `ALTER TABLE` (constraint #4092). Existing rows remain NULL. *Test: migration unit — fresh DB, already-migrated DB (idempotent re-run), pre-migration DB.*
- **FR-21** — Value vocabulary with exactly one source per write site (SR-04 — this taxonomy is the F6 gate's evidence base; no best-guess values):

  | Value | Write site | Precedence tier |
  |---|---|---|
  | `declared` | Stamp read path at record time (FR-14) | STAMP |
  | `extracted` | Row written with a client-extracted `topic_signal` (unstamped event arrived with the signal set) | heuristic floor |
  | `registry-fill` | `enrich_topic_signal` NULL-fill from registry state | heuristic floor |
  | `vote` | Row attribution resolved by majority vote (vote-on-NULL paths) | VOTE |
  | NULL | No mechanism attributed the row | UNATTRIBUTED |

  *Test: one integration case per value asserting the column matches the code path that wrote it.* (See Open Question OQ-A on the `vote` row-level write site.)

### Robustness preconditions (AC-06, AC-08)

- **FR-22** — Root-session-id contract: every hook event emitted from any subagent context, **at any nesting depth**, carries the **root session's** session_id (not an intermediate parent's) and its observation row joins the root session. Today's one-level constraint makes root ≡ parent; the wording is future-proof for depth>1, where the `stamp_miss` canary is the tripwire (depth>1 is explicitly unverifiable today). *Test: integration (parity layer) — (a) subagent-context event with stdin session_id = S joins session S and is stamped from `cycles/{S}.json`; (b) an unknown-session-id event at depth-0 produces an unstamped row and does **NOT** increment `stamp_miss` (structural noise, per FR-09); the increment is asserted only on the subagent-context inheritance-break fixture (FR-09).*
- **FR-23** — Worktree stamping (asserts existing F3 behavior, per the cwd probe): a `cycle_start` intercepted in a git worktree (hook cwd = worktree path, empirically confirmed) writes the tracker under the **main-root** hash via `walkToProjectRoot`/`resolveGitFile`, and subsequent worktree events are stamped. Stamp paths MUST route through the project-root walk and MUST NEVER hash raw cwd (probe negative finding: no persisted raw-cwd discriminator exists to debug a violation). *Test: regression over the F3 gitdir port — worktree fixture with a `.git` file pointing at the main gitdir; assert tracker path and stamped event.*

### Protocols and drive-bys (AC-09)

- **FR-24** — All three protocols (`.claude/protocols/uni/uni-{design,delivery,bugfix}-protocol.md`) gain the restart re-declaration line: on re-entering a broken session, the leader's first action is to re-issue `context_cycle(type:"start", topic:"{feature-id}")` (idempotent server-side, recreates the client tracker). *Verification: doc inspection — line present in all three.*
- **FR-25** — Drive-by docstring fix: the misleading `{alpha}-{digits}` claims in `crates/unimatrix-observe/src/attribution.rs` and `packages/unimatrix/lib/hook-client/topic-signal.js` are corrected to describe the actual filter (hyphen required, `[A-Za-z0-9-_.]`, no digit requirement). Comment-only — no behavior change to the extractor. *Verification: diff review.*

### Issue dispositions (verification items, not new behavior)

- **FR-26** — **#588 disposition** (SR-08 — make the close decision mechanical). AC-04/FR-14..18 resolve: (a) write-time extracted-vs-declared inversion for stamped sessions — structurally moot, the client emits no `topic_signal` while stamped; (b) the declared-vs-vote inversions at close and sweep — fixed for all clients, stamped or not; (c) the unstamped-window write-time inversion — remedied by `FeatureSource::Declared` precedence. **Residue, not resolved**: row-level extracted noise already persisted in historical rows; per-row extracted signals from unstamped (Rust-hook) sessions still recorded into tallies; MCP-only hookless clients (scenario 16) remain unattributable. Close #588 via this feature's PR if the residue list is accepted as named non-goals; otherwise convert the residue to a named follow-up. *Verification: PR description carries this resolved/residue list.*
- **FR-27** — **#574 no-race confirmation**: the architecture records, in one citable line, that relocating `cycle_events` writes server-side to the MCP handler cannot race `load_cycle_observations` windowing or the client-side `context_cycle` interception seam (server-side write vs client-side interception), **and** the assumption's expiry condition: re-check if #574 merges before vnc-030 delivers (SR-12). *Verification: architecture doc inspection.*
- **FR-28** — **Interception-seam survival** (SR-09, ADR-007 §1): vnc-027 (F4a, MERGED 2026-06-08) retired standalone PreToolUse observation but preserves `context_cycle` interception — the seam FR-01..03 hang on. Asserted against the **merged** tree: the narrowed install matcher `lib/merge-settings.js:49` (`PRETOOLUSE_CYCLE_MATCHER = "context_cycle|mcp__unimatrix__context_cycle"`) spawns the hook for both tool names; `buildCycleEventOrFallthrough` (`build-request-tools.js:314`) yields a CYCLE_* `RecordEvent` (not the null sentinel) when `validateCycleParams` passes. vnc-030 adds zero logic to `build-request*.js` (rebase surface is `index.js` orchestration only). The post-rebase seam-survival assertion **gates before any vnc-030 server-work validation**: (1) a `context_cycle(start)` PreToolUse creates the tracker and sends a CYCLE_START `RecordEvent` with `cycle_stamp` attached (`request !== null`, reaches `cycles.writeCycle`); (2) a non-`context_cycle` PreToolUse returns the null sentinel (`build-request-tools.js:326`), `index.js:366` returns at exit 0 with no tracker touch, no canary bump, no network. *Test: two post-rebase client fixtures through the `index.js` pipeline, named in the test plan, run before server work.*

## Non-Functional Requirements

- **NFR-01 — Client size budget** (SR-02). **External dependency RESOLVED** — vnc-027's C-04 size-gate redefinition (ADR-005) merged 2026-06-08: comment-stripped 100,000 B primary, raw 160,000 B backstop. The merged tree measures ~68,907 B stripped / ~112,773 B raw. vnc-030's measured client additions (tracker, stamp attach, suppression, canary) are ~3,900 B raw / ~2,050 B stripped — they clear the gate with headroom. Named fallback if a later edit makes it tight: fold `cycles.js` into `state.js`. *Verification: estimate table in ARCHITECTURE.md; C-04 gate passes at delivery.*
- **NFR-02 — Sync-path budget.** The sync trio (UserPromptSubmit, PreCompact, SubagentStart) gains no extra file I/O; the cycle-tracker read is a single small JSON read shared with event build, tolerating FNF with no retry. *Verification: code review + the F3 perf assertions stay green.*
- **NFR-03 — Fail-open client contract** (F3 C-05). All new client code: never throws, exit 0 always, no stdout on failure paths, no secrets in stderr/breadcrumbs, every fs call wrapped. *Verification: failure-injection unit tests per new fs touchpoint.*
- **NFR-04 — Wire additivity.** Frozen F1 contract: additive only, `skip_serializing_if` on the new optional, no field renames/removals, no `deny_unknown_fields`; existing parity fixtures and ts-rs bindings pass byte-unchanged. *Verification: fixture suite unmodified and green.*
- **NFR-05 — Zero Rust hook changes.** `hook.rs` is untouched; mixed stamped/unstamped clients coexist against one server with no feature flag (per-event self-describing stamp). *Verification: diff review.*
- **NFR-06 — Migration discipline.** `topic_source` migration is idempotent and re-runnable (pragma-guarded, all checks before any ALTER). *Verification: FR-20 tests.*
- **NFR-07 — No new npm dependencies** (F3 pure-TS decision stands). *Verification: package.json diff empty of deps.*
- **NFR-08 — Pinned external behavior** (SR-01). Empirical Claude Code behaviors the design relies on — `--resume` session_id reuse, depth-1 root-session-id inheritance, hook cwd = worktree path — are pinned to claude 2.1.167 in test assumptions; drift detection is the `stamp_miss` canary (FR-10), not silent degradation.

## Acceptance Criteria (verification map)

| AC | Summary | Covered by | Verification method |
|---|---|---|---|
| AC-01 | Tracker lifecycle: create/update/delete on cycle events only; survives crash + `--resume`; 7-day prune; atomic | FR-01..05 | Unit tests per lifecycle event + crash-simulation test (file persists, same session_key post-resume per pinned CLI behavior) |
| AC-02 | Additive `cycle_stamp` field + 7th ts-rs binding; fixtures unmodified; old/new combos tolerated | FR-11, FR-12 | Fixture suite byte-unchanged; serde tolerance tests both directions; ts-rs drift check |
| AC-03 | Extraction suppressed while stamped; unchanged otherwise | FR-08 | Unit: same input with/without tracker file |
| AC-04 | Precedence stamp → (marker when present) → vote-on-NULL; NULL-fill retained; declared beats vote at close AND sweep; FeatureSource | FR-14..19 | Integration (stamped attribution), unit (both inversion sites), integration (never-declare floor regression). Note: the MARKER tier is review-time and **does not exist yet** (deferred, SR-07) — the wording "marker when present" is normative; the named follow-up issue (with crt-052 snapshot-seam dependency) must exist before design gate exit |
| AC-05 | `topic_source` column, values declared/extracted/registry-fill/vote/NULL, one source per write site | FR-20, FR-21 | Migration idempotence tests + one integration case per value |
| AC-06 | Root-session-id contract + subagent-gated `stamp_miss` canary (zero-tolerance) | FR-09, FR-10, FR-22 | (a) Integration: subagent event with stdin session_id=S joins S, stamped from `cycles/{S}.json` (unchanged). (b) Canary unit fixtures (ADR-006 §5): single-declared-session-with-subagent asserts `stamp_miss == 0`; simulated inheritance-break (subagent carrying a non-inherited id while root tracker exists) asserts exactly one increment; depth>1 grandchild id lands in `stamp_miss`, not silent loss. A depth-0 never-declare session with no tracker is NOT counted. No 0.20 threshold, no concurrent-file rule, no baseline measurement |
| AC-07 | Accuracy ≥ heuristic baseline on declared protocol sessions; fallback regression for never-declare | FR-19 + measurement | **Pinned methodology**: accuracy denominator = declared protocol sessions only (declaration = ground truth); never-declare sessions appear ONLY in the fallback regression sample — their vote matches are token-mention recall, not accuracy evidence. Strengthened per SR-06: the fallback sample includes multiple never-declare shapes (uni-zero, research-spike, ad-hoc — at least one each), and the check includes a before/after `topic_source` distribution comparison on the live DB, not accuracy alone |
| AC-08 | Worktree stamping under main-root hash (asserts existing F3 gitdir-port behavior; cwd probe resolved) | FR-23 | Worktree regression test over the gitdir port; no raw-cwd hashing anywhere in stamp paths |
| AC-09 | Restart re-declaration line in all three protocols | FR-24 | Doc inspection |
| AC-10 | UDS-path stamp regression — stamp decoration byte-equivalent over the merged vnc-027 UDS transport, not just HTTP | FR-29 | Regression test seam pinned at `transport-uds.encodeFrame` (transport-uds.js:55-62): stamped FNF `RecordEvent` via `index.js` `runFireAndForget` with `config.mode="uds"` carries `cycle_stamp` in the decoded frame, byte-equivalent to the HTTP body for the same input |

## Domain Model & Ubiquitous Language

- **Cycle stamp** — the declared-attribution contract carried per event as `ImplantEvent.cycle_stamp: Option<CycleStampPayload{topic, phase}>`. Presence of the field IS the declared flag; it is the *declared* channel, `topic_signal` remains the *advisory/extracted* channel. They never coexist on a stamped client's events.
- **Cycle tracker** — the client disk file `cycles/{session_key}.json` (`{topic, phase, declared_at, updated}`) in the F3 state dir; the stamp's source of truth. Lifecycle: CREATE on `cycle_start` interception, UPDATE on `phase-end`, CLEAR on `cycle_stop`, IGNORE on SessionStart/SessionClose/Stop, PRUNE at 7 days.
- **session_key** — `sanitizeSessionKey(session_id)`; the root session's session_id at any nesting depth (subagent events inherit it).
- **Precedence chain** — STAMP (write-time, contractual) → MARKER (review-time recovery; tier exists in the contract but its implementation is a deferred named follow-up) → VOTE (fires only on NULL) → UNATTRIBUTED. Presence-gated, not ordered: the vote structurally cannot override a stamp because it is only consulted at NULLs.
- **FeatureSource** — `Declared | Inferred` on registry session state. Declared = set via cycle interception/stamp; Inferred = set via eager attribution or vote. Declared features win over extraction and vote everywhere.
- **topic_source** — per-row attribution provenance column: `declared` / `extracted` / `registry-fill` / `vote` / NULL (write-site mapping in FR-21). The F6 hook.rs-retirement gate's evidence base.
- **stamp_miss canary** — content-free `health.json` counter; **subagent-gated**: increments only when a depth≥1 subagent-context event finds no tracker for its inherited root session_id (inheritance drift). Depth-0 never-declare sessions are structural noise and never increment. A **zero-tolerance invariant** (`stamp_miss == 0`) at test time that carries to production — no threshold, no denominator, no baseline. The tripwire for CLI inheritance/resume drift and the depth>1 unknown.
- **Never-declare floor** — the permanently retained heuristic pipeline (extraction → eager → NULL-fill → vote-on-NULL) serving sessions that never declare a cycle (uni-zero, research, ad-hoc: 60% of sessions, 25% of observation volume). Demote, never delete.
- **Unstamped window** — the mixed-deployment period where Rust-hook (or old-TS) clients send no stamps; served by the floor plus the FeatureSource precedence fix.
- **Inversion** — any code path where an inferred signal beats a declared feature (the #588 class); two sites fixed here (close, sweep).

## Workflows

1. **Declared protocol session (happy path)** — Leader issues `context_cycle(start)` → PreToolUse interception validates and writes the tracker → every subsequent event from the root thread and all subagents (root session_id inheritance) carries the stamp → server records rows with `topic_source='declared'`, registry `FeatureSource::Declared` → `phase-end` updates the tracker → `stop` deletes it → close/sweep resolve the declared feature regardless of vote. Per-turn Stop drain, compaction re-register, and server restart do not interrupt stamping (disk tracker is immune to all three).
2. **Crash + `--resume`** — Process dies mid-cycle; the tracker file persists on disk; `--resume` reuses the session_id (empirical, claude 2.1.167; corpus 86/86) so the first post-resume event finds the tracker and stamping continues with zero gap. The server registry does NOT survive (re-register overwrite) — the stamp is what keeps attribution correct.
3. **Broken session, fresh restart** — New session_id, tracker file misses. Protocol contract (AC-09): the leader's first action is to re-issue `context_cycle(start, topic)` — idempotent server-side, recreates the tracker. Without re-declaration the session degrades to the floor (vote/NULL) by design.
4. **Never-declare session (uni-zero, research, ad-hoc)** — No tracker, no stamp; extraction emits `topic_signal` exactly as today; rows get `extracted`/`registry-fill`/`vote`/NULL sources. This floor must be regression-protected (AC-07 fallback sample).
5. **Worktree session** — Hook fires with cwd = worktree path (probe-confirmed); `walkToProjectRoot`/`resolveGitFile` resolve to the main root; tracker and stamps live under the main-root hash so every thread of the session shares one tracker.
6. **Mixed client/server combinations** — New client + old server: unknown field ignored. Old client (Rust hook) + new server: `cycle_stamp: None` → legacy chain with the inversion fixes (declared registry features now win even unstamped). No flag, no coordination.
7. **Subagent declares (SM-as-subagent / scenario 17)** — Identical to workflow 1; the declaring thread's events carry the root session_id, so the tracker is written under the root key. Last declaration wins (worker clobber is a protocol-authorization concern, visible in the server `Overridden` log — not a mechanism defect).

## Constraints

- **C-01** — Frozen F1 wire contract: additive only (NFR-04).
- **C-02** — Rust `hook.rs` untouched; no feature flag (NFR-05).
- **C-03** — Tracker never copies offsets delete-on-close (FR-04 — ass-072 precondition 5).
- **C-04** — Fail-open client contract (NFR-03).
- **C-05** — Client size budget under the redefined C-04 gate (100,000 B stripped / 160,000 B raw, vnc-027 ADR-005 MERGED); additions ~3,900 B raw / ~2,050 B stripped clear the ~68,907 B stripped / ~112,773 B raw merged tree; fallback = fold `cycles.js` into `state.js` (NFR-01).
- **C-06** — Sync-path budget: no extra sync-trio file I/O (NFR-02).
- **C-07** — Migration discipline: pragma-guarded idempotent ALTER (NFR-06).
- **C-08** — No new npm dependencies (NFR-07).
- **C-09** — Registry escape-hatch fence (SR-05): the precedence chain touches exactly the FeatureSource flag + two inversion flips; everything else in the per-turn-drain/re-register family is a named follow-up.
- **C-10** — Inversion fixes are minimal-diff and their post-fix semantics are documented as a citable interface for crt-052 (SR-10); `infra/session.rs` adjacency with crt-052 means sequential delivery (vnc-027 → vnc-030 → crt-052, pinned).
- **C-11** — Stamp paths route through the project-root walk only; raw cwd is never hashed (FR-23, probe design implication).
- **C-12** — Empirical CLI behaviors pinned to claude 2.1.167 in test assumptions; canary is the drift detector (NFR-08).
- **C-13** — The marker-recovery follow-up issue (with crt-052 transcript-snapshot-seam dependency) must exist before design gate exit (SR-07).

## Dependencies

- **vnc-027 (F4a, #680) — MERGED 2026-06-08** — owned the C-04 size-gate redefinition (ADR-005: 100,000 B stripped / 160,000 B raw, merged) and the merged UDS transport. Preserved the `context_cycle` interception seam (FR-28): narrowed matcher `merge-settings.js:49`, null sentinel `build-request-tools.js:326`. vnc-030 rebases `index.js` orchestration onto the merged tree (zero changes to `build-request*.js`) and discharges vnc-027's post-merge UDS-stamp obligation owed to #699 (AC-10/FR-29).
- **F3 client (vnc-026, shipped)** — `state.js` (atomicWrite, sanitizeSessionKey, prune), `config.js` (`walkToProjectRoot`/`resolveGitFile` gitdir port, landed b2e215fd/#696), `build-request.js` (stamp attach point, transport-agnostic), `topic-signal.js` (suppression + docstring fix), `health.json` (canary field).
- **Server surfaces** — `crates/unimatrix-engine/src/wire.rs` (`ImplantEvent`), `crates/unimatrix-server/src/infra/session.rs` (sweep, registry state, FeatureSource), `uds/listener.rs` (close path, `enrich_topic_signal`, record paths), `crates/unimatrix-observe/src/attribution.rs` (docstring), `services/observation.rs` (windowing — read-only context for #574 check).
- **Migration precedent** — v9→v10 `topic_signal` column (pragma pattern, constraint #4092).
- **ts-rs codegen** — drift-checked bindings, 7th export (constraint #4726).
- **Downstream consumers** — crt-052 (post-fix close/sweep semantics; session selection consumes the inversion fixes), F6 #682 (`topic_source` evidence).
- **Pinned CLI** — claude 2.1.167 (resume reuse, depth-1 inheritance, worktree cwd).

## NOT in Scope

- **Building** the TS UDS transport, parity corpus, hook-set reduction, size-gate/offset-delete carry-items — vnc-027 (F4a, MERGED). vnc-030 does NOT build UDS; it only proves the stamp rides the merged UDS transport byte-equivalently (AC-10/FR-29).
- Deleting any heuristic (extraction, NULL-fill, eager, vote) — all survive as the never-declare floor; full vote retirement never clears the evidence bar, including at F6.
- Marker-recovery implementation — does not exist (OQ1 resolved); deferred to a named follow-up that must consume crt-052's transcript-snapshot seam, never a second buffer reader.
- Server registry lifecycle redesign (per-turn drain, re-register overwrite, mid-session amnesia) — except the FeatureSource flag and the two inversion flips (C-09 fence).
- Any Rust `hook.rs` change.
- #578 audit-log retention (deferred post-OSS-cloud-v1, vnc-029).
- MCP-only hookless clients (scenario 16) — unattributable by construction; named, not solved.
- Giving uni-zero/research sessions declarations — protocol-side decision, separate change.
- Depth>1 subagent inheritance verification — unverifiable until Claude Code lifts the constraint; the canary is the tripwire.
- Tightening the extractor's permissive filter (ass-072 discovery 4) — only its **docstrings** are corrected here (FR-25); behavior change is out of scope.

## Open Questions (for the architect)

- **OQ-A — `topic_source='vote'` row-level write site.** Majority vote today resolves the **session** feature at close/sweep; whether any path writes vote-derived attribution at **row** level (or whether `vote` rows are those resolved through review-time windowing of vote-attributed sessions) must be pinned by the architect at the exact code site, so FR-21's one-source-per-write-site rule holds. If no row-level vote write exists, the architect either defines the backfill that produces `vote` rows or records that `vote` is reachable only via close/sweep backfill and specifies it.
- **OQ-B — Canary surfacing.** FR-10 defines trigger semantics and review points; `health.json` is content-free observability with no alerting infra. The architect decides whether any active surfacing (e.g., a breadcrumb on first increment per session) fits the size and fail-open budgets, or whether passive counter + review-point consultation is the whole mechanism.
- **OQ-C — Post-vnc-027 headroom.** The actual byte headroom after vnc-027's gate redefinition and reductions is unknown until it delivers. The architecture must carry the per-module estimate table (NFR-01) against the gate definition (comment-stripped 100,000 B / raw 160,000 B) and name the fallback now, not at delivery.
- **OQ-D — Marker-recovery follow-up issue.** C-13 requires the named follow-up (with the crt-052 seam dependency) to exist before design gate exit. Filing it is a design-leader action; the architect should reference its number in ARCHITECTURE.md once filed.
- **OQ-E — Canary delivery crux: is "I am a subagent" independent of root-id inheritance? (ADR-006 §7, delivery-resolved — NOT answered here.)** The subagent-gated increment (FR-09) assumes the "I am a subagent" signal (depth≥1 / subagent-context detection) is an **independent** client-side signal from root-id inheritance. ass-072 verified depth-1 root-id inheritance (328/328, claude 2.1.167) — but that is inheritance *working*. The inheritance-break case may be observationally identical to a never-declare session client-side **if a broken CLI also strips the subagent's knowledge that it IS a subagent**. Two branches, resolved empirically by delivery before committing to the production canary: **Branch A** — subagent-context detection survives the drift (signals independent) → ship the client-side production canary (primary recommendation). **Branch B** — co-dependent, breaks together → narrow the canary to the test-time invariant only and DROP the production signal (do not ship a tripwire noisy by construction). The **test-time zero-tolerance invariant (FR-10) ships either way**; only the production canary's existence is delivery-probe-gated. Delivery probes whether SubagentStart / depth indicators are present on hook stdin under a simulated/aged CLI where root-id inheritance is absent. No answer is asserted at spec time.
