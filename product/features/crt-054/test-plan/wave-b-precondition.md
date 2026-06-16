# Test Plan — Wave B startup precondition assert

**Component**: a fail-loud startup assert that the crt-052 Wave B `HeldBufferScan` handle is wired (Surface B durability-to-review rests on it). Placed in `main.rs` next to `RetentionConfig::validate()` / `[transcript_signals]` validate, on BOTH server-construction paths (~700 and ~1236). Guards Surface B only — Surface A does not depend on Wave B.
**Pseudocode**: `pseudocode/wave-b-precondition.md` · **Layer**: unit/integration (startup).
**Anchor ACs**: AC-07 (survival dependency — supporting), AC-13 (no single-event). **Risks**: R-02 (supporting, NFR-7), ADR-010.

## Startup precondition — unit/integration

`crates/unimatrix-server/src/main.rs` tests or the server-construction test path.

1. `test_wave_b_handle_wired_passes_startup` — Arrange: a normally-configured server (Wave B ON, `transcript_hold_max_sessions > 0`). Act: run the startup precondition assert. Assert: it passes (the `HeldBufferScan` handle is present). (ADR-010.)
2. `test_wave_b_handle_unwired_fails_loud` — Arrange: a construction path where the `HeldBufferScan` handle is unwired (simulate the regression). Act: startup. Assert: it **fails LOUD at startup** (panic/early-exit with a clear message) — NOT a silent degrade. A regression that left Surface B with no hold to ride must be caught here, not discovered as a believable-zero at review. (NFR-7, R-02, ADR-010.)
3. `test_precondition_on_both_construction_paths` — assert the precondition runs on BOTH server-construction paths (~700 and ~1236), not just one — a single-path assert leaves the other path silently unguarded. (ADR-010.)

### Negative-mutation
- Removing the precondition from either construction path must fail `test_precondition_on_both_construction_paths`. A regression that made Wave B disableable (e.g. allowing `transcript_hold_max_sessions = 0`) is caught by `config.rs validate()` (covered) AND the survival believable-zero guard (AC-07).

## Scope note
- This guards **Surface B only**. Surface A (`compaction_events`) is written at the handler regardless of Wave B; do NOT couple the Surface A writer tests to this precondition.
- This is a hard dependency crt-054 ASSERTS (fail-loud), not a code path it owns — the assertion is the contract that crt-052 Wave B stays ON/non-disableable (ADR-006/ADR-010).
