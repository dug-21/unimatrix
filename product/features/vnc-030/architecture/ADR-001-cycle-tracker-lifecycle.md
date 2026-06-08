## ADR-001: Cycle Tracker — `cycles/{session_key}.json` Lifecycle in a New `cycles.js` Module, state.js Atomics, No Delete-on-Close

### Context

ass-072 (GO) specifies a client-side per-session tracker so attribution becomes contractual: the declared topic must survive the three server-state-loss events (per-turn Stop→SessionClose registry drain, compaction/resume `register_session` overwrite, server restart). F3's `state.js` already provides atomic write (temp+rename, state.js:76), `sanitizeSessionKey` (:40), and an age-prune pattern (`pruneOffsets`, :133, 7-day threshold). The offsets lifecycle deletes on SessionClose (FR-16) — which fires **every assistant turn** (Rust hook maps `Stop|TaskCompleted => SessionClose`, hook.rs:478) — so copying it would kill the stamp after turn 1 (binding SCOPE constraint; ass-072 precondition 5). The cwd probe (agents/vnc-030-cwd-probe-report.md) proved hook cwd carries the **worktree** path and that F3's gitdir port (`walkToProjectRoot`/`resolveGitFile`, config.js:42-103) resolves it to the main root in production.

### Decision

1. **New module `lib/hook-client/cycles.js`** (~2.2 KB raw / ~1.1 KB stripped, measured estimate) — not folded into `state.js` (9,489 B, distinct responsibility) and not into `build-request*.js` (those stay pure). API, all never-throw (F3 C-05):
   - `readCycle(stateDir, sessionId) -> {topic, phase}|null` — missing/corrupt/mistyped → `null`, never throws.
   - `writeCycle(stateDir, sessionId, topic, phase) -> bool` — full file `{topic, phase, declared_at, updated}` via `state.atomicWrite`; create-or-overwrite (last declaration wins — declaration-source-agnostic per ass-072 scenario 17; protocol, not mechanism, authorizes who declares).
   - `updatePhase(stateDir, sessionId, phase) -> bool` — read-modify-write, atomic; missing file → recreate is NOT attempted (phase-end without start is a protocol violation; degrade to no-op `false`).
   - `deleteCycle(stateDir, sessionId) -> bool`.
   - `pruneCycles(stateDir)` — age > 7 days since `updated` (same policy/shape as `pruneOffsets`); called where `queue.prune` already runs on the FNF path.
2. **File**: `~/.unimatrix/{projectHash}/hook-client/cycles/{sanitizeSessionKey(sid)}.json`, beside `offsets/` (vnc-026 ADR-003 state-dir layout inherited unchanged). Raw `session_id` is passed in; sanitization happens inside (pattern #4772: never pre-sanitize at call sites).
3. **Lifecycle (ass-072 client-state-lifecycle spec, verbatim)**:
   | Trigger | Action |
   |---|---|
   | `CYCLE_START_EVENT` frame built (post-validation by construction — failed `validateCycleParams` never yields a CYCLE_* frame) | `writeCycle(topic = payload.feature_cycle, phase = payload.next_phase ?? null)` |
   | `CYCLE_PHASE_END_EVENT` frame | `updatePhase(payload.next_phase ?? null)` |
   | `CYCLE_STOP_EVENT` frame | `deleteCycle` |
   | SessionStart (startup/resume/clear/compact), SessionClose, Stop | **never touch the file** |
   | Crash / kill -9 | file persists; `--resume` reuses session_id (empirical, claude 2.1.167) → stamping continues with zero gap |
   | Fork (`--fork-session`) | new id → file miss → marker-recovery class (deferred follow-up, ADR-007 §4) |
   | Age > 7 days | pruned |
4. **Worktree routing**: the tracker path derives exclusively from `config.resolve(cwd).stateDir` (which routes through `walkToProjectRoot`/`resolveGitFile`) — **never from a hash of raw cwd**. A worktree-issued `cycle_start` therefore lands under the main-root hash where every other thread reads it (AC-08 asserts this existing behavior with a stamp-path regression test).

### Consequences

Easier: stamp survives all three server-state-loss events by being a disk file keyed by session_id; crash+`--resume` recovery is free; concurrent sessions are isolated by key; the module reuses proven F3 machinery (no new failure modes in atomics/sanitize). Harder: one more module against the size budget (measured, fits — ARCHITECTURE.md budget table); the 7-day prune means a session idle >7 days resumes unstamped (acceptable — same policy as offsets, vote floor catches it); `updatePhase` no-op-on-missing means a phase-end after manual file deletion silently degrades (fail-open by contract).

Cross-references: ass-072 Q1/Q3, SCOPE AC-01/AC-08, ADR-002 (who calls these functions), ADR-006 (canary), ADR-007 §1 (interception seam), vnc-026 ADR-003, pattern #4772.
