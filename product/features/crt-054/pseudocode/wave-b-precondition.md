# Component 10 — Wave B startup precondition assert

**File**: `crates/unimatrix-server/src/main.rs` (modify) — both server-construction paths: the daemon path (~`:698`, where `with_transcript_hold(...)` is called at `:716`) and the mirror path (~`:1234`, `with_transcript_hold` at `:1252`).
**ADRs**: ADR-010 (crt-052 Wave B is a verified startup precondition; fail-loud if the `HeldBufferScan` handle is unwired — Surface B durability depends on it).

## Purpose

Surface B's survival-to-review rests on the crt-052 Wave B transcript hold being wired into the `SessionRegistry` as the `HeldBufferScan` handle. If a regression leaves it unwired, the held route never folds and every drained-session counter silently reads 0 — the believable-zero trap at startup. This component fails LOUD at startup instead, on BOTH construction paths. (Guards Surface B ONLY; Surface A does not depend on the hold.)

## What "wired" means (verified)

`main.rs` constructs `TranscriptHold` (`:698-715` / `:1234-1251`) and calls `.with_transcript_hold(Arc::clone(&transcript_hold) as Arc<dyn HeldBufferScan>)` on the registry builder (`:716-717` / `:1252-1253`). The registry stores it as `Option<Arc<dyn HeldBufferScan>>` (`session.rs:272`). The precondition asserts that the wiring actually happened — the handle is `Some` and reachable — before the server starts serving.

## Pseudocode (added on each construction path, after the registry is built with the hold)

```
// After: let registry = SessionRegistry::new(...).with_transcript_hold(transcript_hold_dyn);
// and after the transcript_hold/config are available.

// Wave B precondition (ADR-010): Surface B (the activity fold's survival across the
// crt-052 hold) requires the HeldBufferScan handle to be wired. Fail LOUD now rather
// than silently read 0 at review.
assert_wave_b_precondition(&registry, &config)?;     // returns StartupError on failure
```

```
fn assert_wave_b_precondition(registry: &SessionRegistry, config: &UnimatrixConfig) -> Result<(), StartupError>
    // 1. The HeldBufferScan handle must be wired.
    if not registry.has_transcript_hold():           // see "Registry accessor" below
        return Err(StartupError::WaveBUnwired {
            detail: "transcript hold (HeldBufferScan) is not wired into the SessionRegistry; \
                     Surface B (activity fold) cannot survive to review",
        })

    // 2. The hold must be ON / non-disableable — crt-052 forbids transcript_hold_max_sessions == 0
    //    in RetentionConfig::validate(); re-affirm here so a regression that bypasses that validate
    //    still fails loud for Surface B's sake (NFR-7).
    if config.retention.transcript_hold_max_sessions == 0:
        return Err(StartupError::WaveBDisabled {
            detail: "transcript_hold_max_sessions == 0 disables the hold; \
                     Surface B (activity fold) would be purged before review",
        })

    Ok(())
```

## Registry accessor (small addition to `session.rs`)

The registry's `transcript_hold` is private (`Option<Arc<dyn HeldBufferScan>>`, `:272`). Add a cheap predicate so `main.rs` can assert without exposing the handle:

```
// On SessionRegistry (infra/session.rs)
pub fn has_transcript_hold(&self) -> bool
    return self.transcript_hold.is_some()
```

(If a richer probe is wanted, a method that calls `held_arcs_for_feature("__startup_probe__")` and confirms the trait object responds could be used — but `is_some()` is sufficient for "wired vs unwired" and avoids a spurious scan. Keep it to the predicate unless the test plan asks for a live probe.)

## Placement / both paths (R-01 family at startup)

- The assert MUST be added on BOTH paths (~`:698` daemon and ~`:1234` mirror). A common helper `assert_wave_b_precondition` called from both prevents the two paths from diverging (one path asserting, the other not, is exactly the kind of regression this guards).
- Place it next to the existing `RetentionConfig`/`InferenceConfig::validate()` startup checks and the `[transcript_signals]` validate (Component 9) so all startup preconditions are co-located.

## Error handling

- Failure → `StartupError` (or the existing startup error type used by the `?`-returning startup fn), surfaced loudly; the server does NOT start. No silent degrade (ADR-010).
- New `StartupError` variants: `WaveBUnwired`, `WaveBDisabled` (or fold into the existing startup/config error enum, matching the file's convention).

## Key test scenarios (hints)

- Wiring present + `transcript_hold_max_sessions > 0` → precondition passes, server starts (happy path).
- Unwired handle (registry built WITHOUT `with_transcript_hold`) → `assert_wave_b_precondition` returns `Err(WaveBUnwired)`; startup fails loud (ADR-010).
- `transcript_hold_max_sessions == 0` → `Err(WaveBDisabled)` (NFR-7 belt-and-suspenders).
- Both construction paths invoke the same helper (review + a test per path, or a shared-helper unit test) — neither path is missing the assert.
- The assert is Surface-B-only: it does NOT gate Surface A (a compaction row is written even if the hold were absent — but startup would have already failed, so this is a design invariant, not a runtime branch).
