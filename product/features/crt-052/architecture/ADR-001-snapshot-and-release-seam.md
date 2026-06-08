## ADR-001: Snapshot-and-Release Seam — `take_transcripts_for_feature` Returns Owned Raw Snapshots, Sibling to `clear`

### Context
vnc-025 ADR-004 (#4742) named `clear_transcripts_for_feature` (`session.rs:299`) as the single crt-052
insertion point and pre-committed it to becoming "take-shaped later, parsing never under a lock." That
method today returns counts-only `Vec<TranscriptPurgeRecord>` and clears buffers in place. crt-052 must
read buffer **content** before purge, under the same two-phase lock discipline (registry lock → Arc
clone; per-buffer lock → byte copy; ALL parsing strictly after every lock releases — AC-01,
Constraint 1, vnc-025 ADR-001 #4739). The pinned design position (load-bearing) is that the seam
returns OWNED RAW snapshot bytes plus per-session elision/hole metadata — NOT pre-filtered candidates —
because candidate selection (the four marker families) is a SEPARATE consumer, and #700 (MARKER
recovery) must reuse the same single producer with different patterns (see ADR-002). Mutating the
existing counts-only method to also return content would (a) create a content-bearing value flowing
through the purge-only path with no consumer there (the secrets-posture objection vnc-025 ADR-004
raised), and (b) entangle the snapshot lifecycle with the in-place `clear()`.

### Decision
Add a **sibling** method, leaving `clear_transcripts_for_feature` unchanged for the
`session_close`/`stale_sweep` callers that only need counts:

```rust
pub fn take_transcripts_for_feature(
    &self,
    feature_cycle: &str,
) -> Vec<(String, TranscriptSnapshot)>
```

Two-phase, identical discipline to the existing method:
- **Phase 1 (registry lock):** linear scan on `state.feature.as_deref() == Some(feature_cycle)`
  (`None` never matches — declared sessions are contract-attributed by vnc-030 ADR-007 §2, #4819, so a
  declared cycle can no longer be vote-flipped at this point); Arc-clone each matching buffer; collect
  `(session_id, Arc<Mutex<TranscriptBuffer>>)`; **release the registry lock.**
- **Phase 2 (per-buffer lock):** for each Arc, take the buffer lock, call `buf.snapshot()`
  (ADR-002), release. Poison-recovers per vnc-025 ADR-008 (#4764): `lock().unwrap_or_else(|p|
  p.into_inner())`, treat-as-empty + `clear_poison()`.
- **Return:** owned `Vec<(String, TranscriptSnapshot)>`. The buffer is **not** cleared here — purge is
  still the separate `purge_cycle_transcripts` / `clear_transcripts_for_feature` step that fires
  after distillation (ADR-005, AC-05). Snapshot reads; purge clears; they do not merge.

The method does the byte copy (the only content-bearing work) entirely **off** the registry lock; no
parsing, JSONL handling, or marker matching occurs in `session.rs` at all (Constraint 10 — all logic
lives in the `unimatrix-observe` distill module, ADR-003).

This is the take-shaped seam vnc-025 ADR-004 foresaw, realized as an addition rather than a mutation,
keeping diffs minimal against the close/sweep functions vnc-030 left untouched (Constraint 13).

### Consequences
Easier: AC-01 maps structurally onto the two phases; a concurrency test streaming deltas during the
snapshot proves no-parse-under-lock; `clear_transcripts_for_feature`'s existing callers and tests are
untouched (minimal-diff vs vnc-030 ADR-007 §2); snapshot and purge are independently testable.
Harder: there are now two attributed-session scans (snapshot then purge) on the cycle-review path —
acceptable at OSS scale (one-pass linear, no feature→session index), and the purge scan is already
present. Reviewers must police that no caller of the new method clears buffers as a side effect.
Cross-refs: ADR-002 (the `TranscriptSnapshot` shape), ADR-005 (where this is called), ADR-008 (held
buffers this scans under Option B), vnc-025 ADR-001/002/004/008, vnc-030 ADR-007 §2.
