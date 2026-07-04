# Test Plan — Backstop Reclaim (sole reclamation path)

**File:** `unimatrix-server/src/infra/transcript_hold.rs:308` (`sweep_expired`), cap-eviction, session-close
**Risks:** R-06 (High), R-03 (Critical), R-15, R-16 · **ACs:** AC-13, AC-14 (+ NFR-8)

> With the eager review-purge gone, the 24h TTL sweep, 64-cap eviction, and per-turn session-close carry the
> **full** reclamation load. A broken backstop means unbounded residency (secrets) with **no visible
> failure** — the highest-leak-suspect path is reclamation-WITHOUT-review. The backstops themselves are
> UNCHANGED (NG-2); these tests prove they still reclaim correctly and emit content-free audits now that they
> are the sole path. The C-5 exhaustive-match obligation that relocated here is asserted in `orphan-deletion.md`.

---

## R-06 — each backstop reclaims a never-reviewed cycle (AC-13)
Three sub-cases; each reclaims a cycle whose buffer was NEVER retrieved:
- `test_ttl_sweep_reclaims_never_reviewed_cycle` — 24h TTL `sweep_expired` reclaims the buffer AND emits a
  content-free terminal audit (`trigger=stale_sweep`, bytes/session-id only). Assert buffer gone
  (synchronous) AND audit byte-free.
- `test_cap_eviction_reclaims_never_reviewed_cycle` — 64-cap eviction reclaims + content-free audit.
- `test_session_close_reclaims_never_reviewed_cycle` — per-turn session-close reclaims + content-free audit.
- Positive event (reclamation firing) MAY poll for the audit's appearance (#4879 asymmetry — the positive
  side polls; the negative "no purge on review" side never does).

## R-03 — no-new-persistence on the reclamation-without-review path (AC-14, Critical)
- `test_reclamation_audit_content_free` — the backstop audit carries counters + session id only, **zero
  content** — the audit now fires ONLY at the backstop (SR-02). Content-scan the audit row: no 64+ hex run,
  no verbatim delta text (#5089 shape).
- `test_reclamation_path_writes_no_candidate_bytes` — for the cap/TTL/session-close reclamation-without-review
  path, scan every SQL row / file / log line written and assert none contains buffer or candidate byte
  content. (R-03 sc.2 — the least-tested, highest-leak-suspect path.)

## R-15 — long-merge-window fidelity (NFR-8; loss must be VISIBLE, never silent)
- `test_aged_buffer_degradation_is_visible` — age/evict a subset of dev-phase buffers, then run the
  post-merge retrieval. Assert aged sessions are `Reconstructed`/empty **with per-session loss surfaced**
  (`elided_bytes`/`has_holes`/`dropped_candidates`), NOT missing without trace, NOT a crash. **Silence
  (candidates absent with no loss signal) is a FAILING outcome.** (Ties R-01.)
- `test_fresh_vs_aged_partition` — one cycle with fresh + aged buffers → fresh yield `Primary`, aged yield
  `Reconstructed`/empty-with-loss; the response distinguishes them.
- `test_open_cycle_does_not_extend_buffer_life` — buffers governed solely by cap+TTL, independent of cycle
  open/closed state; holding the cycle open does NOT extend buffer life. (Residency envelope bounded.)

## R-16 — post-reclamation retrieval (no crash, no stale verbatim)
- `test_post_reclamation_retrieval_empty_or_reconstructed` — simulate cap-eviction / TTL sweep, then
  retrieve → empty/`Reconstructed`, no panic, no verbatim.
- `test_no_stale_bytes_after_reclamation` — a post-reclamation retrieval's candidates do NOT equal the
  pre-reclamation verbatim (guards re-emitting already-reclaimed, possibly secret-bearing content).

**Coverage requirement:** each backstop reclaims a never-reviewed cycle with a content-free audit; aged
degradation is surfaced (`Reconstructed`+loss), never silent, never a crash; no stale verbatim.
