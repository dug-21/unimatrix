## ADR-001: Activity Fold Lives Inside TranscriptBuffer, Folded at apply_delta on Both Routes

### Context
crt-054 (producer-only, re-scoped 2026-06-16) must produce one of its two surfaces — Surface B, the in-memory throughput/signature fold exposed as `activity_snapshot()` — from the streamed transcript without persisting content. The fold accumulates: `bytes_total` (u64), `delta_count` (u32), and `class_counts` (`[u32; MAX_SIGNAL_CLASSES]`, v1 indices `0=error, 1=refusal`). It must count on BOTH delta routes:
- the **registered** route — `apply_delta` reached through the live session (`session_transcript.rs:150`);
- the **held** route — drained sessions whose buffers ride crt-052 Wave B's hold (`infra/session.rs:388-401`, the `HeldBufferScan` branch).

A fold living only on the registered route misses held-route bytes — the believable-zero trap (#750, lessons #4998/#5025; SR-02). The crt-055 producer contract (binding) fixes `apply_delta` as the merge boundary and mandates both routes. Three seams were evaluated in ass-078 RQ-5; `apply_delta` is correct because the delta bytes are already borrowed under the buffer lock for the merge memcpy. `TranscriptBuffer` is content-opaque by construction (vnc-025 ADR-002, #4740): no `Display`, metadata-only `Debug`.

This ADR is the producer-only successor of the prior crt-054 ADR-001 (#4999). The prior version also carried a **reload latch** (compaction + re-read detection) inside the accumulator; that is removed — there is no in-stream compaction marker, and compaction is now produced exclusively by Surface A (the durable `compaction_events` table, ADR-007). The fold is throughput + behavioral signatures only.

### Decision
Fold at `apply_delta` (`session_transcript.rs:150`); co-locate the scalar accumulator INSIDE `TranscriptBuffer` as a single embedded counter field, with scan logic in a sibling module (e.g. `infra/transcript_activity.rs`) because the buffer module is at the 500-line cap. The accumulator holds only `u64`/`u32` scalars plus the fixed `[u32; MAX_SIGNAL_CLASSES]` class-count array — a `derive(Debug, Clone, Copy)`-safe metadata struct; never a `Vec<u8>`, `String`, or `&[u8]` content field.

`apply_delta` gains one fold call after the merge: `self.activity.fold(bytes, &self.scanner)` — O(bytes), allocation-free, under the already-held buffer lock. Because the accumulator lives inside the buffer, the single `Arc<Mutex<TranscriptBuffer>>` carries both bytes and counters through every lifecycle path with zero new wiring; the held route folds into the SAME accumulator automatically (the decisive reason to co-locate, the SR-02 mitigation). `Debug` is extended to print the new scalar counts, never bytes.

The fold runs on every non-zero-length delta. The zero-length-delta and end-overflow early-returns in `apply_delta` (`session_transcript.rs:156-168`) are bytes-that-never-entered-the-span; the fold call is placed after the merge so it counts only bytes that actually merged, consistent with `high_water` semantics.

v1 fold scope is frozen (SR-06): `bytes_total`, `delta_count`, `class_counts[0..2]` (error, refusal). Deferred signals (turn-size percentiles, thrash/rolling-hash, entropy, language detection — ass-078) need measured evidence and are out of scope.

### Consequences
Easier: held-route coverage correct by construction; no new lock, lifecycle, or content field; inherits the buffer's content-opacity; the crt-052 hold's bounded memory bounds the accumulator too.

Harder: `TranscriptBuffer::new` gains a scanner param threaded through every construction site (like `max_bytes`); content-opacity `Debug` discipline extends to the new fields; one new sibling module file.

Cross-refs: ADR-002 (the RegexSet scanner injected here), ADR-003 (the `activity_snapshot()` read surface this feeds), ADR-005 (never-persist envelope), ADR-006 (survival-to-review), ADR-007 (compaction is Surface A, not an in-stream fold class). vnc-025 ADR-002 #4740 (content opacity inherited).
