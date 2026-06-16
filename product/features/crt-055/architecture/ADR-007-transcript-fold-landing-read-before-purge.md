## ADR-007: Transcript-Fold Landing — Read-Before-Purge, Sum Across Held Sessions, Checked Width Conversion

### Context
crt-054 produces `ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }` per session as an in-memory running fold (crt-054 ADR-003/#5028), never persisted by the producer. crt-055 must land it into durable columns. Three hazards: (SR-09) the crt-052 Wave-B hold purge (`purge_cycle_transcripts`) zeroes the buffers at review — reading after it silently zeroes the columns; (SR-10) producer widths `u64`/`u32` must convert into `i64` SQLite columns without truncation, and undeclared/held-route-miss sessions must die fail-loud, not fabricate a zero; (SR-08) a held-route miss yields a real zero indistinguishable from "no activity". The contract fixes the consumer columns (SCOPE §"Consumer persistence").

### Decision
In the review pipeline, BEFORE `purge_cycle_transcripts`, call the producer's activity collector `activity_snapshots_for_feature(feature_cycle)` (registered ∪ held, dedup-by-`Arc`, filtered by `feature_cycle` — crt-054 ADR-004). Sum across the cycle's sessions and land:
- `transcript_bytes_total` ← Σ `bytes_total`, `transcript_delta_count` ← Σ `delta_count`.
- `transcript_error_count` ← Σ `class_counts[0]`, `transcript_refusal_count` ← Σ `class_counts[1]` (fixed indices per ADR-008).
- `signal_class_counts_json` (TEXT DEFAULT `'{}'`) ← the full `class_name → summed count` map, forward-compatible for classes added beyond error/refusal.

Use checked/saturating conversion at the persist boundary (`u64`/`u32` → `i64`); saturate-and-warn rather than panic on the (practically impossible) overflow. Pin the collector call site strictly ahead of `purge_cycle_transcripts` and assert the ordering in test (SR-09). Drive each transcript metric's presence flag (ADR-003) off whether the fold was non-empty for at least one declared session; an undeclared-only cycle surfaces "unavailable", never `0` (SR-08, SR-10). The fold is content-free (integers only) — the structural leak gate stays intact; no transcript bytes touch the persist path.

### Consequences
Easier: the honest throughput proxy (bytes) and behavioral-signature counts become durable and cross-cycle-comparable; read-before-purge ordering is explicit and test-asserted; `signal_class_counts_json` future-proofs the catalog without another migration. Harder: the read-ordering is a silent-zero trap if a refactor moves the purge earlier (mitigated by the ordering assertion); the sum must handle the per-session presence correctly so one undeclared session does not zero a cycle with other valid sessions. Cross-refs: crt-054 ADR-003 (#5028 ActivitySnapshot), ADR-004 (#5029 late-bind attribution), ADR-006 (#5031 survival-to-review), Constraint 6 (read-before-purge), SR-08/09/10, ADR-002 (single writer), ADR-003 (presence flags), ADR-008 (class indices).
