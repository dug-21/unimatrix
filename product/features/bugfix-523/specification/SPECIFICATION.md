# Specification: bugfix-523 — Hardening Batch

NLI Tick Gate + Log Downgrade + NaN Guards + Session Sanitization

GH Issue: #523
Feature dir: `product/features/bugfix-523/`

---

## Objective

Four independent hardening defects in `unimatrix-server` are resolved as a single deliverable:
an explicit `nli_enabled` gate on Path B of the graph inference tick to eliminate background
tick congestion; a log-level downgrade for expected deprecated-entry misses in Path C to
remove warn spam that obscures real signal; `!v.is_finite()` prefix guards on all 19 float
fields in `InferenceConfig::validate()` to catch NaN/Inf at server startup; and a
`sanitize_session_id` guard in the `post_tool_use_rework_candidate` UDS dispatch arm to close
the last session injection gap in `listener.rs`.

---

## Functional Requirements

### FR-01 — Explicit NLI gate in `run_graph_inference_tick`

**File**: `crates/unimatrix-server/src/services/nli_detection_tick.rs`

In `run_graph_inference_tick`, insert the following guard at the PATH B entry gate — structurally
after the `run_cosine_supports_path(...)` call returns (Path C complete) and before the
`nli_handle.get_provider().await` call:

```rust
if !config.nli_enabled {
    tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped");
    return;
}
```

The structural landmark for the insertion site is the comment block labelled
`// === PATH B entry gate ===` (currently after the closing `.await` of the
`run_cosine_supports_path` call). The existing `candidate_pairs.is_empty()` fast-exit and the
`get_provider()` call follow immediately after; the new guard must be inserted before both of
them, as the earliest possible Path B exit.

