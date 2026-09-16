# Test Plan — C8 Parity Corpus (opencode cases)

The col-022 split-brain drift sentinel. **File correction (verified against the tree):** the
BRIEF/ARCHITECTURE name `uds/parity_corpus_uds.rs`, but that file is UDS **framing/byte** goldens
only (`generate_framing`/`generate_hash_goldens`, asserts non-emptiness). The 7-event
provider/normalizer cases live in:
- `crates/unimatrix-server/src/uds/parity_corpus_cases.rs` (canonical-event / provider-alias /
  unknown-passthrough cases; `Case::new(name, event, arms, ...)`; `cases()` :69; arm-key constants
  like `"normalize_event_name::canonical::PostToolUse"`, `"build_request::PostToolUse::non_claude_
  provider"`) and siblings `parity_corpus_cases_tools.rs`, `parity_corpus_cases_b.rs`,
  `parity_corpus_transcripts.rs`.
- `crates/unimatrix-server/src/uds/parity_corpus_gen.rs` — the generator + drift gate:
  `struct Case { name, event, stdin, arms, transcript, response }` (:44); `all_arm_keys()` (:92);
  `assert_coverage(cases)` (:363, called from `generate_parity_corpus` :446 `#[test] #[ignore]`);
  goldens regenerated under `packages/unimatrix/test/fixtures/parity/`.
- Zero-diff gate: `scripts/check-parity-drift.sh` (regenerate → `git diff --exit-code` + non-vacuity
  guards); local regen `scripts/regen-parity.sh`. JS consumes the fixtures in
  `packages/unimatrix/test/hook-client/parity-*.test.js`.

**Delivery must confirm which file receives the opencode cases** (likely `parity_corpus_cases.rs` +
new arm keys). This plan is written against that seam.

Risks owned: **R-03 (Critical, split-brain), R-16 (parity-gap honesty for the 7-event sweep)**. ACs:
AC-02b (parity green, drift-failing). Coupled with C1 + C3 (change together).

## Corpus expectations

### opencode cases across the 7 canonical events (FR-04, AC-02b, R-03)
Add `Case::new(...)` entries covering opencode for each canonical event, declaring the arm keys they
cover (e.g. `normalize_event_name::canonical::<Event>` and `build_request::<Event>::opencode`):
- SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, SubagentStart, PreCompact, Stop.
- Include a **provider** case (`provider="opencode"`), a **model** case (carrying `model_id`, ties to
  c4/#5670), and a **subagent** case (child `session.created`+`parentID`+`agent` on `extra.agent_type`).
- Register any new arm keys in `all_arm_keys()` (gen.rs:92).

### Drift sentinel — the load-bearing R-03 assertion
- `assert_coverage` (gen.rs:363) must FAIL if an opencode arm key is declared without a case, or a
  case references an unknown arm key ("arm keys without a corpus case (R-02)" / "references unknown
  arm key"). Requirement: after adding the opencode arm keys, there is a corresponding case for each
  — and removing/omitting one arm (simulating a one-sided edit) fails the generator test. This is
  the mechanism that catches C2-edited-without-C3 (or vice versa).
- `scripts/check-parity-drift.sh` is green: regenerating from the Rust oracle produces zero diff
  against committed fixtures (so the JS normalizer, which consumes those fixtures, is byte-parity
  with Rust). Requirement: `scripts/regen-parity.sh` is run and the new opencode fixtures committed
  in the same change (per #5302 — single-source the CONTRACT, not just shared data).

### Full contract, not just data (#5302)
Each opencode case asserts the full `(canonical_name, provider, model_id passthrough)` contract, not
merely the event name — so a divergence in provider stamping or model_id handling between arms is
caught, not just a name mismatch.

## R-16 — 7-event sweep honesty (shared with c1)
The corpus proves canonicalization parity; the AC-01 "each reachable/bus-derived event lands as a
stored record; SubagentStart injection is a measured parity gap" is the assembled-path sweep
(infra-001 + c1). c8 ensures the SubagentStart **observation** case is present in the corpus and
that the corpus does not encode an injection leg (which does not exist).

## Cross-references
- C1 plugin builds the frames; C2 (Rust arm) + C3 (JS arm) canonicalize them; C8 is the sentinel
  that they agree. All four land in the same change (C-1 split-brain constraint).
- model_id parity case ties to c4 (wire) and c5 (crossing test) per lesson #5670.
