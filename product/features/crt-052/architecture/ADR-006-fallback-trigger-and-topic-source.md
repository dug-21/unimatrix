## ADR-006: Reconstruction-Fallback Trigger Keyed to Hole/Elision State (Tail-Window-Equivalence), `topic_source` a Soft Preference

### Context
Goal 5 / AC-07 / OQ-2(resolved: whole-session either/or per session): when an attributed session's
buffer is empty or hole-ridden, distillation input is reconstructed from that session's stored
`ObservationRecord`s (tool, input, response_snippet). The buffer is NOT lossless — tail-window
equivalence only (vnc-025 ADR-002 #4740, ADR-008 #4764): full-content equality holds below the 4 MiB
cap; under ring-tail overflow `base_offset` advances and head deltas are clipped (`elided_bytes > 0`).
SR-08: a trigger assuming losslessness mis-fires (over-firing fallback, or missing real loss — cf.
#3359 threshold/window mismatches). vnc-030 shipped `observations.topic_source`
(`declared`/`extracted`/`registry-fill`/`vote`/NULL, #4816); OQ-1 / SR-06 pin it as a **soft recall
preference**, never a hard filter (hardening would drop legitimately-attributed sessions). Registry
selection for the PRIMARY path is already contract-attributed by vnc-030 ADR-007 §2 (#4819) — declared
sessions cannot be vote-flipped — so `topic_source` is relevant only to fallback ordering.

### Decision
**Fallback trigger (whole-session either/or per session v1), defined against ADR-002 buffer semantics:**
a session falls back when its `TranscriptSnapshot` is **empty** (`bytes.is_empty()` after JSONL
filtering yields no user/assistant blocks) **OR** hole/elision loss exceeds a threshold expressed in
those terms — i.e. `elided_bytes > 0` indicating ring-tail clipping, or `holes` covering more than a
configured fraction of the span. The threshold is defined explicitly against tail-window-equivalence,
NOT against an assumed lossless buffer (SR-08), and is tested at the cap boundary and under ring-tail
overflow. The trigger is per-session and whole-session: a session is either primary or reconstructed,
not a byte-level mix (OQ-2).

**Reconstruction (`unimatrix-observe/src/distill/reconstruct.rs`):**
```rust
fn reconstruct_from_observations(
    session_id: &str, obs: &[ObservationRecord], session_cap: usize,
) -> Vec<TranscriptCandidate>
```
builds distillation input from the session's already-loaded observations (tool, input,
response_snippet ≤500 chars), emits candidates with `provenance: Reconstructed` (ADR-007). It is
**distillation-input only**: never writes the byte buffer, never produces observation rows (AC-07).
The reconstruction ceiling is 0.81 (DEC-weakest, 5 of 8 lost — ass-070 Q6); this is a fidelity floor,
made discriminable by provenance labeling, not parity.

**`topic_source` as a SOFT preference (SR-06):** within the set of an attributed session's
observations, reconstruction **orders/prefers** `declared`/`registry-fill` rows ahead of
`vote`/`extracted` rows to reduce mis-scoped input — but candidates remain **feature-match-scoped**;
no observation is dropped for its `topic_source`, and no session is excluded by it. crt-052 does not
persist or re-derive `topic_source` (it reads the already-loaded column only).

### Consequences
Easier: the trigger is correct under overflow because it reads the same `elided_bytes`/`holes`/`base_offset`
the buffer already tracks (no parallel loss accounting); fallback is whole-session so the consumer sees a
clean primary-vs-reconstructed label per session (ADR-007); `topic_source` sharpens recall without a
hard-filter regression (SR-06). Harder: the threshold fraction is a tuning parameter that must be
boundary-tested (cap edge, ring-tail) or it mis-calibrates (SR-08); reconstruction is a genuinely
lower-fidelity path, so the spec must keep its degraded label load-bearing for future quality
measurement; the soft-preference ordering must be implemented as a stable sort key, not a filter, or it
silently becomes SR-06's banned hard filter. Cross-refs: ADR-002 (snapshot metadata read), ADR-007
(provenance), vnc-025 ADR-002 #4740 / ADR-008 #4764, vnc-030 ADR-004 #4816 / ADR-007 §2 #4819, ass-070
Q6, SR-06/SR-08.
