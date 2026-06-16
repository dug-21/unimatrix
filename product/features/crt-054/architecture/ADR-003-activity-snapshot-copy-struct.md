## ADR-003: activity_snapshot() — a Copy Counter Struct Read Surface That Carries bytes_total, Never Transcript Bytes

### Context
crt-055 reads Surface B during the review pipeline. The read must expose the accumulated counters per session without exposing transcript content, and must reuse the proven multi-session collection so multi-session correctness is inherited. The existing content reader is `TranscriptBuffer::snapshot() -> TranscriptSnapshot { bytes, .. }` (`session_transcript.rs:261`, with public `high_water` at `:102`), collected across the registered-union-held set by `take_transcripts_for_feature` (dedup by `Arc` identity). That path is byte-bearing and serves crt-052 distillation on the response-only path. Reusing it for the activity read would drag a content-bearing read onto the consumer's persist sourcing — exactly ass-077's rejected R-A. The activity read must be a separate, bytes-free reader.

This ADR is the producer-only successor of the prior crt-054 ADR-004 (#5002). It is reconciled against the crt-055 producer contract: the returned struct carries `bytes_total` (the honest throughput proxy, **bytes not tokens**), `delta_count`, and `class_counts` — and **no `saw_compaction` / `reload_after_compaction` latch** (compaction is Surface A; the reload reckoning is crt-055's). crt-054 never sums across sessions and never persists — it exposes the per-session snapshot; crt-055 sums across the cycle's held sessions and lands the columns.

### Decision
Add a counters-only read surface mirroring the snapshot seam structurally but returning no bytes.

`TranscriptBuffer::activity_snapshot(&self) -> ActivitySnapshot` returns a `Copy` struct of the counters — `bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; MAX_SIGNAL_CLASSES]` — with **no `Vec<u8>`, no `bytes` field, no `Display`, metadata-only `Debug`**; structurally incapable of carrying a transcript byte. It poison-recovers like `snapshot()` (#4764, treat-poisoned-as-empty).

A registry-level collector mirrors `take_transcripts_for_feature` for the held-union-registered selection: Phase 1 registry-lock scan + `Arc`-clone + the Wave B held-scan branch via the optional `HeldBufferScan` handle, dedup by `Arc` identity; Phase 2 per-buffer-lock calls `activity_snapshot()` only. This is a second collection method, deliberately parallel to `take_transcripts_for_feature` (they read different things and must not merge, lest the activity read accidentally pick up bytes). The byte-bearing `take_transcripts_for_feature`/`snapshot()` is NOT invoked by the activity read path — it stays on the response-only distillation path.

Field widths are fixed by the contract at the producer side (`bytes_total: u64`, `delta_count: u32`) and land in crt-055's `i64` columns; the conversion at the `activity_snapshot()` → consumer boundary uses checked/saturating conversion to preclude silent wraparound (Open Q3 / SR-03). crt-054 owns the producer-side widths; crt-055 owns the conversion-at-persist.

### Consequences
Easier: multi-session correctness inherited from the proven dedup-by-`Arc` collection; the bytes-free reader makes the never-persist envelope structural (ADR-005); Wave B severability preserved (the held-scan branch is optional).

Harder: a second collection method parallel to `take_transcripts_for_feature` (deliberate duplication); both must track future changes to the registered-union-held selection.

Cross-refs: ADR-001 (the counters being read), ADR-004 (late-bind attribution via the `feature_cycle` filter the collector uses), ADR-005 (bytes-free is the envelope), ADR-006 (the read must observe the counter before the crt-052 purge). crt-055 producer contract §"Surface B" (the binding field/type definition this conforms to). Removed vs prior ADR-004: `saw_compaction`/`reload_after_compaction` fields; the in-handler cross-session sum (now crt-055's).
