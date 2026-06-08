## ADR-009: Audit-Shape Move to Review/Sweep with Named No-Consumer Verification; Wave A/B Rollback Boundary

### Context
Two coupled concerns:
1. **Audit-shape move (OQ-3 / SR-03):** vnc-025 ADR-004 (#4742) emits the content-free
   `transcript_session_purged` audit (operation, agent_id "server", session_id, outcome,
   `detail "bytes=<n> trigger=<session_close|stale_sweep|cycle_review>"`, never content) at each purge
   point. Option B (ADR-008) moves the bytes' lifetime: buffers no longer purge at per-turn close; they
   purge at cycle review (post-distill), stale sweep, or cap-eviction. The audit must move with them.
   This changes vnc-025's shipped audit **timing/cadence**. SR-03: a downstream consumer keying on
   per-close audit cadence would silently break.
2. **Delivery staging:** the pinned position requires a clean cut so delivery ships two waves in one PR
   with a rollback boundary — (a) the provable pipeline, fully testable on fixtures without (b) the
   Option B held-buffer state machine.

### Decision
**Audit-shape move (SR-03 / OQ-3):**
- The `transcript_session_purged` event SHAPE is unchanged (content-free; `trigger` enum gains/uses
  `cycle_review` and `stale_sweep`; per-turn `session_close` trigger goes away for held buffers).
- The audit fires **exactly once per held session** at the point it is actually purged (review,
  sweep, or cap-eviction) — AC-11. Cap-eviction (ADR-008) emits it too, so eviction is never silent.
- **Named verification (gate condition, SR-03):** before the audit points move, the spec/risk phase
  MUST run and record a survey confirming **no downstream consumer keys on per-close
  `transcript_session_purged` cadence** — checking `gc_audit_log` (crt-036, which only GC's by age),
  any retention/analytics reader of the audit log, and any test asserting per-close emission. The
  result is recorded as an explicit gate condition; the move does not land until the survey is clean.
  The new contract is documented: "exactly once per held session at review/sweep/eviction" (AC-11).

**Wave A / B rollback boundary:**
- **Wave A (provable pipeline):** snapshot seam (ADR-001/002), pure selection (ADR-003), candidates
  outside the memoized struct (ADR-004), one helper at four returns + exhaustive gate (ADR-005),
  reconstruction fallback (ADR-006), loss visibility (ADR-007), config knobs. Wave A is **complete and
  correct with NO Option B**: if every buffer is empty at call time (the per-turn-drain reality), it
  degrades cleanly to the reconstruction fallback. All Wave A components are unit/integration testable
  on committed fixtures with no held-buffer machinery.
- **Wave B (continuity remedy):** the Option B held-buffer store (ADR-008) and the audit-shape move
  above. Wave B is layered on top: `take_transcripts_for_feature` (ADR-001) gains held-buffer scanning,
  `drain_and_signal_session` gains a `hold_on_drain` call (minimal diff, Constraint 13), and the audit
  points move.
- **Rollback boundary:** the two waves are separable commits/feature-flag-able within one PR. Wave A
  has zero compile-time or test dependency on `transcript_hold.rs`; reverting Wave B leaves Wave A
  shipping (degraded to fallback, AC-07) — a safe rollback target. The seam return type (ADR-002) is
  identical with and without Wave B, so no interface churn across the boundary.

### Consequences
Easier: SR-03 closed by a named pre-move survey rather than an assumption; AC-11's "exactly once per
held session" is a concrete audit-cadence contract; delivery can stage and roll back at a clean line
because Wave A is independently correct (the per-turn-drain reality means "everything falls back" is a
real, tested mode, not a degenerate one). Harder: the audit `trigger` enum and its consumers must be
updated in lockstep with the purge-point move (the survey is the guard); maintaining Wave A's full
independence from Wave B is a discipline reviewers must police (no Wave A code may reference
`transcript_hold.rs`); the single-PR/two-wave staging asks delivery to keep the boundary clean rather
than interleaving. Cross-refs: ADR-001 (seam, both waves), ADR-008 (held store + cap-eviction audit),
vnc-025 ADR-004 #4742, crt-036 (gc_audit_log), OQ-3, SR-03, Constraint 13.
