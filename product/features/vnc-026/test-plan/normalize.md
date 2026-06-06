# Test Plan: normalize.js (event canonicalization)

Oracle: `hook.rs:50-105` (`map_to_canonical` / `normalize_event_name`). Risk: R-01 (via corpus).
Suite: `test/hook-client/normalize.test.js` — pure string-map unit tests; authoritative coverage
comes from Layer 1 parity cases (parity-corpus.md), these units give fast-fail locality.

## Unit Tests

### Canonical mapping
- `test_canonical_events_identity` — each of the 13 canonical event names maps to itself.
- `test_gemini_aliases` — `BeforeTool` → `PreToolUse`; `AfterTool` → `PostToolUse`; `SessionEnd` → `Stop`. Provider inference recorded per pseudocode (Gemini aliases set `provider`).
- `test_unknown_event_sentinel` — unrecognized name → `__unknown__` sentinel with raw name preserved for the generic-observation passthrough (AC-01 "unknown-event passthrough": raw name must survive into the built request — joint assertion with build-request.md).

### Defensive behavior
- `test_empty_event_name` — `""` → sentinel path, no throw.
- `test_case_sensitivity_parity` — casing handled exactly as Rust (e.g., `pretooluse` is NOT canonical unless hook.rs lowercases — golden-driven, not assumed).
- `test_whitespace_name` — `" PreToolUse "` handled exactly as Rust (golden-driven).

## Parity Tie-In (R-01)

Corpus cases exercising this module (manifest IDs from parity-corpus.md):
- one case per canonical event (13);
- one per Gemini alias (3);
- unknown-event passthrough (1);
- empty/whitespace name defensive cases.

Unit expected values for alias/sentinel behavior are cross-checked against the corpus goldens —
if a unit expectation disagrees with a golden, the golden wins (Rust is the oracle).

## Concrete Assertions

- `normalizeEventName(raw) -> {canonical, provider?}` is pure: same input twice → deep-equal output, no I/O (fs spy clean).
- Map is closed: function never returns a name outside {13 canonical} ∪ {`__unknown__`}.
