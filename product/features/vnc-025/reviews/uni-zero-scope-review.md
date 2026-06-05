# uni-zero product review (advisory — human judgment required)

**Gate**: scope-review | **Stance**: Approve — right feature, right position in the F-sequence, cleanly bounded; recommended answers to all three open questions below.

## Vision / roadmap fit

vnc-025 is F2 in the post-ass-069 OSS-cloud finalization sequence (F1 vnc-024 shipped and verified → **F2 this** → F3 vnc-026 #679 → F4 vnc-027 #680 → F5 nan-016 #681 → F6 vnc-028 #682). The sequencing is correct and the timing is right:

- **F3 — the OSS-cloud remote MVP and the strategic crux of `personal-cloud` — is hard-blocked on this buffer existing.** vnc-026's issue lists F2 as a dependency. Nothing else in the open issue landscape competes for this slot.
- **Goal labels on #670 are accurate**: `proactive-delivery` (closes the remote PreCompact-fidelity gap — knowledge restoration at compaction is exactly the proactive surface) and `personal-cloud` (unblocks F3). The `self-learning` payoff was correctly split out to crt-052 (#689), which carries that label itself.
- **The vnc-025 / crt-052 boundary is clean and mutually consistent.** I cross-checked #689: its scope (distill-before-purge insertion, reconstruction fallback, extraction approach pending ass-070 #683) matches vnc-025's non-goals exactly, and vnc-025's cycle-review purge method is explicitly designed as crt-052's future insertion point. No gap, no overlap.
- **Architectural principle alignment is strong**: in-memory-only + purge-on-close honors principle 8 (no secrets in any database — per ADR #4721 this IS the secrets guarantee, there is no scanner); content-free audit honors principle 2; the empty-buffer no-op preserves graceful degradation; the `get_state()` clone constraint protects the principle-7 hot path.

"Ships dark" is acceptable: F3 immediately follows, all behavior is exercisable via direct dispatch, and the only live pre-F3 behavior (PreCompact empty-buffer path) is byte-identical to today (AC-11).

## Approach commentary

**Strengths**

- The background research is code-verified with line references against the post-vnc-024 merge — this is the standard scope research should meet. The `SessionState: Clone` / hot-path `get_state()` deep-copy trap is correctly elevated to the single biggest structural constraint, with an AC (AC-10) guarding it.
- AC-05 explicitly preserves the load-bearing batch-filter non-persistence property from vnc-024 ADR-004 — the most likely regression in this wiring, named and gated.
- The fire-and-forget contract (always `Ack`, no content in logs, silent no-op for unregistered sessions) is carried through goals, approach, and ACs consistently with the PoC-validated semantics.
- New focused module respects the 500-line rule against three already-oversized files.

**Concerns to weigh**

1. **Aggregate memory envelope.** The per-session cap (proposed 4 MiB) bounds one session, but there is no global bound: N concurrent sessions × 4 MiB. For the personal-cloud single-container posture with a handful of sessions this is fine, but the design should state the envelope explicitly and record that a global cap is deliberately out of scope (the 4 h `sweep_stale_sessions` eviction is the existing backstop). I do not recommend building a global cap — just deciding it on the record.
2. **Double-prepend at F4, not here.** The empty-buffer guard correctly prevents double-prepend for today's Rust hook (it never streams deltas). But vnc-027 (F4) replaces hook.rs with a TS UDS client that *does* stream deltas — at that point the client must drop its client-side `prepend_transcript` in favor of the server-side block, or both fire. vnc-025's design is right; the requirement belongs on #680. Recommend adding a note there now so it isn't rediscovered at F4 design time.
3. **Cycle-review purge scan.** The new registry method scans all sessions for `state.feature == feature_cycle` under the mutex. Fine at OSS scale and consistent with constraint 3 (in-place key work only) — flagging only so the design keeps it a scan, not a new index, until evidence demands otherwise.

## Recommended answers to open questions

**Q1 — Buffer bound default, knob home, overflow policy**
Recommend: **4 MiB default, confirmed; knob lives next to `transcript_retention`; ring-tail overflow, confirmed.**
Rationale: PreCompact needs only the 12 KB tail, so 4 MiB is already generous headroom for crt-052's future distillation window — ass-070 will tell us if distillation wants more, and the knob makes that a config change, not a redesign. The knob belongs beside `transcript_retention` because they are the two halves of one transcript-policy surface, and the enterprise seam (goal 6) reads that section as a unit. Ring-tail is the simplest policy that satisfies the only current reader, and crt-052's reconstruction fallback already covers the lost-head case.

**Q2 — Gap handling: full covered-range tracking vs tail-contiguity check**
Recommend: **the simpler tail-contiguity check, with the representation encapsulated in `TranscriptBuffer` so range tracking is a local retrofit.**
Rationale: PreCompact is the only buffer reader in vnc-025; the sole hard requirement is never serving NUL-filled holes in the tail block, and the contiguity check meets it. Building full range tracking now would be speculative design for crt-052's fallback trigger before ass-070 has reported — it risks pre-shaping that decision with no current consumer. Encapsulation in the new module keeps the retrofit cost low if crt-052 wants it. This also matches the feature's own "like-for-like only" framing.

**Q3 — Composite-key seam: documented constructor vs full `SessionKey` newtype re-key**
Recommend: **documented constructor seam; defer the re-key to enterprise.**
Rationale: re-keying every registry call site for a dimension OSS never populates (`tenant = "default"`) is churn with zero OSS behavior change and a real regression surface across `session.rs`/`listener.rs`/`tools.rs` hot paths. The constructor seam gives enterprise exactly one place to change, which is the whole point. Consistent with principle 6's zero-required-infrastructure posture and with how every other enterprise seam in this scope is handled (config-read retention match, capability inheritance).

## Recommended actions

1. **Approve the scope** with the Q1–Q3 answers above carried into design.
2. Ask design to **state the aggregate memory envelope** (per-session cap × expected session count) and record the no-global-cap decision explicitly.
3. **Add a note to #680 (vnc-027 / F4)**: the TS UDS client must not client-side prepend the transcript block once it streams deltas — server-side block supersedes `prepend_transcript`.
4. No changes to goal labels, sequencing, or the crt-052 split — all verified consistent.
