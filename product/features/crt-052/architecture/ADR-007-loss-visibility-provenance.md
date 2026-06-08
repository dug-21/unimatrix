## ADR-007: Loss Visibility and Degraded Provenance Are Mandatory in the Candidates Section

### Context
Goal 6 / AC-08 / Constraint 8: transcript loss must never be silent. ass-070 Q5 found anchors uniformly
distributed, so elision (which advances `base_offset` and clips the head) loses **early** decisions
proportionally — exactly the highest-value DEC family. Two distinct loss modes exist: (a) byte loss
within an otherwise-primary session (`elided_bytes > 0`, holes present — vnc-025 ADR-002 #4740), and
(b) whole-session reconstruction (ADR-006), a 0.81-ceiling fidelity floor. A consumer (the extracting
agent, and future quality measurement) must be able to discriminate primary-from-reconstructed and see
per-session byte loss, or it will over-trust degraded output.

### Decision
The `transcript_candidates` section carries per-session loss and provenance, surfaced whenever
non-zero/active:

```rust
pub enum CandidateProvenance { Primary, Reconstructed }

pub struct SessionLossInfo {
    pub session_id: String,
    pub elided_bytes: u64,    // from TranscriptSnapshot.elided_bytes (ADR-002)
    pub has_holes: bool,      // from TranscriptSnapshot.holes (ADR-002)
    pub provenance: CandidateProvenance,
}

pub struct TranscriptCandidatesSection {
    pub candidates: Vec<TranscriptCandidate>,
    pub loss: Vec<SessionLossInfo>,
}
```

- Every session that contributes candidates appears in `loss` whenever it has non-zero `elided_bytes`,
  holes, OR reconstructed provenance (AC-08). A clean primary session with no loss may be omitted from
  `loss` (silence only means "no loss to report").
- `provenance` is per-session (consistent with ADR-006's whole-session either/or trigger): a session is
  `Primary` or `Reconstructed`, never mixed.
- The loss metadata is derived entirely from the `TranscriptSnapshot` fields (ADR-002) and the
  fallback decision (ADR-006) — no new buffer read.

The section is additive and absent when empty (ADR-004 / AC-04); `loss` is part of it, so loss
visibility rides the same response-transient, never-persisted path (AC-06).

### Consequences
Easier: AC-08 maps directly onto `SessionLossInfo`; the consumer guidance (ADR-009 / AC-13) can instruct
the agent to weight `Reconstructed` candidates differently and to note when early-decision elision is
likely; future quality measurement can partition recall by provenance. Harder: the section now has two
parallel collections (`candidates` + `loss`) the assembly step must keep consistent (a reconstructed
session with zero candidates still warrants a `loss` row so the loss is not invisible); the
`Primary`/`Reconstructed` label is load-bearing for downstream trust, so its derivation must be the same
predicate ADR-006 uses, not a re-computation. Cross-refs: ADR-002 (metadata source), ADR-004 (transient
section), ADR-006 (provenance predicate), ass-070 Q5/Q6, Constraint 8.
