# ASS-072: Contractual Cycle Attribution — Client-Side Stamp Lifecycle Across Session Scenarios

## Question

Can a client-side cycle tracker — the TS client persists `{topic, phase}` per-session state when it intercepts `cycle_start`, and stamps that cycle id onto every subsequent event (main-thread and subagent alike) — provide **contractual** feature attribution across all session lifecycle scenarios? And for the scenarios where it can't: what precedence chain (stamp → transcript-marker recovery → vote-on-NULL-only) covers the residue, and can the voting heuristics be retired or only demoted?

## Why It Matters

Attribution today is inference: client-extracted topic guesses feed a server-side tally vote (~90% accurate), subagent noise can dominate it, and #588 shows extracted signals overriding explicit declarations. The stamp replaces inference with contract for every declared session — write-time, transport-agnostic, surviving server restarts — and structurally fixes #588 (extraction stops once a declaration exists; nothing is left to override it).

This is the F4 (#680) attribution redesign decision: F4 currently plans review-time transcript-marker attribution per ass-069 Q3. The stamp is the stronger write-time design — *if* it holds across the real lifecycle scenarios. Fast-follower to F3 (in process); the spike runs against F3's design artifacts so its findings land before F4 design begins.

Verified foundations this design rests on (ass-071 + 2026-06-06 code verification):
- Subagent hook events carry the **parent session's session_id** (`hook.rs:449-453`; empirically confirmed in the live DB) — so per-session state lookup covers subagent events with no special handling.
- The client already intercepts `cycle_start`/`cycle_stop` (the interception F4 keeps regardless).
- F3 already builds per-session client state (`last_offset` for delta streaming) — the tracker is one more field in an existing store.

## Bounded Questions

### Q1: Tracker mechanics and state lifecycle

- Specify the client state lifecycle: create on `cycle_start` interception, phase updates, clear on `cycle_stop`, behavior on crash / resume / compaction. Where does it live relative to F3's per-session state store?
- Stamp carriage: does the stamp ride the existing `topic_signal` payload field (zero wire change) or need a distinct field to disambiguate "declared" from "extracted"? The server must be able to tell contract from guess — recommend the shape (frozen F1 contract: additive only).
- Once stamping is active, does client-side topic *extraction* stop entirely, or continue into a separate advisory field?

### Q2: Scenario × mechanism matrix

Trace each scenario end-to-end (protocol docs + code paths + client state): which mechanism attributes correctly — stamp / marker recovery / vote / unattributed? **The list below is the minimum set; the researcher must hunt for scenarios it misses.**

1. Happy path — `cycle_start` issued, all subagents stem from that session
2. Design/delivery session split — each session declares its own cycle (verify both protocols' declarations actually reach the interception path)
3. Broken session — environment failure mid-protocol, restart skips `cycle_start` (see Q3)
4. Concurrent sessions on different features — no cross-talk between per-session state files
5. Sequential cycles in one session — clear/re-stamp on `cycle_stop` → next `cycle_start`
6. Ad-hoc session that never declares
7. uni-zero / research sessions (never declare — same class as 6?)
8. Worktree-isolated agents (`isolation: worktree`) — different cwd: does project-root detection resolve the same client state and socket? Suspected trap; verify empirically.
9. Compaction mid-session — session_id persists; does state survive?
10. Server restart mid-session — registry state lost; client stamp keeps arriving (scenario should *favor* the stamp over today's registry enrichment — confirm)
11. Subagent spawned before `cycle_start` lands — ordering race at session open

### Q3: Broken-session capture paths (scenario 3 deep-dive)

Measure both recovery paths empirically before concluding voting is still needed:
- **`--resume`**: does a resumed session keep its session_id? If yes, client state (keyed by session_id) survives the crash and stamping continues. Verify with a real resume.
- **Fresh-session marker recovery**: a restarted protocol session's transcript contains the re-entry text ("You are the Delivery Leader… {feature-id}"). Can review-time transcript-marker reading attribute the new session without any `cycle_start`? What does the protocol itself need to guarantee this (e.g., the restart prompt always names the feature)?
- Residue: what fraction of broken-session shapes have *neither* path?

### Q4: Precedence chain design

- Recommend the chain: stamp (primary, contractual) → transcript-marker recovery (review-time backstop) → vote (fires ONLY when the attribution field is NULL — by contract it can never override a stamp or marker).
- Server-side consequences: what happens to `enrich_topic_signal` (NULL-fill survives?), `check_eager_attribution` / `majority_vote_internal` (retire vs NULL-only), and the #588 priority ordering (should become moot — confirm structurally, not by re-ordering).
- Make the parent-session-id inheritance **contractual**: specify the AC + test F4 must carry (subagent-originated event → parent session row), since the stamp depends on it and today it's emergent Claude Code behavior, silently ignored when wrong.

### Q5: Transition compatibility — mixed clients until F6

- Until F6 retires `hook.rs`, local UDS sessions may use the Rust hook, which will never stamp. The precedence chain must hold with mixed clients: stamped sessions (TS) and unstamped sessions (Rust hook) coexisting against one server.
- Verdict: what can actually be *retired* at F4, what waits for the F6 retirement gate, and what config/feature-flag (if any) governs the transition?

## Output

`product/research/ass-072/FINDINGS.md`:
- **Scenario × mechanism matrix** — every scenario, the mechanism that attributes it, and the failure mode if any (primary deliverable)
- Client state lifecycle spec (create / update / clear / crash / resume / worktree)
- Stamp wire-shape recommendation (additive, declared-vs-extracted distinguishable)
- Precedence-chain design + voting verdict (retire / NULL-only / unchanged) with the #588 disposition
- Parent-session-id contract: the AC + test F4 must carry
- Transition plan for mixed clients (F4 → F6)
- Go/no-go on the stamp as F4's primary attribution mechanism

## Constraints & Prior Art

- **Research only** — no production code changes; throwaway empirical checks (resume test, worktree test) are fine.
- **Frozen F1 wire contract** — stamp carriage must be additive; no mutation of existing payload bindings.
- **Demote-don't-delete discipline holds until evidence says otherwise** — the spike may recommend retirement, but the recommendation must address scenario-6/7 residue and the Q5 mixed-client window explicitly.
- Prior art: ass-069 FINDINGS Q1/Q3 (attribution PoC, heuristic demotion case), ass-071 FINDINGS (parent-session inheritance evidence, `Agent` tool_use join), 2026-06-06 verification (hook.rs:449-453, record_topic_signal call sites, enrich_topic_signal NULL-fill), #588 (the inversion bug this absorbs), #680 F4 (the consumer), #679 F3 design artifacts (client core + state store).

## Tracking

GitHub Issue: #694
Consumer: F4 (#680) attribution redesign — findings land before F4 design begins.
