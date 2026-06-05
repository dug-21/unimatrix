# Agent Report: vnc-025-agent-1-architect

## Artifacts

- `product/features/vnc-025/architecture/ARCHITECTURE.md`
- `product/features/vnc-025/architecture/ADR-001-transcript-field-shape.md` (Unimatrix #4739)
- `product/features/vnc-025/architecture/ADR-002-transcript-buffer-representation.md` (#4740)
- `product/features/vnc-025/architecture/ADR-003-dispatch-tee-before-filter.md` (#4741)
- `product/features/vnc-025/architecture/ADR-004-purge-audit-lifecycle.md` (#4742)
- `product/features/vnc-025/architecture/ADR-005-shared-transcript-block-extraction.md` (#4743)
- `product/features/vnc-025/architecture/ADR-006-buffer-cap-config.md` (#4744)
- `product/features/vnc-025/architecture/ADR-007-session-key-seam.md` (#4745)

## Key Decisions

1. **ADR-001 (SR-01, decided first)**: `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>` —
   `get_state()` clone cost becomes one Arc clone (AC-10 structural). Lock order:
   registry → buffer, never reversed; merge memcpy happens under the buffer lock only, so the
   worst-case 1 MiB frame never blocks the global registry mutex.
2. **ADR-002 (SR-02, AC-02)**: contiguous-span + hole-range representation (order-independent
   merge), ring-tail overflow with metadata-only elision (no spliced markers), holes capped at
   64 ranges, manual metadata-only `Debug`, no `Display`, no content-bearing returns/errors.
3. **ADR-003 (SR-07, SR-08)**: batch arm tees deltas to the merge *before* the vnc-024 filter;
   the filter line is untouched; merge calls reuse the already-sanitized dispatch entry; every
   delta outcome returns `Ack`.
4. **ADR-004 (SR-03, SR-04)**: purge points collect `TranscriptPurgeRecord` under lock, emit
   `transcript_session_purged` after release via `log_event_async`, fire-and-forget; purge
   never depends on audit; `clear_transcripts_for_feature` is the named crt-052 seam
   (counts-only return today, deliberately not bytes); sweep returns purge records for
   silently-evicted sessions too (AC-08 coverage).
5. **ADR-005 (SR-05, A3)**: hook extraction internals move to shared `uds/transcript_block.rs`
   with `from_path`/`from_bytes` entry points — PreCompact parity by construction; golden test
   compares the two on one fixture. F3 delta-content contract (raw JSONL bytes, file offsets)
   becomes load-bearing.
6. **ADR-006**: `transcript_buffer_max_bytes` on `RetentionConfig` (4 MiB default,
   validate ≥ 64 KiB), injected via `SessionRegistry::with_transcript_cap`; `new()` keeps the
   default for tests; SR-06 evidence trigger documented (>32 sessions or >256 MiB resident).
7. **ADR-007**: `session_key(tenant, project, session)` constructor seam in the new module,
   used by new code paths only; no legacy re-key.

## Open Questions (for spec)

1. AC-02 × overflow (A1): convergence under cap-crossing sequences is tail-window
   equivalence, not full-content — spec must phrase the AC-02/AC-07 interaction test that way
   or escalate if full convergence is required.
2. Drain/sweep return shapes are specified as tuples (minimal churn); named structs are an
   acceptable spec substitution — `SignalOutput` (persisted shape) must stay untouched.
3. The 64-range hole-metadata cap is a tunable; the bounded-metadata property is the
   requirement, not the constant.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned #4725 (transport-convergence test
  pattern, applied in ADR-003), #4721 (vnc-024 ADR-005 retention/secrets posture, applied in
  ADR-004/006), #4720 (vnc-024 accept-and-drop ADR being replaced), plus vnc-024 decision set
  via context_lookup and #4737 (SessionState clone-cost pattern, drove ADR-001).
- Stored: entries #4739 "ADR-001 vnc-025: Transcript Rides SessionState as
  Arc<Mutex<TranscriptBuffer>>", #4740 "ADR-002: Contiguous-Span TranscriptBuffer...",
  #4741 "ADR-003: Tee Deltas Before the Untouched Batch Filter...", #4742 "ADR-004: Purge
  Audit Lifecycle...", #4743 "ADR-005: Shared Transcript-Block Extraction Core...",
  #4744 "ADR-006: transcript_buffer_max_bytes on RetentionConfig...", #4745 "ADR-007:
  session_key() Constructor Seam..." via context_store (category decision, topic vnc-025).
- No prior ADRs superseded: vnc-024 ADR-004's accept-and-drop guard is replaced in code, but
  that ADR documents the F1-era decision with an explicit "until #670 wires the consumer"
  horizon — it remains accurate history, not a stale active decision; no deprecation needed.

---

## Continuation: R-02/R-06 Gap Closure (post risk-strategy)

Gap: attacker-controlled `offset: u64` could overflow `offset + bytes.len()` inside
`apply_delta`, panicking under the per-session buffer mutex and poisoning it — bricking
merges and PreCompact for that session. No ADR pinned the arithmetic or poison policy.

Pinned in new **ADR-008** (`architecture/ADR-008-arithmetic-overflow-poison-policy.md`),
chosen over amending ADR-002 because the decision spans two components (buffer arithmetic +
lock-site policy in dispatch/PreCompact/purge) and ADR-002's merge semantics are unchanged:

- **Layer 1**: checked arithmetic throughout `TranscriptBuffer`; a delta whose
  `offset.checked_add(len)` overflows is dropped whole (no partial write, no accounting);
  u64→usize conversions only on span-relative values proven ≤ cap. Contract: no
  wire-reachable input can panic inside the buffer.
- **Layer 2**: every buffer-mutex lock site uses
  `lock().unwrap_or_else(|p| p.into_inner())` + `clear()` on the poison path
  (treat-as-empty — only state with guaranteed invariants, and SR-02-safe). Merges resume
  empty, PreCompact degrades to the empty-buffer path, always-Ack (ADR-003) preserved.

ARCHITECTURE.md updated: ADR-008 row in Technology Decisions; `apply_delta`
Integration Surface line now states the never-panics contract.

## Knowledge Stewardship (continuation)
- Queried: mcp__unimatrix__context_briefing -- returned #734 (server-resilience pattern:
  never panic on lock acquisition in async server code — directly supports the layer-2
  policy), #4740/#4741 (ADR-002/ADR-003, the decisions this hardens). No conflicting prior
  decision; nothing superseded.
- Stored: entry #4746 "ADR-008: Checked Offset Arithmetic (Drop-Whole on Overflow) +
  Treat-as-Empty Poison Recovery, Preserving Always-Ack" via context_store (category
  decision, topic vnc-025).
