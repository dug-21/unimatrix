## ADR-008: Option B Held-Buffer Store — Held-Count Cap + Independent Stale-Sweep TTL, Loud Re-Adoption

### Context
OQ-1 resolved (binding) to Option B, server-only transcript hold; Option A (close-reason wire field) is
off the table (no wire change). The per-turn drain (#4799) frees the buffer at every `Stop`→`SessionClose`,
so a multi-turn session's buffer is empty at review and the primary path is starved to the 0.81 fallback
in every realistic session. The remedy: buffers survive `drain_and_signal_session`, keep merging deltas,
re-adopt on re-registration, and purge only at cycle review (post-distill) and stale sweep. This is the
most state-machine-heavy component and the dominant risk surface (SR-01/SR-02/SR-03). Two hazards:
- **SR-01 (High):** held sessions are unbounded by default — a never-reviewed/never-swept session
  (drift, crash, mis-attribution) leaks one buffer (up to cap) per session; held-count has no natural
  ceiling, so memory = `cap × held-count` is unbounded. Reliance on cycle-review to reclaim is unsafe.
- **SR-02 (High):** a held buffer must rebind to the **same** `feature_cycle` on re-registration; #981
  shows NULL/mis-set `feature_cycle` silently breaks the pipeline. A held buffer re-adopted under the
  wrong cycle silently mis-scopes candidates.

Constraint 13: `drain_and_signal_session` / `clear_transcripts_for_feature` were deliberately left
untouched by vnc-030 ADR-007 §2 (#4819) — diffs must stay minimal and cite, not rework, the precedence
interface.

### Decision
A new bounded server-only structure `unimatrix-server/src/infra/transcript_hold.rs`, owned by the
registry, holding `Arc<Mutex<TranscriptBuffer>>` keyed by session, with **two independent reclamation
mechanisms** (both mandatory — neither alone bounds memory):

```rust
struct TranscriptHold { /* session_id -> HeldBuffer { arc, feature_cycle, last_activity_at } */ }
fn hold_on_drain(&self, session_id, arc, feature_cycle);          // called from drain, minimal diff
fn readopt(&self, session_id, registering_feature_cycle) -> Option<Arc<Mutex<TranscriptBuffer>>>; // re-register reclaims ONLY on cycle match (SR-02)
fn sweep_expired(&self, ttl_secs) -> Vec<TranscriptPurgeRecord>;  // independent TTL stale-sweep
fn purge_held_for_feature(&self, feature_cycle) -> Vec<TranscriptPurgeRecord>; // post-distill at review
```

1. **Held-count cap (SR-01):** `transcript_hold_max_sessions` (config knob). When the cap is hit,
   evict oldest-`last_activity_at`-first; **every eviction emits the purge audit** (ADR-009) so cap-hit
   eviction is never silent. Memory is bounded by `buffer_cap × max_sessions` regardless of review
   cadence.
2. **Independent stale-sweep TTL (SR-01):** `transcript_hold_ttl_secs` (config knob). The existing
   `sweep_stale_sessions` path also sweeps the hold by TTL on `last_activity_at` — reclamation does NOT
   depend on cycle-review ever firing. The two mechanisms (cap, TTL) are independent; either bounds
   memory alone, and both run.
3. **Re-adoption is loud (SR-02):** on `SessionRegister`, if a held buffer exists for the session,
   `readopt(session_id, registering_feature_cycle)` rebinds it **only if** the re-registering
   `feature_cycle` matches the held `feature_cycle`. The caller MUST pass the re-registering cycle —
   hence `readopt` is **2-arg** (Gate 3a ratification: supersedes the earlier 1-arg
   `readopt(session_id)` form; the match is impossible without the registering cycle, R-01/AC-11(b)).
   On mismatch: **fail loud** — do NOT silently re-adopt under the wrong cycle; drop
   the held buffer (treat as a fresh session) and emit a diagnostic (metadata-only, no content). The
   re-adoption key is derived from the contract-attributed `feature_cycle` (vnc-030 ADR-007 §2 makes a
   declared cycle un-flippable), and the spec cites #981. Held buffers keep merging deltas for held
   sessions (deltas route to the held `Arc` between drain and re-register/sweep).
4. **Purge at review (post-distill) and sweep:** `take_transcripts_for_feature` (ADR-001) scans both
   registered AND held buffers for the feature; purge fires after distill via `purge_held_for_feature`
   + the existing `clear_transcripts_for_feature`. Diffs to `drain_and_signal_session` are minimal: it
   gains a `hold_on_drain` call and otherwise keeps its vnc-030-shipped shape (Constraint 13).

This realizes `TranscriptRetention::PurgeOnCycleClose` as named — the current purge-at-every-turn-close
is MORE aggressive than the policy and starves it; Option B makes the buffer survive turns and purge at
cycle close as the policy intends.

### Consequences
Easier: AC-11 is concrete (≥3 simulated turn boundaries, re-adopt on re-register, held merge continues,
bounded memory, sweep reclaims, purge/audit fires exactly once per held session at review/sweep); memory
is provably bounded by two independent mechanisms (SR-01 closed); mis-scoped re-adoption is impossible
because mismatch fails loud (SR-02 closed). Harder: this is the heaviest state machine in the feature
and the only pre-merge proof of the primary path is simulation (assumption: a re-adoption gap not
exercised by the ≥3-turn test surfaces only post-merge — the spec must make AC-11 a hard gate); delta
routing to held buffers adds a lookup on the drain→re-register window that must not regress the
microsecond lock discipline (Arc-clone under registry lock, merge under buffer lock — vnc-025 ADR-001);
the cap-eviction policy must emit audit or it reintroduces silent loss. This is Wave B (ADR-009),
layered on a Wave A that is correct without it. Cross-refs: ADR-001 (seam scans held buffers), ADR-009
(audit-shape move + staging), #4799, #981, vnc-030 ADR-007 §2 #4819, vnc-024 ADR-005 #4721, SR-01/SR-02.
