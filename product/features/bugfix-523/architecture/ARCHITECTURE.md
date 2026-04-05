# Architecture: bugfix-523 — Server Hardening Batch

## System Overview

This batch addresses four independent defects in `unimatrix-server`. Each fix is a surgical
single-file change with no new abstractions, no schema changes, and no API surface changes.
The four items operate across three subsystems:

| Item | Subsystem | File |
|------|-----------|------|
| 1 | Cortical background tick — NLI gate | `services/nli_detection_tick.rs` |
| 2 | Cortical background tick — observability | `services/nli_detection_tick.rs` |
| 3 | Server startup validation | `infra/config.rs` |
| 4 | UDS dispatch security | `uds/listener.rs` |

Items 1 and 2 share a source file and MUST be assigned to the same implementation agent to
avoid merge conflicts (SR-06).

---

## Component Breakdown

### Item 1 — NLI Tick Gate (Path B)

**Component**: `run_graph_inference_tick` in `nli_detection_tick.rs`

**Responsibility**: Controls which phases of the graph inference tick execute based on config.
Phase A (Informs) and Path C (cosine Supports) are unconditional. Path B (NLI Supports,
Phases 6/7/8) is gated — currently by an implicit `get_provider()` error return, which still
incurs an async await call and precedes rayon dispatch.

**Fix**: Make the Path B gate explicit. Insert `if !config.nli_enabled { return; }` at the
PATH B entry point — the structural landmark after Path C's `run_cosine_supports_path` call
returns, before the `nli_handle.get_provider().await` call.

**Why this location is correct**: The comment block at the PATH B entry gate (lines 546–555)
already marks this as the boundary. Path C's `run_cosine_supports_path(...)` call ends at
line 544. The `candidate_pairs.is_empty()` fast-exit at line 552 is an orthogonal short-circuit
(no candidates regardless of NLI config). The `nli_enabled` gate must come after the empty-check
would have returned in order to preserve the empty-check's `tracing::debug!` message, but that
ordering is not load-bearing — both guards precede `get_provider()`. The gate must land BEFORE
the `get_provider().await` at line 560 to avoid the async call entirely.

The correct insertion sequence at the PATH B entry gate is:

```
// [existing] if candidate_pairs.is_empty() { return; }
// [NEW] if !config.nli_enabled { tracing::debug!("..."); return; }
// [existing] let provider = match nli_handle.get_provider().await { ... }
```

