## ADR-003: Candidate Selection Is a Pure Module in `unimatrix-observe/src/distill/`, Untrusted-Input-Hardened

### Context
OQ-5 resolved to "lean `unimatrix-observe`" — selection sits beside the pure, no-I/O synthesis
functions (`synthesize_narratives` `synthesis.rs:15`, `build_phase_narrative` `phase_narrative.rs:21`).
ass-070 decided the architecture: server rules SELECT whole marker-matched user/assistant blocks; the
agent EXTRACTS all semantics. The server has no generation capability (`unimatrix-embed` is ONNX-only —
Non-Goal, Constraint 6), so no server-side classification beyond advisory family hints. Buffer content
is **untrusted** client-disk JSONL (Constraint 7, SR-09): a corrupt or adversarial line must degrade to
skip-with-count, never panic the cycle-review handler. The 500-line rule (Constraint 10, #693) forbids
growing `tools.rs`/`session.rs`/`listener.rs`; this is new focused-module logic.

### Decision
New module tree `unimatrix-observe/src/distill/` — pure, no I/O, no locks, no `tracing`, unit-testable
against committed fixtures:

- `jsonl.rs` — parse Claude Code JSONL line-by-line; keep only `user`/`assistant` **text** blocks; drop
  `tool_use`/`tool_result`/`thinking`/command-noise. Every unparseable/unknown line increments a
  skip-count and is dropped — **never** returns `Err`, never panics (Constraint 7). The parser operates
  on `&[u8]` and tolerates a truncated final line (ring-tail/hole boundary).
- `markers.rs` — the four marker families (decision phrases, rework signals, lesson markers, phase/gate
  markers — ~50 patterns ported from ass-070's `extractor.py`), built once as a `regex`-class set
  (`OnceLock`); no heavyweight runtime dependency (AC-13). Matching yields advisory `FamilyHint`s only.
- `select.rs` — the entry point:
  ```rust
  fn select_candidates(
      bytes: &[u8], session_id: &str, base_offset: u64, session_cap: usize,
  ) -> Vec<TranscriptCandidate>
  ```
  pipeline: parse → keep user/assistant text blocks → match four families → **keep matched blocks
  whole** (no windowing — ass-070 ablation: windowing loses multi-paragraph context) → dedup → enforce
  per-session volume cap (`session_cap`, default 24 KB) → order chronologically with `session_id`,
  `byte_offset` (logical, derived from `base_offset` + in-snapshot offset), `ts`, `family_hints`.
- `mod.rs` — re-exports; the per-cycle aggregate cap is enforced by the handler glue (ADR-005) across
  the union of all sessions' candidates, not inside the per-session call.

No `Result` carries content; the only output is `Vec<TranscriptCandidate>`. The module is reachable from
the cycle-review handler only via ADR-005's helper; #700 (ADR-002) will add a sibling `markers`-style
pass in this same tree, reinforcing the single-reader split.

### Consequences
Easier: AC-02/AC-03 are unit-tested against the committed independent fixture corpus with zero server
scaffolding; the parser's skip-with-count is a direct AC and fuzz/malformed-line target (SR-09); the
<10 ms rule-pass estimate (ass-070) holds since it is pure Rust over in-memory bytes; the 500-line rule
is honored — `tools.rs`/`session.rs` get only thin wiring. Harder: the ported regex set must be
authored against fixtures written **independently** of it (AC-03 / OQ-6) or recall is self-fulfilling —
a test-authoring obligation the spec must gate; the `Vec<FamilyHint>`-per-candidate shape must stay
advisory (server never classifies — Non-Goal) or it leaks a semantic contract the agent is meant to own.
Cross-refs: ADR-002 (input bytes + base_offset), ADR-005 (caller + aggregate cap), ADR-006
(reconstruction sibling), ass-070 Q2/Q4/Q6, Constraints 6/7/10.
