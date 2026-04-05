## ADR-001: bugfix-523 Hardening Batch — Gate Placement, NaN Guard Scope, and Log Test Strategy

### Context

bugfix-523 is a 4-item hardening batch in `unimatrix-server`. Three architectural decisions
in this batch required explicit resolution before implementation could proceed:

**(a) NLI gate placement** — Item 1 requires a `nli_enabled` config check to be inserted
inside `run_graph_inference_tick`. The existing control flow has an implicit gate via
`get_provider()` returning `Err` when `nli_enabled=false`, but this still executes the async
call. crt-039 ADR-001 (entry #4017) established a structural invariant: Phase A (Informs) and
Path C (cosine Supports) are unconditional; the outer `run_graph_inference_tick` call in
`background.rs` must remain unconditional. The gate must be inside the function at the Path B
boundary only. Inserting it at the wrong location (before `run_cosine_supports_path`) would
violate ADR-001 and silently disable cosine Supports edge accumulation in production.

**(b) 19-field NaN guard scope** — Item 3 requires `!v.is_finite()` guards on float fields
in `InferenceConfig::validate()`. SCOPE.md OQ-01 asked whether to include the 6 fusion weight
fields (`w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`) and 2 phase weight fields
(`w_phase_histogram`, `w_phase_explicit`) in addition to the 11 threshold fields. These 8
fields use a loop-based check with a comparison-only guard; the sum constraint does not catch
NaN (because `NaN > 1.0` is false under IEEE 754). OQ-01 was resolved in SCOPE.md but the
architecture must record the explicit decision.

**(c) Log level test strategy** — Items 1 and 2 introduce `tracing::debug!` calls whose level
is an acceptance criterion. SR-03 in the risk assessment flags that log-level ACs have blocked
gates in prior features (lesson #3935). A choice between full tracing assertion (via
`tracing-test`) and behavioral-only coverage must be made explicitly before delivery.

### Decision

**(a) NLI gate placement**

Insert `if !config.nli_enabled { return; }` at the PATH B entry gate, defined structurally as:
after the `run_cosine_supports_path(...)` call completes and after the
`if candidate_pairs.is_empty() { return; }` fast-exit, before the
`let provider = match nli_handle.get_provider().await` call.

The comment block at this location already labels it "=== PATH B entry gate ===". This
placement satisfies ADR-001 (entry #4017) because:
- Phase A (Informs write loop via Phase 4b) executes before this site.
- Path C (`run_cosine_supports_path`) executes before this site.
- Only `get_provider()`, Phase 6 text fetch, Phase 7 rayon dispatch, and Phase 8 Supports
  writes are gated. These are all Path B.
- `background.rs` call site is unchanged.

The gate emits a distinct `tracing::debug!` message to allow operator disambiguation:
`"graph inference tick: NLI disabled by config; Path B skipped"` (vs. the existing
`"graph inference tick: NLI provider not ready; Supports path skipped"` from `get_provider()` Err).

**(b) 19-field NaN guard scope**

Include all 19 float fields: 11 threshold fields (individual checks), 6 fusion weight fields
(loop), and 2 phase weight fields (loop). Rationale:

- IEEE 754: `NaN > 1.0` is false. A NaN fusion weight passes the current loop guard
  (`*value < 0.0 || *value > 1.0`). The sum check computes `NaN + ... = NaN`, and
  `NaN > 1.0` is false, so `FusionWeightSumExceeded` does not fire. NaN propagates silently
  into every search scoring call until server restart.
- The error produced without the is_finite guard would be `FusionWeightSumExceeded`
  (misleading) or no error at all (silent). The per-field `NliFieldOutOfRange` with the
  correct field name is strictly better.
- The fix is a one-token change per loop guard (`!value.is_finite() || ` prefix), mechanical
  and low-risk.
- The 11 threshold fields are f32 (NLI/cosine thresholds) and f64 (PPR weights). Pattern
  is identical: `!v.is_finite() || <existing comparison>`. No new error variant is needed;
  `ConfigError::NliFieldOutOfRange` is the established variant (lesson #4132).

**(c) Log level test strategy**

Behavioral-only coverage. Log level is NOT asserted in tests for Items 1 and 2.

Tests will:
- For Item 1 (AC-01/AC-02): Assert that with `nli_enabled=false`, Supports edges are not
  written and Path A (Informs) edges are present. This is a behavioral proxy for the gate
  firing correctly.
- For Item 2 (AC-04/AC-05): Assert that `run_cosine_supports_path` skips pairs with absent
  category_map entries (function returns, pair is not written, no panic). The non-finite
  cosine `warn!` stays as `warn!` — tested by asserting the pair is also skipped.

Log level assertions via `tracing-test` harness are explicitly excluded because:
1. Lesson #3935: subscriber state leakage and initialization conflicts in parallel tests have
   caused Gate 3b failures in prior features.
2. The log level change is a two-line edit with zero branching logic. The behavioral invariant
   (skip, no write, no panic) is the safety property; the level is observability-only.
3. Adding `tracing-test` as a dev-dependency for two assertions in one batch is not justified
   by the risk reduction achieved.

Gate 3b reviewers: AC-04 and AC-05 are accepted as behavioral-only per this ADR. Log level
assertions would require the `tracing-test` harness and are deferred unless a future feature
makes this a systemic concern.

### Consequences

- Path A (Informs) and Path C (cosine Supports) run unconditionally on every tick in all
  production configurations. No edge accumulation regression.
- With `nli_enabled=false` (production default): the `get_provider()` async call is never
  made; rayon thread pool receives no dispatches from this function; 353-second tick
  congestion scenario is eliminated.
- With `nli_enabled=true` and provider available: full NLI path executes unchanged. No
  regression in the NLI-enabled path.
- NaN or ±Inf in any of the 19 `InferenceConfig` float fields will cause server startup
  failure with a specific field name in the error message. Silent NaN propagation into
  scoring pipelines is impossible from these fields.
- The sum-of-six constraint (`FusionWeightSumExceeded`) is not removed; it remains as a
  second line of defence for valid-but-wrong weight combinations. The NaN guard is an
  earlier, per-field check.
- Log level assertions for Items 1/2 are acknowledged untested. Any future feature adding
  `tracing-test` to the test harness should backfill these ACs.
- No API surface changes. No schema changes. No new dependencies.

### Related

- crt-039 ADR-001 (entry #4017): defines Path A/Path C unconditional invariant
- Lesson #4132 (entry #4132): NaN trap pattern for InferenceConfig fields
- Pattern #3921 (entry #3921): sanitize_session_id consistency rule for UDS dispatch arms
- Log level semantic contract (entry #3467): warn for anomalies, debug for expected degraded mode
