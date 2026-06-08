## ADR-006: FR-16 Offset Delete — SessionClose Delete Removed; Age-Prune Is the Effective Mechanism (TaskCompleted Branch Retained but Unregistered)

*Amended 2026-06-08 per RISK-TEST-STRATEGY R-04: original version claimed TaskCompleted produced a RecordEvent frame and treated TaskCompleted keying as the primary deletion mechanism. Both corrected below.*

### Context

The delta-streaming offset file (`hook-client/offsets/{sid}.json`) is deleted on
every successful Stop→SessionClose (index.js:268-269). Claude Code fires Stop per
turn, so each turn deletes the offset and the next delta send re-streams the
transcript from offset 0 — wasted bandwidth and server-side merge work every turn.
Carry-item (#680 comment, 2026-06-08): key the delete to `TaskCompleted` and/or
age-prune. SR-08: this code path is shared with the HTTP transport — the change
must be the minimal key swap, not a streaming redesign. Verified during design:
`state.pruneOffsets` (7-day cutoff, state.js:133) exists but has NO caller — its
doc claims "called opportunistically on FNF spawns" but it is currently dead code.

**Verification finding (R-04, code-confirmed)**: `TaskCompleted` is recognized by
the client (normalize.js:43-44 canonical event; normalize.js:90-91) and built into
a frame (build-request.js:60) — but it is registered **nowhere**: not in
`merge-settings.js HOOK_EVENTS` (lines 29-39) and not in this repo's
`.claude/settings.json`. The host never spawns the hook with `TaskCompleted`, so
a delete keyed to it is unreachable end-to-end. Additionally, the original version
of this ADR claimed TaskCompleted produces a RecordEvent FNF frame — wrong:
build-request.js:59-66 routes `Stop` and `TaskCompleted` through one shared case
building a **SessionClose** frame. This makes frame-type keying actively dangerous
(a Stop spawn is also a SessionClose frame and would wrongly delete every turn);
keying must discriminate by canonical event name.

### Decision

1. **Remove the SessionClose delete** in `runFireAndForget` — the
   `request.type === "SessionClose"` delete goes away. This alone fixes the
   per-turn re-stream (the actual defect FR-16 targets).
2. **Age-prune is the effective deletion mechanism (option a, decided)**: wire
   `state.pruneOffsets(config.stateDir)` alongside the existing `queue.prune` at
   the top of `runFireAndForget` — making the documented 7-day behavior real.
   AC-10's wording ("keyed to `TaskCompleted` and/or the existing age-prune")
   explicitly permits age-prune-only; uni-zero review pre-endorsed it as the
   honest fallback. We do **NOT** register `TaskCompleted` in `HOOK_EVENTS` or
   settings.json (option b, rejected): adding a hook event to serve an offset
   cleanup optimization directly contradicts this feature's hook-set-reduction
   goal (ADR-004), expands the per-session spawn count, and buys nothing the
   7-day prune does not already provide. A pruned mid-session offset degrades to
   one full re-stream — safe (idempotent server-side merge).
3. **TaskCompleted delete branch retained as a zero-cost forward provision**:
   `runFireAndForget` receives the canonical event (or an `isTaskCompleted`
   flag) from index.js and deletes the offset when the carrying send succeeds
   AND the canonical event is `TaskCompleted` — a single equality check, no new
   code path. The keying is by **canonical event name, never frame type**
   (Stop and TaskCompleted are both SessionClose frames). The branch is
   unreachable under current host registrations — this is documented and
   deliberate, not silent (FR-22-by-analogy satisfied by this ADR plus the
   pinning unit test): it is covered by a unit test proving it fires if the
   event ever arrives (e.g., a future host version registering it, or manual
   invocation), and the assertable negative — a Stop spawn must NOT delete —
   guards the discrimination. If keeping the branch costs anything beyond the
   flag and one conditional, drop it and keep age-prune only.
4. **Scope guard (SR-08)**: no other delta/offset behavior changes — offset
   write cadence, delta frame format, 1 MiB caps, and the ADR-004(vnc-024)
   never-queue-delta rule are untouched. The change is transport-agnostic and
   applies identically to HTTP and UDS (intended: the per-turn re-stream hurts
   the HTTP path most). Spec must carry an explicit AC that the HTTP path's
   externally visible behavior is unchanged except the delete timing.

### Consequences

Easier: per-turn full re-streams stop — offsets survive across turns; the F2
buffer receives true deltas; dead `pruneOffsets` code becomes the documented and
sole effective cleanup; the hook set stays reduced (no new event registration);
the unreachable-branch question is resolved by explicit decision, not silence.

Harder: offset files now live up to 7 days, so disk residue grows slightly
(bounded: one small JSON per session, pruned); every session re-streams from 0
once after its offset is age-pruned — acceptable; the TaskCompleted branch is
test-only dead code until a host registers the event — reviewers must not
"clean it up" without revisiting this ADR; tests keyed to the old
SessionClose-delete behavior must be updated deliberately, not deleted (the
delete must NOT fire on SessionClose/Stop — now an assertable negative).

Cross-references: FR-16 (vnc-026), SR-08, R-04 (RISK-TEST-STRATEGY), ADR-004
(hook-set reduction — why option b was rejected), vnc-024 ADR-004 (delta frames
never queued), pattern entry #4809 (verify event registration before keying
behavior to it).
