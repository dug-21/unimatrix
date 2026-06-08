# Test Plan — C5 `infra/session.rs` FeatureSource precedence

Source: ADR-004. ACs: AC-04, AC-07. Risks: R-04, R-13, R-17, R-09, R-18. File: extend `crates/unimatrix-server/src/infra/session.rs` tests. `cargo test -p unimatrix-server`.

Enums: `FeatureSource::{Declared, Inferred(InferredOrigin)}`, `InferredOrigin::{Registered, Voted}`; `SessionState.feature_source` (default `Inferred(Registered)`). Four fenced touchpoints only (SR-05, C-09): field+enums, source assignment at existing set sites, `apply_stamp`, two `or_else` flips.

## FeatureSource decision tree (R-04 — CRITICAL, one case per branch)

The `matches!(src, Declared)` guard is the **only** precedence determinant. ADR-004 §4 decision tree:

### unstamped_declared_registry_beats_extraction (FR-15, #588 unstamped-window remedy)
- Unstamped event; registry feature with `FeatureSource::Declared` + a contradicting extracted `topic_signal` → row attributes to the **declared** feature, `topic_source == 'declared'`.

### unstamped_inferred_registered_extraction_wins
- Unstamped event; registry feature `Inferred(Registered)` + extracted signal → **extraction** wins, `topic_source == 'extracted'` ("explicit wins" survives only against Inferred/absent registry — no never-declare regression).

### inferred_voted_no_extraction_yields_registry_or_vote_source (FR-21)
- Registry `Inferred(Voted)` + no extraction → `registry-fill`/`vote` source per FR-21, **not** `declared`.

### debug_forensics_log_now_logs_declared_over_extraction (ADR-004 consequence)
- Assert the inverted forensics log fires (logs declared-overrides-extraction, the new direction).

## `apply_stamp` idempotency (R-17)

### apply_stamp_sets_declared_idempotent
- First stamped event: `apply_stamp(sid, "vnc-030")` sets feature + `Declared`. Second stamped event same session same topic → **no-op** (no `Overridden` log, feature+source unchanged, no churn/mutex thrash).

### apply_stamp_contradicting_topic_logs_override_once
- Stamped event after a contradicting declared topic → last-writer-wins, logged **once** as a genuine override (not per-event).

### apply_stamp_restores_declared_after_reregister (R-13)
- After a re-register resets `feature_source` to `Inferred(Registered)`, one stamped event re-applies `Declared`.

## Inversion fix 1 — sweep (R-04, FR-16, session.rs:628)

### sweep_declared_beats_contradicting_vote
- A declared session (`feature_source == Declared`, `feature.is_some()`) with a contradicting `majority_vote_internal` result → `sweep_stale_sessions` resolves to the **declared** feature (was: `majority_vote_internal(..).or_else(state.feature)`).

### sweep_inferred_session_uses_vote_then_registry (floor preserved)
- An `Inferred` session → vote → `.or_else(registry feature)`, today's order (NULL-gated). No never-declare regression.

### sweep_returns_resolved_feature_for_crt052 (ADR-007 §2 interface)
- `SweepResult.resolved_feature == declared feature` when `feature_source == Declared && feature.is_some()` — the citable interface crt-052 consumes.

## Re-register window (R-13 — accepted-consequence boundary)

### reregister_then_sweep_before_stamp_degrades_then_restores
- Simulate re-register (`feature_source → Inferred(Registered)`) then an **immediate** sweep with a contradicting vote and **no** intervening stamped event → behavior matches the **documented accepted consequence** (degrades to floor for the gap), NOT a regression beyond it. Then: re-register → one stamped event (`apply_stamp` restores `Declared`) → sweep → declared wins. (Assert the boundary, don't assert it away.)

## Minimal-diff / crt-052 adjacency (R-18, FR-18, C-10)

### inversion_fix_is_minimal_diff (gate diff review)
- Verification: the sweep fix is **one guard around the existing `or_else` + the FeatureSource check, nothing else**. Zero changes to `drain_and_signal_session`/`clear_transcripts_for_feature`/transcript buffer. (Diff review at gate, not a runtime test.)

## AC-07 accuracy (manual, R-09/R-20)
- Accuracy denominator = **declared protocol sessions only** (declaration = ground truth). Fallback regression sample includes ≥1 uni-zero, ≥1 research-spike, ≥1 ad-hoc never-declare shape (SR-06). Before/after `topic_source` distribution comparison on the live DB **windowed on post-migration rows** (R-20). Canary is **decoupled** from this step (ADR-006 rev2 — no noise-baseline measurement here).

## Coverage requirement
The full ADR-004 §4 decision tree has one case per branch; `matches!(src, Declared)` is the sole precedence determinant; `apply_stamp` no-ops when feature+source already match; the re-register accepted-consequence boundary is asserted; the sweep fix is minimal-diff with a citable `resolved_feature`.
