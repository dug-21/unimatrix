# Scope Risk Assessment: vnc-030

Mode: scope-risk · Date: 2026-06-08 · Inputs: SCOPE.md (approved 2026-06-08), ass-072 FINDINGS, PRODUCT-VISION.md

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Stamp correctness rests on **emergent, uncontracted Claude Code behavior**: `--resume` session_id reuse (empirical on claude 2.1.167 only) and root-session_id inheritance for subagent events. A CLI upgrade silently breaks attribution with no error path. | High | Med | Architect: treat AC-06's `stamp_miss` canary as a hard design invariant, not an add-on; spec should pin the verified CLI version in test assumptions and define a canary alert threshold so drift is detected, not just counted. |
| SR-02 | **Client size budget is 3 bytes** (99,997/100,000). Every client addition (tracker, stamp attach, suppression, canary) is unlandable unless vnc-027's C-04 gate redefinition ships first AND frees enough headroom — which itself depends on vnc-027's OQ2 lean (server-side preformatted UDS responses). Two stacked external dependencies on one budget. | High | Med | Architect: size each client addition up front (~30 lines per ass-072 is a claim, not a measurement); define a fallback if post-vnc-027 headroom is insufficient (e.g., trade comments/breadcrumbs). Spec: make the budget a numbered constraint with a measured estimate per module. |
| SR-03 | New `cycle_stamp` wire field repeats a known regression class: Unimatrix #3486 — new context_cycle fields were extracted from tool_input but **not inserted into payload** at hook interception. Also frozen-F1 fixtures must pass byte-unchanged with a 7th ts-rs export. | Med | Med | Spec: explicit AC that the field round-trips end-to-end (client attach → server read → row), plus the existing fixture-unchanged AC-02. Cite #3486 in the brief so delivery checks both ends. |
| SR-04 | `topic_source` migration is low-risk (pragma-guarded ALTER, #4092/#4358 precedent) but its **value vocabulary** (`declared/extracted/registry-fill/vote/NULL`) is the F6 gate's evidence base — a wrong/ambiguous taxonomy now poisons a future retirement decision. | Med | Low | Spec: define each source value's exact write site and precedence-tier mapping; one row source per code path, no "best guess" values. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Non-goal "server registry lifecycle redesign … **except where the precedence chain requires it**" is an open-ended escape hatch. The inversion fixes sit inside the same per-turn-drain/re-register machinery ass-072 flagged as broken (discoveries 2/3); delivery can be pulled into the amnesia-bug family. | Med | Med | Architect: enumerate exactly which registry touchpoints the precedence chain requires (expected: FeatureSource flag + the two `or_else` flips, nothing else) and name everything beyond that as out-of-scope follow-ups in the architecture doc. |
| SR-06 | Heuristic demotion modifies paths that are the **only** attribution for 60% of sessions / 25% of observations. AC-07's accuracy methodology is novel (declared-sessions-only denominator) and its fallback regression sample is thin ("at least one never-declare session"). A regression here is product-level: silent attribution loss for uni-zero/research sessions. | High | Med | Spec: strengthen AC-07's fallback sample (multiple never-declare session shapes: uni-zero, research, ad-hoc); require before/after `topic_source` distribution comparison on the live DB, not just accuracy. |
| SR-07 | Precedence chain ships with a **hole at tier 2**: MARKER recovery does not exist (OQ1 resolved: deferred). Fork-resume (scenario 14) and fresh-restart-without-redeclaration fall through to VOTE/NULL until the follow-up lands — and the follow-up is gated behind crt-052's snapshot seam, which delivers later. | Med | High | Spec: word AC-04 as "stamp → (marker when present) → vote-on-NULL" so the missing tier is explicit, and require the named follow-up issue (with crt-052 seam dependency) to exist before design gate exit. |
| SR-08 | #588 disposition ("close via PR, or convert any residue to a follow-up") leaves "residue" undefined — risk of closing a 20.2%-surface issue while the unstamped-window FeatureSource remedy only partially covers it. | Low | Med | Spec: state the exact #588 claims AC-04 resolves vs. what remains (mixed-window extracted-vs-declared for Rust-hook sessions) so the close decision is mechanical. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-09 | **Three in-flight features share two files.** vnc-030 rebases `build-request.js` on vnc-027, whose AC-08 retires standalone PreToolUse observation — the very event class the tracker's interception seam lives in. If vnc-027's reduction drifts during its own delivery, vnc-030's anchor moves. (Pattern #924: group parallel work by file to avoid conflicts.) | High | Med | Architect: state the interception-seam contract as a named assumption with a seam-survival assertion/test vnc-030 runs post-rebase; coordinate with vnc-027's design before its gate closes. |
| SR-10 | `infra/session.rs` adjacency with crt-052: vnc-030 edits `sweep_stale_sessions`/close path; crt-052 edits `drain_and_signal_session`/`clear_transcripts_for_feature` in the same files, and crt-052's session selection **consumes** vnc-030's inversion fixes. A vnc-030 slip or behavior change ripples into crt-052's design assumptions. | Med | Med | Architect: document the post-fix close/sweep semantics as an explicit interface crt-052 can cite; keep the inversion fixes minimal-diff (flip `or_else` order + FeatureSource guard) to ease the downstream rebase. |
| SR-11 | AC-08 (worktree stamping) depends on the still-open **worktree cwd dump** (shared OQ with vnc-027, owner: this design session, first task). Until run, the live exposure rate and the AC's test shape are unknown; vnc-027 also blocks on the result. | Med | High | Run the dump as design task #1 (already assigned); cross-post to #680 before the architect finalizes AC-08's test design. Do not let architecture proceed past the client section without it. |
| SR-12 | #574 relocation (cycle_events writes moving to the MCP handler) is checked only as a one-line design-time no-race note. If #574 lands between design and delivery, `load_cycle_observations` windowing inputs change under vnc-030's feet. | Low | Low | Architect: record the no-race argument AND the assumption's expiry condition (re-check if #574 merges before vnc-030 delivers). |

## Assumptions

1. **`--resume` reuses session_id** (SCOPE Background → Attribution; ass-072 Q3). Empirical on one CLI version. If wrong/drifting: crash recovery degrades to VOTE/NULL silently → SR-01.
2. **vnc-027 delivers first and its C-04 redefinition yields usable client headroom** (SCOPE Background → TS client, Size). If wrong: every client-side AC (AC-01/02/03/06/08) is blocked → SR-02.
3. **vnc-027 preserves the `context_cycle` interception seam** (SCOPE Parallel-Design Coordination). "Compatible by design" is asserted from vnc-027's SCOPE, not its architecture → SR-09.
4. **F3 gitdir port covers the stamp path** (SCOPE Goal 4: "verify, not re-implement"). Verified at parity for event capture, not yet for cycles-tracker writes → covered by AC-08, contingent on SR-11.
5. **Hook stdin `cwd` carries the worktree path** (SCOPE Open Question 1). Assumed by the ass-072 trap analysis; unmeasured → SR-11.
6. **Subagent events carry the root session_id at any depth** (SCOPE Goal 4 / AC-06). Verified depth-1 only; depth>1 explicitly unverifiable (SCOPE Non-Goals) → SR-01 canary is the sole mitigation.

## Design Recommendations

1. **Sequence the worktree cwd dump first** (SR-11) — it is the only unresolved input gating an AC, and vnc-027 is also waiting on it.
2. **Treat the size budget as a designed quantity, not a hope** (SR-02): architecture should carry a per-module byte estimate table and a named fallback if post-vnc-027 headroom is short.
3. **Make external-behavior dependence observable** (SR-01): canary thresholds + pinned CLI version in test assumptions; the spec should define what a climbing `stamp_miss` triggers.
4. **Fence the registry escape hatch** (SR-05): architecture enumerates the exact registry touchpoints; anything else becomes a named follow-up next to ass-072 discoveries 2/3.
5. **Protect the never-declare floor** (SR-06): demotion ACs need a fallback regression sample broader than one session, plus a live-DB `topic_source` distribution check.
6. **Write the two cross-feature contracts down** (SR-09, SR-10): interception-seam assumption (vs vnc-027) and post-fix close/sweep semantics (for crt-052) as citable architecture sections.
