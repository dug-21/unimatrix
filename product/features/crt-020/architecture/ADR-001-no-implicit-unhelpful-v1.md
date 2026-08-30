## ADR-001: No Implicit Unhelpful Votes in v1

### Context

crt-020 derives implicit helpfulness votes by joining `injection_log` with resolved `sessions`
outcomes after each session closes. The original design brief proposed a half-weight implicit
unhelpful signal for rework and abandoned sessions: when a session with a negative outcome closed,
entries that were injected during that session would accumulate a fractional `unhelpful_count`
increment.

The argument for implicit unhelpful votes was symmetry: if a successful session is evidence that
injected entries were helpful, a failed session should be evidence that some injected entry was
unhelpful.

That argument has a critical structural flaw: **session failure cannot be reliably attributed to
any individual injected entry.**

A session may fail (rework, abandoned, or TimedOut) for reasons entirely unrelated to context
injection:
- The task itself was underspecified by the human
- The agent hit a tool failure or external dependency error
- The session was abandoned because the user changed their mind
- A later agent in the same feature cycle introduced a bug — not the one that received context
- The injected entries were accurate and relevant; the session failed anyway

In all these cases, penalizing the injected entries would be incorrect. The signal-to-noise ratio
for "session failed AND this entry was injected" is low enough that the signal may be net harmful:
entries that are injected frequently into complex, high-variance tasks would systematically
accumulate unhelpful votes, lowering their confidence scores precisely because they are used in
the hardest sessions.

The prior design (ADR-001: Pair Accumulation Counter Location) addressed the fractional weight
problem (0.5 vote per rework session via a pair accumulation counter), but it did not address
the attribution reliability problem. Even at 0.5 weight, applying unhelpful signal to every
injected entry in a failed session degrades confidence accuracy when the failure was not caused
by the injected content.

Three implementation approaches for implicit unhelpful were evaluated before this decision:

**Option A — Pair accumulation counter (prior design)**: `implicit_unhelpful_pending` table,
read-modify-write per entry, fire one `unhelpful_count += 1` when counter hits 2.
- Con: Requires a dedicated table and atomic read-modify-write logic. The fractional weight
  partially mitigates the attribution noise, but does not eliminate it.

**Option B — Probabilistic application**: Apply one unhelpful vote with 50% probability per entry
per rework session. Unbiased in expectation but noisy per-entry.
- Con: Same attribution reliability problem. Noise makes per-entry signal meaningless at low
  injection counts.

**Option C — Defer to a future feature with better attribution**: Apply zero unhelpful signal in
v1. Design a v2 mechanism that establishes causal attribution before penalizing an entry (e.g.,
by correlating the specific content injected with the nature of the failure, or by requiring an
agent to explicitly flag an entry as unhelpful after a rework).
- Pro: No false negatives on correct, frequently-used entries.
- Pro: Eliminates the `implicit_unhelpful_pending` table, simplifying schema and implementation.
- Pro: v1 ships useful signal (helpful votes from success sessions) without shipping harmful signal.

### Decision

**No implicit unhelpful votes in v1.** Sessions with outcome `"rework"`, `"abandoned"`, or status
`TimedOut` produce zero signal. `unhelpful_count` is not modified by crt-020 under any
circumstances.

The background tick processes all sessions with `implicit_votes_applied = 0 AND status = 1
(Completed) AND outcome IS NOT NULL`. For sessions with `outcome = "success"`, it applies one
`helpful_count` increment per distinct injected entry. For all other outcomes, it marks the session
`implicit_votes_applied = 1` and moves on — no vote writes occur.

This eliminates the need for:
- The `implicit_unhelpful_pending` table (and the v13 DDL that created it)
- The `increment_pending_and_drain_ready` store method
- The `gc_pending_counters` maintenance step
- Any branching in `apply_implicit_votes` on rework vs abandoned outcome

The v13 schema migration is still required for `implicit_votes_applied` on the `sessions` table,
but it is now a single `ALTER TABLE` plus one index — no new table.

Implicit unhelpful voting is deferred to a future feature that can establish reliable causal
attribution between injected entry content and session failure.

### Consequences

**Easier**:
- `apply_implicit_votes` is simpler: one `record_usage_with_confidence` call per tick (helpful
  only), no second call for unhelpful. No pair accumulation read-modify-write.
- Schema migration v12 → v13 is minimal: one column, one index.
- Confidence scores for high-traffic entries are not degraded by session failures unrelated
  to the injected content.
- Testing is simpler: one vote path to cover (helpful), not two.

**Harder**:
- The confidence formula remains asymmetric in the implicit signal domain: `helpful_count` grows
  from implicit votes, `unhelpful_count` does not. Entries that are consistently injected into
  failed sessions will not be penalized. This is an accepted limitation of v1.
- Future implicit unhelpful voting requires a new feature with attribution design, not a simple
  extension of crt-020. Complexity is deferred, not eliminated.
