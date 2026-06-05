# Agent Report — vnc-024-agent-2-spec

## Task
Apply two FINAL design-review decisions to `SPECIFICATION.md` to match the updated ADRs (no relitigation):
1. Typed `transcript_delta` contract via `TranscriptDeltaPayload` (sixth codegen binding), dual-sided round-trip.
2. `RetainDays` rejected in the OSS build; `PurgeOnCycleClose` only.

## What Changed

### Change 1 — typed TranscriptDeltaPayload, dual-sided round-trip
- **Domain model / Wire types table**: five → six codegen targets; added `TranscriptDeltaPayload { offset: u64, bytes: String }` row, noting carrier unchanged (rides `ImplantEvent.payload: serde_json::Value`, no new wire variant).
- **Domain model / transcript_delta section**: added paragraph defining the typed shape, its dual role (cross-language contract + guard deserialization shape), carrier unchanged.
- **FR-01 / FR-02**: "five wire types" → "six exported types" including `TranscriptDeltaPayload`.
- **FR-11**: rewritten — was Rust-side round-trip + carrier-accepts-arbitrary-payload; now typed contract verified by a **dual-sided (Rust↔TS)** cross-language fixture through the `node --test` harness, parsing into `TranscriptDeltaPayload` on both sides. Closes the F2 hand-mirror drift gap.
- **AC-11**: rewritten to assert the dual-sided round-trip (parses into `TranscriptDeltaPayload` on both Rust and TS sides, like AC-06) + binding-emission check.
- **AC-12 (GATE)**: added note that the guard parses the payload into the typed `TranscriptDeltaPayload` (shared shape with the contract). **Zero-durable-rows gate prerequisite UNCHANGED.**
- **AC-01 / AC-02**: "five types" → "six exported types".
- **Traceability SR-04 / SR-06**: updated to credit the typed binding + dual-sided round-trip closing the hand-mirror gap.

### Change 2 — RetainDays rejected in OSS build
- **Domain / ubiquitous language (transcript_retention)**: added scope definition — governs the **raw session transcript (ephemeral working state)**, explicitly NOT distilled knowledge, observations, or audit log. Updated enum comments + paragraph: OSS honors only `PurgeOnCycleClose`; `validate()` rejects `RetainDays` with an enterprise-only error; enum shape retained as the enterprise seam.
- **FR-13**: OSS `validate()` rejects `RetainDays(_)` with an enterprise-only error; OSS honors only `PurgeOnCycleClose` (replaced the prior range-check framing).
- **AC-13**: now asserts `validate()` REJECTS `RetainDays` with an enterprise-only error and ACCEPTS `PurgeOnCycleClose`; enum-shape requirement (enterprise seam) kept.
- **Constraint 6**: added the OSS-rejection clarification.
- **Constraint 9 (new)**: NO content secret-scanner exists to reuse; the architectural control (accept-and-drop + in-memory-ephemeral + purge) is the guarantee; no requirement may assume a secret-redactor licenses persisting raw transcript.

### R-10 open question
The SPECIFICATION has no open-questions section (R-10 is tracked in the architecture artifacts / ADR-005, which already downgrades it: only live OSS value is `"PurgeOnCycleClose"`; the tagged `RetainDays` TOML form is an enterprise concern). No spec edit required; flagged for the architect to confirm the ADR-side downgrade stands.

## Confirmations
- **AC-11 is now dual-sided (Rust↔TS).**
- **AC-13 asserts the OSS rejection of `RetainDays` (and acceptance of `PurgeOnCycleClose`).**
- All other FRs/NFRs/ACs and the traceability table left intact; SR-01..SR-09 traceability still accurate; constraints renumbered cleanly (new constraint appended as #9).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-005 (#4721) and ADR-004 (#4720) for vnc-024, confirming the OSS-rejects-RetainDays and typed-payload decisions; consistent with the updated ADR files. Read-only tier; no storage.
