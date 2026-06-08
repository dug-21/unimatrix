# Agent Report — vnc-030-researcher

## Task
Resolve vnc-030's design-time open questions (OQ1 marker-recovery inventory, OQ2 uni-zero attribution provenance) and parallel-design coordination points (vnc-027, crt-052); update SCOPE.md in place.

## Output
- `product/features/vnc-030/SCOPE.md` updated: OQ1/OQ2 resolved with code-level evidence, AC-07 baseline methodology pinned, Proposed Approach item 4 closed, new Background subsections "Server attribution surfaces" and "Parallel-Design Coordination", one still-open question (worktree cwd dump, shared with vnc-027 OQ5).

## Key Findings
1. **OQ1 — marker recovery does not exist; defer confirmed.** `context_cycle_review` attribution is `load_cycle_observations` (`crates/unimatrix-server/src/services/observation.rs:308`): time windows from `cycle_events` DB rows + topic_signal session discovery — never transcript content. The F2 buffer's only content-bearing reader is `TranscriptBuffer::contiguous_tail` (`infra/session_transcript.rs:179`, module doc `:10`), consumed solely by the PreCompact block path (`uds/listener.rs:1646`). Follow-up issue must pin a dependency on crt-052's transcript snapshot seam (its SCOPE interaction warning) instead of adding a second buffer reader.
2. **OQ2 — uni-zero is ordinary extraction→eager/vote; ass-072 UQ-4's premise was wrong.** `is_valid_feature_id` (`crates/unimatrix-observe/src/attribution.rs:15-23`) has NO digit requirement despite the `{alpha}-{digits}` docstrings — any hyphenated ASCII token validates. `/uni-zero` prompt tokens and Skill tool_input JSON yield `uni-zero` via `extract_feature_id_pattern` (`:45-55`). Same class as the live-DB `SHA-256` misfire. AC-07 baseline therefore restricted to declared protocol sessions (declaration = ground truth); never-declare sessions only in the fallback regression sample. Drive-by for delivery: fix both docstrings.
3. **Coordination**: vnc-027 owns C-04 (its AC-09, sequenced first); the OQ5 worktree-cwd dump has NOT been run by either session; shared `build-request.js` surface is compatible (vnc-027 AC-08 preserves cycle interception — our tracker seam); crt-052 consumes our attribution quality and adjoins our `infra/session.rs` close/sweep fixes.

## Items Where vnc-030 Impacts vnc-027 / crt-052 (for Design Leader routing)
1. **→ vnc-027**: vnc-030 will NOT carry C-04; depends on vnc-027 AC-09 landing first (contingency: gate rides with F4b if it ships first). vnc-027's OQ2 server-side-preformatted lean also protects the size budget vnc-030's client additions need.
2. **→ both**: the worktree cwd dump (vnc-027 OQ5 / vnc-030 AC-08) is unowned — assign to one design session.
3. **→ crt-052**: its Delivery Ordering rationale 2 and constraint 12 cite "vnc-027" for attribution/close-sweep adjacency — post-split those point at vnc-030; its "vnc-027 OQ-4 / AC-19" references are stale (marker tier → vnc-030; offset-delete → vnc-027 AC-10).
4. **→ crt-052**: vnc-030's marker-recovery follow-up issue will name crt-052's snapshot seam as a prerequisite.
5. **→ crt-052**: sequential delivery advised — vnc-030 edits `sweep_stale_sessions`/close path in the same files as crt-052's Option B continuity remedy.
6. **→ either F4**: crt-052 OQ-1 Option A (close-reason `SessionClose` field) could ride vnc-030's wire surface as easily as vnc-027's — route if the human picks A.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced #4092 (idempotent ALTER TABLE guard, governs `topic_source` migration), #4726 (ts-rs drift-checked bindings, governs 7th export), #4140 (set_feature_force no-op on absent sessions — stamp removes this dependence), #1067 (eager attribution immutable), #3382 (registry NULL-fill); all cited in SCOPE Background.
- Stored: entry #4801 "Feature-id extractor has no digit requirement — any hyphenated token validates, despite {alpha}-{digits} docstrings" via /uni-store-pattern.