**ADR-001 compliance** (entry #4017): Phase A (Informs write loop, Phase 4b) and Path C
(cosine Supports, `run_cosine_supports_path`) execute before this gate. Both are unconditional.
Only Path B — the `get_provider()` call and the rayon dispatch — is skipped. The outer call
in `background.rs` remains unconditional per ADR-001.

**Distinct log message** (OQ-02 resolved): Emit
`tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped")`
on early return. This is distinct from the existing `get_provider()` Err message
(`"graph inference tick: NLI provider not ready; Supports path skipped"`) so operators can
distinguish intentional-off from transient-not-ready.

---

### Item 2 — Log Level Downgrade in `run_cosine_supports_path`

**Component**: `run_cosine_supports_path` in `nli_detection_tick.rs`

**Responsibility**: Processes cosine-based Supports candidates through a multi-gate pipeline.
Gate 3 (category lookup) handles the case where an entry was deprecated between Phase 2 DB
read and the current point in the tick.

**Fix**: Change `tracing::warn!` to `tracing::debug!` at the two category_map miss sites:
- Source ID absent from category_map (after `category_map.get(src_id)`)
- Target ID absent from category_map (after `category_map.get(tgt_id)`)

**Why this is expected behavior**: The comment at Gate 3 (line 791–792 in source) already
documents: "If an entry was deprecated between Phase 2 DB read and this point, it will be
absent." The HNSW index is rebuilt on compaction cycles, not on every deprecation. A miss
here is an expected race condition in the HNSW-plus-DB architecture, not an anomaly.

**What stays as `warn!`**: The non-finite cosine guard at line 765–771 MUST remain `warn!`.
A NaN/Inf cosine from HNSW is a structural anomaly — it indicates a potential data integrity
issue in the vector index, not an expected race condition. Log level semantic contract (ADR
entry #3467): operational anomalies use `warn!`; expected degraded-mode behavior uses `debug!`.

---

### Item 3 — `InferenceConfig::validate()` NaN Guards

**Component**: `InferenceConfig::validate()` in `infra/config.rs`

**Responsibility**: Validates all `InferenceConfig` fields at server startup before any scoring
or detection code executes.

**Problem**: IEEE 754 comparison semantics: `NaN <= 0.0` evaluates to `false`, and `NaN >= 1.0`
evaluates to `false`. A guard of `v <= 0.0 || v >= 1.0` silently passes NaN. The three
crt-046 fields (`goal_cluster_similarity_threshold`, `w_goal_cluster_conf`, `w_goal_boost`)
already have `!v.is_finite()` prefix guards (PR #516, lesson #4132). All earlier float fields
do not.

**Fix**: For each of the 19 affected fields, prefix the existing comparison guard with
`!v.is_finite() || `. This converts a comparison-only guard to a finite-then-compare guard.
No new `ConfigError` variant is needed — `ConfigError::NliFieldOutOfRange` is the established
variant for all `InferenceConfig` field errors.

**Why weight fields are included** (OQ-01 resolved): The 6 fusion weight fields (`w_sim`,
`w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`) and 2 phase weight fields
(`w_phase_histogram`, `w_phase_explicit`) currently rely on the sum check to catch NaN
indirectly. However: a NaN in any fusion weight makes `fusion_weight_sum` NaN, and
`NaN > 1.0` is false, so the sum check silently passes. The resulting error would be a
misleading "weights don't sum to 1.0" rather than "w_sim is not finite". NaN fusion weights
propagate into every search result until server restart. Adding `!v.is_finite()` prefix to
the per-field loop catches the problem at the correct field with the correct error.

**Implementation structure**: The 6 fusion weight fields are validated in a loop over
`fusion_weight_checks` array (lines 1151–1169). The 2 phase weight fields are validated in a
loop over `phase_weight_checks` array (lines 1173–1187). For both loops, the guard
`if *value < 0.0 || *value > 1.0` must become `if !value.is_finite() || *value < 0.0 || *value > 1.0`.
For individual f32 and f64 field checks (the 11 threshold fields), each guard is expanded
inline at its existing check site.

**Complete field list** (19 fields — implementor checklist):

Group A — individual f32 threshold fields (11):
- `nli_entailment_threshold` — guard `<= 0.0 || >= 1.0`
- `nli_contradiction_threshold` — guard `<= 0.0 || >= 1.0`
- `nli_auto_quarantine_threshold` — guard `<= 0.0 || >= 1.0`
- `supports_candidate_threshold` — guard `<= 0.0 || >= 1.0`
- `supports_edge_threshold` — guard `<= 0.0 || >= 1.0`
- `nli_informs_cosine_floor` — guard `<= 0.0 || >= 1.0`
- `supports_cosine_threshold` — guard `<= 0.0 || >= 1.0`
- `ppr_alpha` (f64) — guard `<= 0.0 || >= 1.0`
- `ppr_inclusion_threshold` (f64) — guard `<= 0.0 || >= 1.0`
- `ppr_blend_weight` (f64) — guard `< 0.0 || > 1.0`
- `nli_informs_ppr_weight` (f64) — guard `< 0.0 || > 1.0`

Group B — fusion weight fields in loop (6 f64):
- `w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`

Group C — phase weight fields in loop (2 f64):
- `w_phase_histogram`, `w_phase_explicit`

**What this does NOT change**: Cross-field invariant checks (lines 1080–1086,
1110–1116) are downstream of individual-field checks. They are not modified.
`ConfigError::NliFieldOutOfRange` is unchanged. No new error variants.

---

### Item 4 — `sanitize_session_id` Gap in `post_tool_use_rework_candidate` arm

**Component**: `dispatch_request` match arm for `HookRequest::RecordEvent` where
`event.event_type == "post_tool_use_rework_candidate"` in `uds/listener.rs`

**Responsibility**: Validates and dispatches rework candidate events from PostToolUse hooks
(col-009), recording to session registry and observation store.

**Problem**: The arm reads `event.session_id` directly at line 690 (`record_rework_event`)
and line 694 (`record_topic_signal`) without calling `sanitize_session_id`. Every other
`dispatch_request` arm that consumes `session_id` has this guard (pattern #3921). This arm
is the last missing guard.

**Fix**: Insert the `sanitize_session_id` guard immediately after the capability check block
(after the `return HookResponse::Error { code: -32003 }` arm at lines 660–665), before the
`event.payload.get("tool_name")` extraction at line 666. The session_id here is
`event.session_id` — a field on the `HookEvent` struct, not a destructured local.

The guard pattern mirrors the `RecordEvent` general arm (lines 731–741):

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

**Why after capability check, not before**: The capability check gate at lines 660–665 is a
permissions gate — it returns before touching any session data. The session_id guard is a
data integrity gate — it must come before the first `event.session_id` use. The capability
check can legitimately reject before validating session_id (an unauthorized client's malformed
session_id is irrelevant). This ordering is consistent with all other arms in `dispatch_request`.

**SR-05 compliance**: The guard is inserted before `event.payload.get("tool_name")` at the
start of payload extraction, which is before both `record_rework_event` and
`record_topic_signal` calls. No `event.session_id` value reaches any registry or storage
path without passing the guard.

---

## Component Interactions

All four items are independent. No data flows between them. The interaction map is:

```
background.rs (tick scheduler)
  └─ run_graph_inference_tick()          ← Item 1 gate inserted here
       ├─ Phase A: Informs write loop    ← unconditional, unaffected
       ├─ Path C: run_cosine_supports_path()  ← unconditional, Item 2 warn→debug here
       └─ Path B: [gate] get_provider() → rayon dispatch  ← Item 1 gate skips this

ServerConfig::validate()
  └─ InferenceConfig::validate()        ← Item 3: 19 field guards added
       (called at server startup, before any tick runs)

UDS socket → dispatch_request()
  └─ RecordEvent { post_tool_use_rework_candidate } arm  ← Item 4 guard added here
       ├─ sanitize_session_id()  [NEW guard]
       ├─ record_rework_event()
       └─ record_topic_signal()
```

---

## Technology Decisions

See ADR-001 (file: `ADR-001-hardening-batch-523.md`) for all decisions in this batch.

No new dependencies. No new error types. No new MCP tools or schema changes.

---

## Integration Points

All existing interfaces are unchanged by this batch:

| Interface | Change |
|-----------|--------|
| `NliServiceHandle::get_provider()` | None — called less often when `nli_enabled=false`, same signature |
| `HookResponse` variants | None — `HookResponse::Error { code, message }` used as-is |
| `ConfigError::NliFieldOutOfRange` | None — reused as established variant |
| `sanitize_session_id` | None — called from one additional site, same signature |
| `rayon_pool.spawn()` | None — skipped when gate fires, unchanged when executed |
| `record_rework_event` / `record_topic_signal` | None — not reached on invalid session_id |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `config.nli_enabled` field access | `bool` on `InferenceConfig` | `infra/config.rs` |
| `nli_handle.get_provider()` | `async fn() -> Result<Arc<dyn NliProvider>, NliError>` | `services/nli_service.rs` |
| `sanitize_session_id(s: &str)` | `fn(&str) -> Result<(), String>` | `uds/listener.rs` (local) |
| `ERR_INVALID_PAYLOAD` | `i64` constant | `uds/listener.rs` |
| `ConfigError::NliFieldOutOfRange { path, field, value, reason }` | enum variant | `infra/config.rs` |
| `rayon_pool.spawn(closure)` | `fn(FnOnce() -> T + Send + 'static) -> JoinHandle<T>` | `services/rayon_pool.rs` |
| `run_cosine_supports_path(store, config, pairs, existing, category_map, ts)` | `async fn` | `nli_detection_tick.rs` |

---

## Test Architecture

### SR-03 Resolution: Log Level Test Strategy (Explicit Decision)

**Decision: Behavioral-only coverage for Items 1 and 2 log-level ACs. Log level is not
asserted in tests. This is acknowledged and defended.**

Rationale:

1. Lesson #3935 (referenced in SR-03): prior features that attempted tracing-level assertions
   using `tracing-test` or `tracing_subscriber` harnesses caused Gate 3b failures due to test
   ordering, subscriber state leakage between parallel tests, and runtime initialization conflicts.

2. The cost-benefit is asymmetric: adding `tracing-test` as a dev-dependency introduces a
   harness used in exactly two test assertions, while the behavioral invariant (skip logic,
   no panic, correct return value) is what protects production.

3. The log level change (warn → debug) is a two-line edit with no branches. The risk of
   incorrect level is lower than the risk of flaky tracing tests blocking the gate.

**What IS tested for Items 1 and 2**:
- Item 1 (AC-01/AC-02): Test that `run_graph_inference_tick` with `nli_enabled=false` does NOT
  call `get_provider()` — verified by asserting Supports edges are not written and no provider
  panic occurs. Path A writes ARE present in output (AC-02). This is a behavioral proxy.
- Item 2 (AC-04/AC-05): Test that `run_cosine_supports_path` with a candidate pair absent from
  `category_map` continues (no panic, pair skipped, function returns). The non-finite cosine
  case still panics/warns — test that it is handled. Log level itself is NOT asserted.

**Gate acknowledgment**: At Gate 3b, reviewers must accept AC-04 as behavioral-only. The
architecture explicitly states this is the agreed approach. Any gate feedback requesting
log-level assertions must be escalated to the Bugfix Leader, not unilaterally added by
the implementation agent (which would introduce the `tracing-test` harness risk).

### Test locations

| Item | Test file | Pattern |
|------|-----------|---------|
| 1 (NLI gate) | `nli_detection_tick.rs` `#[cfg(test)]` | Existing `test_path_c_runs_unconditionally_nli_disabled` tests are the baseline; add test asserting no Supports edge with `nli_enabled=false` and a mock-ready provider (to distinguish gate from `get_provider()` Err) |
| 2 (log downgrade) | `nli_detection_tick.rs` `#[cfg(test)]` | Test `run_cosine_supports_path` with category_map missing src_id or tgt_id; assert pair is skipped, function returns without panic |
| 3 (NaN guards) | `config.rs` `#[cfg(test)]` | Use `assert_validate_fails_with_field(c, "field_name")` for all 19 fields + 2 representative Inf tests (AC-06 through AC-26). Each test sets exactly one field to `f32::NAN` or `f64::NAN` |
| 4 (sanitize guard) | `listener.rs` `#[cfg(test)]` | Dispatch-level test: `post_tool_use_rework_candidate` event with malformed session_id returns `HookResponse::Error { code: ERR_INVALID_PAYLOAD }` without reaching `record_rework_event` |

### Item 3 test pattern (19 NaN tests)

Each NaN test follows the pattern established for crt-046 fields (config.rs lines 8004–8094):

```rust
#[test]
fn test_nan_guard_nli_entailment_threshold() {
    let mut c = InferenceConfig::default();
    c.nli_entailment_threshold = f32::NAN;
    assert_validate_fails_with_field(&c, "nli_entailment_threshold");
}
```

Tests for the 6 fusion weight fields and 2 phase weight fields must use the exact field name
string because `assert_validate_fails_with_field` checks `err.to_string().contains(field_name)`.
Fusion and phase weight fields are in a loop — the `field` parameter in `NliFieldOutOfRange`
comes from the `&'static str` in the check array, so the field name in the error will match
the array entry exactly.

---

## Open Questions

None. All OQs from SCOPE.md resolved prior to architecture. SR-01/SR-02/SR-03 resolved in
this document.