Constraints:
- Path A (Informs write loop) and Path C (`run_cosine_supports_path`) are unconditional and
  must not be gated (crt-039 ADR-001, entry #4017).
- The caller in `background.rs` must remain unconditional; the gate lives inside
  `run_graph_inference_tick` only.
- The debug message text is prescribed: `"graph inference tick: NLI disabled by config; Path B
  skipped"`. This is intentionally distinct from the existing `get_provider()` Err message
  (`"graph inference tick: NLI provider not ready; Supports path skipped"`) so operators can
  differentiate intentional-off from transient-not-ready.
- Update the comment at the `get_provider()` call site to reference the explicit check rather
  than calling the Err-return path "expected when nli_enabled=false".

### FR-02 — Log downgrade for deprecated-entry misses in `run_cosine_supports_path`

**File**: `crates/unimatrix-server/src/services/nli_detection_tick.rs`

Change `tracing::warn!` to `tracing::debug!` at exactly two sites within
`run_cosine_supports_path`:

1. The `category_map.get(src_id)` None arm (currently: `"Path C: source entry not found in
   category_map (deprecated mid-tick?) — skipping"`).
2. The `category_map.get(tgt_id)` None arm (currently: `"Path C: target entry not found in
   category_map (deprecated mid-tick?) — skipping"`).

The existing finite-cosine warn at the `!cosine.is_finite()` guard site (currently:
`"Path C: non-finite cosine for candidate pair — skipping"`) is a structural anomaly (NaN
from HNSW) and must remain `tracing::warn!` unchanged.

No other changes to `run_cosine_supports_path`. The existing comment at the Gate 3 block
already explains the expected-deprecation scenario; no new comments required.

### FR-03 — `!v.is_finite()` prefix guards on all 19 float fields in `InferenceConfig::validate()`

**File**: `crates/unimatrix-server/src/infra/config.rs`

For each of the 19 fields listed in the field checklist below, prefix the existing comparison
guard with `!v.is_finite() || `. The pattern follows the three crt-046 guards already present
(lines ~1382, ~1393, ~1404):

```rust
// Before (example):
if self.nli_entailment_threshold <= 0.0 || self.nli_entailment_threshold >= 1.0 {

// After:
let v = self.nli_entailment_threshold;
if !v.is_finite() || v <= 0.0 || v >= 1.0 {
```

For the fusion weight fields and phase weight fields, which use a loop over a slice, the
`is_finite()` guard must be added inside the loop:

```rust
// Before (example, fusion weights):
for (field, value) in fusion_weight_checks {
    if *value < 0.0 || *value > 1.0 {

// After:
for (field, value) in fusion_weight_checks {
    if !value.is_finite() || *value < 0.0 || *value > 1.0 {
```

Error variant: `ConfigError::NliFieldOutOfRange` — the established variant (lesson #4132). No
new error variants.

The `value.to_string()` representation of NaN is `"NaN"`, which is a valid debug string.
No format change to the error struct fields.

#### Field Checklist — All 19 Fields

| # | Field name | Type | Current guard (before prefix) | Valid range |
|---|-----------|------|-------------------------------|-------------|
| 1 | `nli_entailment_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 2 | `nli_contradiction_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 3 | `nli_auto_quarantine_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 4 | `supports_candidate_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 5 | `supports_edge_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 6 | `ppr_alpha` | f64 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 7 | `ppr_inclusion_threshold` | f64 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 8 | `ppr_blend_weight` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 9 | `nli_informs_cosine_floor` | f32 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 10 | `nli_informs_ppr_weight` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 11 | `supports_cosine_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 12 | `w_sim` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 13 | `w_nli` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 14 | `w_conf` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 15 | `w_coac` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 16 | `w_util` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 17 | `w_prov` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 18 | `w_phase_histogram` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 19 | `w_phase_explicit` | f64 | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |

Fields 1–11 are inline guards using a local `let v = self.<field>;` binding.
Fields 12–17 are loop-body guards in the `fusion_weight_checks` loop.
Fields 18–19 are loop-body guards in the `phase_weight_checks` loop.

Note: `usize` and `u32` fields (`nli_top_k`, `max_contradicts_per_tick`,
`max_graph_inference_per_tick`, etc.) are not subject to IEEE 754 NaN and are out of scope.

### FR-04 — `sanitize_session_id` guard in `post_tool_use_rework_candidate` arm

**File**: `crates/unimatrix-server/src/uds/listener.rs`

In `dispatch_request`, within the `post_tool_use_rework_candidate` arm
(`HookRequest::RecordEvent { ref event } if event.event_type == "post_tool_use_rework_candidate"`),
add the following guard immediately after the capability check block (after the
`uds_has_capability(Capability::SessionWrite)` return) and before any use of
`event.session_id`:

```rust
if let Err(e) = sanitize_session_id(&event.session_id) {
    tracing::warn!(
        session_id = %event.session_id,
        error = %e,
        "UDS: RecordEvent (rework_candidate) rejected: invalid session_id"
    );
    return HookResponse::Error {
        code: ERR_INVALID_PAYLOAD,
        message: e,
    };
}
```

This is the identical pattern applied to the `RecordEvent` general arm (lines 731–738). The
warn message differs only in the parenthetical qualifier `(rework_candidate)` to identify the
arm in logs.

Structural insertion order:
1. Capability check (existing — unchanged)
2. `sanitize_session_id` guard (new — FR-04)
3. Payload field extraction (`tool_name`, `file_path`, `had_failure`) (existing — unchanged)
4. `session_registry.record_rework_event(&event.session_id, ...)` (existing — unchanged)

The `session_id` referenced here is `event.session_id` (field on `HookEvent`), not a
destructured local. The guard must reference `&event.session_id` consistently.

---

## Non-Functional Requirements

### NFR-01 — No regression on Path A or Path C when `nli_enabled = false`

Path A (Informs write loop) and Path C (`run_cosine_supports_path`) must continue to run and
produce graph edges on every tick regardless of the `nli_enabled` flag value. The gate in
FR-01 must be inserted after both paths complete; it must not be inserted at the function
entry point.

### NFR-02 — `InferenceConfig::validate()` catches NaN/Inf before first field use

The `!v.is_finite()` guards must be applied inside `validate()`, which is called at server
startup before any field value is used in computation. NaN must not propagate into scoring
formulas, HNSW queries, or graph weight writes.

### NFR-03 — `sanitize_session_id` is synchronous and O(1)

The `sanitize_session_id` call is a character-by-character scan of a bounded string (max 128
chars). It adds negligible latency to UDS dispatch. No async, no DB access, no allocation
beyond the error string on failure.

### NFR-04 — No new dependencies

All four items use existing functions, error variants, log macros, and test helpers. No new
crates, no new error variants (except as noted — none needed), no schema changes.

### NFR-05 — No regression on existing tests

All existing `InferenceConfig::validate()` tests (boundary values, cross-field invariants)
must continue to pass. All existing UDS dispatch arm tests must continue to pass.

---

## Acceptance Criteria

All 29 ACs are reproduced verbatim from SCOPE.md. Every AC is required; none may be deferred.

- **AC-01**: When `inference_config.nli_enabled = false`, `run_graph_inference_tick` returns
  after Path C without calling `nli_handle.get_provider()` or spawning any rayon task.

- **AC-02**: When `inference_config.nli_enabled = false`, Path A (Informs writes) and Path C
  (cosine Supports writes) are NOT affected — they continue to run and write edges as normal.

- **AC-03**: When `inference_config.nli_enabled = true` and a provider is available, Phases
  6/7/8 continue to execute (no regression in NLI-enabled path).

- **AC-04**: `run_cosine_supports_path` with a candidate pair whose source or target ID is
  absent from `category_map` emits `tracing::debug!` (not `warn!`) and skips the pair.

- **AC-05**: `run_cosine_supports_path` with a non-finite cosine value continues to emit
  `tracing::warn!` (the non-finite cosine guard at line 766 is unchanged).

- **AC-06**: `InferenceConfig::validate()` returns
  `Err(ConfigError::NliFieldOutOfRange { field: "nli_entailment_threshold", ... })` when
  `nli_entailment_threshold = f32::NAN`.

- **AC-07**: `InferenceConfig::validate()` returns
  `Err(ConfigError::NliFieldOutOfRange { field: "nli_contradiction_threshold", ... })` when
  `nli_contradiction_threshold = f32::NAN`.

- **AC-08**: `InferenceConfig::validate()` returns
  `Err(ConfigError::NliFieldOutOfRange { field: "nli_auto_quarantine_threshold", ... })` when
  `nli_auto_quarantine_threshold = f32::NAN`.

- **AC-09**: `InferenceConfig::validate()` returns `Err` when
  `supports_candidate_threshold = f32::NAN`.

- **AC-10**: `InferenceConfig::validate()` returns `Err` when
  `supports_edge_threshold = f32::NAN`.

- **AC-11**: `InferenceConfig::validate()` returns `Err` when `ppr_alpha = f64::NAN`.

- **AC-12**: `InferenceConfig::validate()` returns `Err` when
  `ppr_inclusion_threshold = f64::NAN`.

- **AC-13**: `InferenceConfig::validate()` returns `Err` when `ppr_blend_weight = f64::NAN`.

- **AC-14**: `InferenceConfig::validate()` returns `Err` when
  `nli_informs_cosine_floor = f32::NAN`.

- **AC-15**: `InferenceConfig::validate()` returns `Err` when
  `nli_informs_ppr_weight = f64::NAN`.

- **AC-16**: `InferenceConfig::validate()` returns `Err` when
  `supports_cosine_threshold = f32::NAN`.

- **AC-17**: `InferenceConfig::validate()` returns `Err` when `w_sim = f64::NAN`.

- **AC-18**: `InferenceConfig::validate()` returns `Err` when `w_nli = f64::NAN`.

- **AC-19**: `InferenceConfig::validate()` returns `Err` when `w_conf = f64::NAN`.

- **AC-20**: `InferenceConfig::validate()` returns `Err` when `w_coac = f64::NAN`.

- **AC-21**: `InferenceConfig::validate()` returns `Err` when `w_util = f64::NAN`.

- **AC-22**: `InferenceConfig::validate()` returns `Err` when `w_prov = f64::NAN`.

- **AC-23**: `InferenceConfig::validate()` returns `Err` when
  `w_phase_histogram = f64::NAN`.

- **AC-24**: `InferenceConfig::validate()` returns `Err` when `w_phase_explicit = f64::NAN`.

- **AC-25**: `InferenceConfig::validate()` returns `Err` when
  `nli_entailment_threshold = f32::INFINITY` (representative f32 Inf test).

- **AC-26**: `InferenceConfig::validate()` returns `Err` when `ppr_alpha = f64::INFINITY`
  (representative f64 Inf test).

- **AC-27**: All existing `InferenceConfig::validate()` tests continue to pass — no regression
  on boundary values.

- **AC-28**: `dispatch_request` with a `HookRequest::RecordEvent { event }` where
  `event.event_type == "post_tool_use_rework_candidate"` and `event.session_id` contains an
  invalid character returns `HookResponse::Error { code: ERR_INVALID_PAYLOAD, message: _ }`
  without calling `session_registry.record_rework_event`.

- **AC-29**: `dispatch_request` with a valid `post_tool_use_rework_candidate` event continues
  to call `session_registry.record_rework_event` (no regression on the normal path).

### AC Testability Notes (SR-03 mitigation)

AC-04 (warn→debug downgrade) and the debug! log on early return in FR-01 introduce log-level
assertions. Two options are available to the tester:

**Option A (preferred)**: Use `tracing-test` crate (`#[traced_test]`) to capture log output
in unit tests and assert `!logs_contain("WARN")` and `logs_contain("DEBUG")` for the
category_map miss path (AC-04) and the NLI gate debug! message (AC-01 log assertion).

**Option B (fallback)**: Provide behavioral coverage only — assert the skip/return behavior
without asserting log level. Document the log-level portion as verified by code review only.
This option is acceptable at gate but the tester must flag it explicitly in the test report so
the security reviewer is aware.

The implementor chooses between Option A and Option B; the architect must note the choice in
the IMPLEMENTATION-BRIEF. Option A is preferred to prevent SR-03 gate regression.

---

## Domain Models / Ubiquitous Language

### Graph Inference Tick Paths

The `run_graph_inference_tick` function in `nli_detection_tick.rs` executes three distinct
paths on each background tick:

| Path | Name | Description | Gating |
|------|------|-------------|--------|
| Path A | Informs (structural) | Writes `INFORMS` edges based on structural co-occurrence and PPR graph walk. Phases 4a–5. | Unconditional. Runs on every tick regardless of `nli_enabled`. |
| Path B | NLI Supports | Scores candidate pairs using the NLI model (rayon pool). Phases 6/7/8. | Gated by `nli_enabled` (after FR-01) and `get_provider()` availability. Skipped entirely when `nli_enabled = false`. |
| Path C | Cosine Supports | Writes `SUPPORTS` edges based on pure cosine similarity. No NLI model invocation. | Unconditional. Runs on every tick regardless of `nli_enabled`. |

Path A and Path C share the same `candidate_pairs` list produced by Phase 4b (HNSW scan).
Path B consumes the same `candidate_pairs` list but only when `nli_enabled = true` and a
provider is available.

### `nli_enabled` Flag Semantics

`InferenceConfig.nli_enabled` (bool) represents operator intent:
- `false` (production default): NLI model is intentionally disabled. Path B is not expected
  to run. The `NliServiceHandle` may still exist but `get_provider()` will return Err. The
  new explicit gate short-circuits before calling `get_provider()`.
- `true`: NLI model is configured and expected to be available. Path B runs when a provider
  is ready. If `get_provider()` returns Err (transient), Path B is skipped for that tick
  with a distinct log message.

The two log messages are intentionally different to allow operators to distinguish state:
- `"graph inference tick: NLI disabled by config; Path B skipped"` — `nli_enabled = false`
- `"graph inference tick: NLI provider not ready; Supports path skipped"` — `nli_enabled =
  true`, transient Err from `get_provider()`

### `category_map` and Deprecated-Entry Misses

`category_map: HashMap<u64, &str>` is populated from `all_active` entries at Phase 2 (start
of tick). HNSW candidates are derived from the vector index, which is rebuilt on compaction
cycles, not on every deprecation event. A candidate entry can be deprecated between Phase 2
and the Path C loop. When this occurs, `category_map.get(id)` returns `None`. This is
expected degraded-mode behavior, not an error. Log level must be `debug!`.

### `sanitize_session_id` Contract

`sanitize_session_id(s: &str) -> Result<(), String>`:
- Accepts strings matching `[a-zA-Z0-9\-_]+` with length in [1, 128].
- Returns `Ok(())` if valid.
- Returns `Err(String)` with a human-readable message if invalid. The error string is
  returned directly as the `message` field in `HookResponse::Error`.
- This allowlist is consistent across all UDS arms. It must not be widened for the rework arm.
- The function is synchronous and O(length), bounded at 128 iterations.

### `InferenceConfig::validate()` NaN Trap

IEEE 754 defines NaN comparisons as always-false. A guard of the form `v <= 0.0 || v >= 1.0`
does not fire when `v = NaN` because both sub-expressions evaluate to false. Without
`!v.is_finite()` as a prefix, NaN silently passes validation and propagates into scoring
formulas, graph weight writes, or rayon batch inputs, causing undefined computation results
until server restart. The fix is `!v.is_finite() || <existing_comparison>`.

---

## User Workflows

### Operator: Deploying with `nli_enabled = false` (production default)

1. Operator starts server with default config (`nli_enabled = false`).
2. Background tick fires. `run_graph_inference_tick` executes Path A (Informs) and Path C
   (cosine Supports) to completion.
3. New gate fires: `!config.nli_enabled` is true. Function returns with a single `debug!`
   log. No async `get_provider()` call, no rayon spawn.
4. Operator observes no warn spam from the category_map miss site (FR-02 effect) and no tick
   congestion from NLI rayon pool (FR-01 effect).

### Operator: Supplying NaN in config

1. Operator's config file contains `nli_entailment_threshold = NaN` (or equivalent
   serialized form).
2. Server starts. `InferenceConfig::validate()` runs.
3. FR-03 guard fires: `!v.is_finite()` is true. `validate()` returns
   `Err(ConfigError::NliFieldOutOfRange { field: "nli_entailment_threshold", value: "NaN",
   reason: "must be in range (0.0, 1.0) exclusive" })`.
4. Server fails fast at startup with a clear field-level error. NaN does not reach any
   scoring pipeline.

### Hook Client: Sending rework event with invalid session_id

1. Hook client sends `HookRequest::RecordEvent { event }` where
   `event.event_type = "post_tool_use_rework_candidate"` and `event.session_id =
   "../../etc/passwd"`.
2. Capability check passes (SessionWrite present).
3. FR-04 guard fires: `sanitize_session_id("../../etc/passwd")` returns Err.
4. Arm returns `HookResponse::Error { code: ERR_INVALID_PAYLOAD, message: _ }`.
5. `session_registry.record_rework_event` is never called.
6. warn! log is emitted with `session_id` and error for audit visibility.

---

## Constraints

| ID | Constraint | Source |
|----|-----------|--------|
| C-01 | The `run_graph_inference_tick` caller in `background.rs` must remain unconditional. | crt-039 ADR-001, entry #4017 |
| C-02 | Path A (Informs) and Path C (cosine Supports) must not be gated by `nli_enabled`. | crt-039 ADR-001, entry #4017 |
| C-03 | The NLI gate must be inserted inside `run_graph_inference_tick` at the Path B boundary only (after Path C completes). | SR-01 |
| C-04 | `ConfigError::NliFieldOutOfRange` is the only error variant for `InferenceConfig` float field errors. No new error variants. | Lesson #4132 |
| C-05 | `ERR_INVALID_PAYLOAD` is the established error code for session_id validation failures in UDS dispatch arms. | Entry #3921, existing RecordEvent arm |
| C-06 | All new NaN tests in config.rs must use the `assert_validate_fails_with_field(c, "field_name")` helper (line ~4615 in config.rs) with the exact field name string. | SCOPE.md constraint |
| C-07 | The `sanitize_session_id` guard in FR-04 must be placed after the capability check and before any use of `event.session_id`. | SR-05 |
| C-08 | Items 1 and 2 both modify `nli_detection_tick.rs`. They must be assigned to the same implementation wave or agent to avoid merge conflicts. | SR-06 |
| C-09 | `!v.is_finite()` guards must not be applied to `usize` or `u32` fields in `InferenceConfig`. Scope is float fields only. | SCOPE.md non-goals |
| C-10 | No changes to `RetentionConfig`, `CoherenceConfig`, or any struct other than `InferenceConfig`. | SCOPE.md non-goals |

---

## Dependencies

### Existing Components Used

| Component | Role | Item |
|-----------|------|------|
| `InferenceConfig.nli_enabled` (bool) | Gate condition in FR-01 | 1 |
| `NliServiceHandle::get_provider()` | Existing async call that FR-01 gate avoids | 1 |
| `tracing::debug!` / `tracing::warn!` | Log macros — no changes to macro selection beyond FR-01/FR-02 | 1, 2 |
| `ConfigError::NliFieldOutOfRange` | Existing error variant for all float field validation failures | 3 |
| `sanitize_session_id` | Existing function in `listener.rs` | 4 |
| `ERR_INVALID_PAYLOAD` | Existing error code constant in `listener.rs` | 4 |
| `assert_validate_fails_with_field` | Existing test helper in `config.rs` (~line 4615) | 3 (tests) |
| `tracing-test` (optional) | Log-level assertion in tests (Option A for SR-03) | 1, 2 (tests) |

### External Constraints

- No new crates.
- No schema changes (no SQLite migrations).
- No MCP API surface changes.
- No changes to `NliServiceHandle` interface or initialization path.

---

## NOT in Scope

The following items are explicitly excluded to prevent scope creep:

- Removing Path B code. NLI Supports detection remains in the codebase for future reactivation.
- Re-introducing the outer `if inference_config.nli_enabled` gate in `background.rs` (removed
  by crt-039 ADR-001). That gate must remain absent.
- Adding `is_finite()` guards to `RetentionConfig`, `CoherenceConfig`, or any struct other
  than `InferenceConfig`.
- Adding `is_finite()` guards to integer fields (`usize`, `u32`) in `InferenceConfig`.
- Changing the `NliServiceHandle::get_provider()` interface or initialization path.
- Any schema change, new MCP tool, or API surface change.
- Fixing pre-existing open issues (#452, #303, #305).
- Adding guards to any fields added to `InferenceConfig` after PR #516 that are not in the
  19-field checklist above. Those are out of scope per field-list assumption (SCOPE.md §Assumptions).
- Changing log levels at any site other than the two category_map miss sites in
  `run_cosine_supports_path` (FR-02).

---

## Open Questions

No open questions remain. All four OQs from SCOPE.md were resolved prior to this specification:

- OQ-01 RESOLVED: Fusion and phase weight fields are included. 19 total fields.
- OQ-02 RESOLVED: Distinct debug! message text specified (see FR-01).
- OQ-03 RESOLVED: NaN tests for all 19 fields (AC-06 through AC-24). Two representative
  Inf tests (AC-25, AC-26).
- OQ-04 RESOLVED: PR #521 merged. Rework candidate arm is the sole remaining gap.

One specification-level decision made during authoring:

- **SD-01 (SR-03 mitigation)**: AC-04 and AC-01 log-level testability is addressed by
  offering two options (tracing-test vs. behavioral-only) with a preference for Option A.
  The architect must choose and document the selection in the IMPLEMENTATION-BRIEF. This is
  not an open question for the implementor — it must be resolved before delivery begins.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 18 entries; most relevant were
  #4132 (NaN trap lesson, active, confirmed pattern and error variant), #3902 (lesson on
  sanitize_session_id guard omission pattern), and #3461 (operator-togglable debug logging
  pattern). Entry #3921 (sanitize_session_id consistency rule) is deprecated but its content
  was confirmed current against the source code.
