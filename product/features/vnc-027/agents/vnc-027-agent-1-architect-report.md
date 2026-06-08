# Agent Report — vnc-027-agent-1-architect

Task: architecture for vnc-027 (TS UDS hook client + hook-set reduction, F4a, #680).

## Artifacts

- `product/features/vnc-027/architecture/ARCHITECTURE.md`
- `product/features/vnc-027/architecture/ADR-001-uds-sync-preformatted.md` (Unimatrix #4802)
- `product/features/vnc-027/architecture/ADR-002-transport-adapter-sendresult.md` (#4803)
- `product/features/vnc-027/architecture/ADR-003-node-socket-lifecycle.md` (#4804)
- `product/features/vnc-027/architecture/ADR-004-hook-set-reduction.md` (#4805)
- `product/features/vnc-027/architecture/ADR-005-size-gate-c04.md` (#4806)
- `product/features/vnc-027/architecture/ADR-006-offset-delete-rekey.md` (#4807)
- `product/features/vnc-027/architecture/ADR-007-projecthash-socket-path.md` (#4808)

## Key Decisions

1. **OQ2 / SR-02 / SR-03 (ADR-001)**: UDS sync responses are server-side preformatted. Wire mechanism: additive `accept: Option<String>` (`skip_serializing_if`) on `ContextSearch`/`CompactPayload`, injected by transport-uds at serialization time (never by builders, never queued); new `HookResponse::Text { body }` variant carrying the exact HTTP text/plain bytes; `Ack` for empty injection (204-equivalent); one shared injection-text core with `observe_response_to_http`. No `format_injection` JS port — the size-budget driver is gone. Mechanical `accept: None` additions to hook.rs construction sites are within the freeze, proven by byte-unchanged goldens.
2. **Transport adapter (ADR-002)**: transport-uds.js implements the transport-http `post -> SendResult` contract with a normative frame→SendResult mapping; transform/state/queue/delta untouched. Selection: remote config → HTTP (wins always, OQ1 confirmed); else UDS. Connection-per-frame; fixed 40 ms parity timeouts (`HOOK_TIMEOUT` oracle).
3. **SR-01 / SR-05 (ADR-003)**: Node socket lifecycle as a contract — `socket.end(frame)`, resolve on `'finish'`, never `destroy()` unflushed; sync = half-close then chunked read loop with settle-once timers; no `process.exit`; enqueue only on failed carrying result; server-side truncation-detection test required.
4. **OQ3 / SR-06 (ADR-004)**: PreToolUse matcher narrowed to `context_cycle|mcp__unimatrix__context_cycle` at install + null no-send sentinel in the client (F-02 gate kept); SubagentStop default-off behind `unimatrix.hooks.subagent_stop` settings key. Parity bar split: transport parity full, event-set parity a non-goal with enumerated exclusions.
5. **C-04 (ADR-005)**: stripped ≤ 100,000 B + raw ≤ 160,000 B, removal-only state-machine stripper with regex-literal discipline and embedded self-test; first merge of the feature.
6. **FR-16 (ADR-006)**: offset delete rekeyed to TaskCompleted; `pruneOffsets` (found to be caller-less dead code) wired into the FNF path as the 7-day safety net; explicit HTTP-unchanged AC required (SR-08).
7. **SR-12 (ADR-007)**: socket path derived from the one existing config.js hash derivation. Settled empirically: TS hash for main repo, worktree, and subdir = `0d62f3bf1bf46a0a`, identical to the live daemon's data dir and raw `sha256(path)[:16]`. Cross-language hash-fixture corpus mandated.

## Notable Findings for Downstream Agents

- HTTP ingest rewrites session ids to `http-{sid}` (`prefix_session_id`); UDS does not — cross-transport queue replay is accepted by both ingests but splits session attribution (documented, accepted; SR-10).
- The listener handles exactly one frame per connection (listener.rs:377-516); Rust replay reconnects per frame — connection-per-post is true parity, not a simplification.
- Mixed-client PreCompact double-prepend (SR-11) is real for TS-streams-then-Rust-fires; binding assumption: one project, one client.
- Dogfood drop-detector (SR-07) needs zero new code: state.json breadcrumbs + queue residue + daily db event counts; rollback trigger defined in ARCHITECTURE.md.

## Open Questions (for spec / human)

1. OQ5 residual: capture one live worktree-subagent `cwd` stderr dump during the dogfood soak (confirmation only; hash fixtures make the answer immaterial).
2. Confirm SubagentStop opt-in key name `unimatrix.hooks.subagent_stop`.
3. Lone-surrogate divergence (#4788): kept as a formally excepted divergence; scoping the fix in is a human option, independent of transport work.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- 9 relevant entries; applied #4798 (UDS/HTTP formatting asymmetry — resolved by ADR-001), #4795 (vnc-024 ADR-003 + injection-header wire-contract amendment), #4743 (shared-core parity-by-construction), #3448 (expected FNF error taxonomy), #4780 (size-gate rework lesson), #4788 (lone-surrogate divergence + Layer-1 parity mechanics)
- Stored: entries #4802 "ADR-001 vnc-027: UDS Sync Responses Server-Side Preformatted via Additive accept Field + HookResponse::Text Variant", #4803 "ADR-002 vnc-027: transport-uds.js Adapts UDS Frames to the Existing SendResult Contract", #4804 "ADR-003 vnc-027: Node UDS Socket Lifecycle Contract", #4805 "ADR-004 vnc-027: PreToolUse Narrowed Matcher + No-Send Sentinel; SubagentStop Opt-In", #4806 "ADR-005 vnc-027: C-04 Size Gate", #4807 "ADR-006 vnc-027: FR-16 Offset Delete Rekeyed to TaskCompleted", #4808 "ADR-007 vnc-027: Socket Path from config.js projectHash — SR-12 Settled Empirically" via context_store (category: decision, topic: vnc-027)
- Deprecations: none — no prior ADR is superseded; #4798 stays active until the asymmetry it documents is actually removed by the shipped code (retro deprecates it post-merge)
