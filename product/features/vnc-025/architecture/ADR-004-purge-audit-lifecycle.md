## ADR-004: Purge Points Collect Metadata Under Lock, Emit Content-Free Audit After Release; `clear_transcripts_for_feature` Is the Named crt-052 Seam

### Context

All three purge points run under the registry mutex with a strict no-I/O discipline
(Constraint 3), but `transcript_session_purged` is a SQL write (SR-03 — awaiting under the
lock or losing the audit silently are both failure modes; #735 records spawn_blocking
saturation from fire-and-forget misuse, GH #302 records write-pool starvation from blocking
bridges in async contexts). Separately, crt-052 will insert distill-before-purge at cycle
review; if the clear method is shaped wrong, that retrofit becomes a rewrite (SR-04, #3158).
Sweep has a subtlety: sessions with empty `injection_history` are silently evicted with no
`SweepResult` (`session.rs:522`), but AC-08 requires the purge audit whenever the purged
buffer was non-empty.

### Decision

**Metadata under lock, audit after.** Each purge point computes
`TranscriptPurgeRecord { session_id: String, bytes_purged: u64 }` while it already holds the
relevant locks (reading `buffer.len()` via the Arc it owns), releases everything, then emits
audit. Records with `bytes_purged == 0` emit nothing.

Interfaces:

- `drain_and_signal_session(...) -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>` —
  `SignalOutput` itself is untouched (it feeds the persisted signal queue; no shape churn).
- `sweep_stale_sessions(&self) -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>)` — the purge
  vec covers **every** evicted session with a non-empty buffer, including silently-evicted
  ones, or AC-08 has a hole.
- `clear_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord>` —
  single registry lock; for each session with `state.feature.as_deref() == Some(feature_cycle)`,
  clone the Arc; release the registry lock; then per-buffer `clear()` (in-place, session stays
  registered). There is no feature→sessions index; the one-pass scan over a handful of
  sessions is fine at OSS scale.

**Caller-side emission.** The cycle-review handler (`tools.rs:1918`, async, has
`self.session_registry` and the audit log in scope) gates the call on
`match cfg.retention.transcript_retention { TranscriptRetention::PurgeOnCycleClose => ... }` —
the match on the enum is the enterprise seam (Constraint 7), never an assumed variant. It
emits via `AuditLog::log_event_async` (GH #302: no blocking bridge in async contexts),
fire-and-forget (`tokio::spawn`, error → `tracing::warn!` with byte count only). The
SessionClose/sweep path in `handle_session_close` (`listener.rs:1768`) gets the existing
`Arc<AuditLog>` threaded in (already a dispatch parameter at `listener.rs:265/319/385`) and
emits the same way. **Purge success never depends on audit success** — the audit write happens
strictly after the purge is complete.

**Event shape** (mirrors the content-free `uds_auth_failure` precedent, `listener.rs:409`):
`operation: "transcript_session_purged"`, `agent_id: "server"`, `session_id: <purged id>`,
`outcome: Success`, `target_ids: []`,
`detail: "bytes=<n> trigger=<session_close|stale_sweep|cycle_review>"`. Never content. Rows
are GC'd by the existing `gc_audit_log` retention (crt-036) — no new retention machinery.

**crt-052 seam, named.** `clear_transcripts_for_feature` is the single insertion point for
distill-before-purge: crt-052 changes it to snapshot bytes out before clearing (take-shaped),
under the existing rule that parsing never happens under a lock. It returns counts-only today
*deliberately* — returning bytes now would create a content-bearing value flowing through
cycle review with no consumer, violating the secrets posture for nothing (SR-02 over SR-04's
literal suggestion; the seam is the method, not the payload).

### Consequences

- Easier: SR-03 is closed structurally; AC-08/AC-09 map one-to-one onto returned records;
  crt-052 modifies one method body and its caller, nothing else.
- Harder: two existing registry signatures change (drain, sweep) — call sites in
  `listener.rs:1796/:1814` and tests must be updated; the silently-evicted-session audit case
  needs an explicit test or it will regress unnoticed.
- Cross-references: ADR-001 (Arc handles make metadata reads cheap), ADR-006 (retention gate
  config), vnc-024 ADR-005 / #4721 (what `transcript_retention` governs).
