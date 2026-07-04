# Test Plan — `TranscriptScope` (scoped filter block) `[NEW]`

**Type:** `struct TranscriptScope { phase?, anchor?, r#match?, window? }` (all optional, AND-composed)
**Risks:** R-09 (High) · **ACs:** AC-02, AC-05, AC-07 (bounds), AC-18 (phase-ignores-window)

> AND-composition **narrows** (intersection, not union). Under-selection is a silent miss — the exact class
> of failure the redesign exists to prevent — so the composition tests assert the strict subset.

---

## R-09 — scoped-filter correctness (AC-02, AC-05)
- `test_omit_transcript_is_summary_only_non_destructive` — no `transcript` field → NO candidates section,
  buffer intact. (AC-01 support; R-09 sc.1.)
- `test_transcript_empty_equals_match_dot_star` — `transcript:{}` (present, all-None) and
  `transcript:{match:".*"}` return the **same** full candidate set under the existing per-cycle cap,
  non-destructively. A second identical call returns the same set (buffer survived). (AC-05; R-09 sc.2.)
- `test_and_composition_narrows_to_intersection` — `phase` ∧ `match` returns candidates in the phase window
  **AND** matching the regex — a strict subset of either alone. Assert the intersection, not a union.
  (AC-02; R-09 sc.3.)
- `test_window_modifies_anchor_and_match_ignored_by_phase` — `phase` is self-bounding (a supplied `window`
  has NO effect); `anchor`/`match` honor `window`. (AC-18 mechanism; R-09 sc.4.)
- `test_empty_scope_result_absent_not_null` — a scope yielding nothing → candidates section **absent (not
  present-but-null)**, no crash. (AC-02; R-09 sc.5.)
- `test_invalid_match_regex_returns_error_invalid_params` — a malformed `match` regex →
  `ERROR_INVALID_PARAMS`, NOT a panic. Flag the ReDoS surface (caller-controlled regex over large candidate
  blocks) to the delivery leader — a compile-time complexity bound or size guard. (AC-02 error path; R-09 sc.6.)

## Serde surface
- `test_match_serde_rename` — the `match` Rust keyword deserializes from JSON key `"match"`
  (`r#match` / `#[serde(rename = "match")]`). (SPEC OQ-3.)
- `test_transcript_serde_default_omit_is_none` — `#[serde(default)]`: an omitted `transcript` deserializes to
  `None` (backward-compatible, lean default). Present-but-empty `{}` deserializes to `Some(all-None)`.

## Anchor/phase bounds (feeds `distill-before-purge.md` clock tests)
- `test_anchor_resolves_evidence_ts_span` — `anchor:<finding id>` resolves to `HotspotFinding.evidence[].ts`
  span `[min, max]`; selects `[min − window, max + window]`. (AC-07 bounds.)
- `test_phase_resolves_cycle_events_bounds` — `phase:<id>` resolves `[phase_start, phase_end]` from
  `cycle_events` (`event_type == "cycle_phase_end"`); selects `candidate.ts ∈ [phase_start, phase_end]`.

## Integration anchor
`suites/test_tools.py::test_cycle_review_transcript_empty_returns_candidates` and
`::test_cycle_review_invalid_match_regex_invalid_params` (OVERVIEW §6c).
