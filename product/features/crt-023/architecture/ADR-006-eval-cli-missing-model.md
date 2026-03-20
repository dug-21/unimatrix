## ADR-006: Eval CLI Missing-Model Behavior — Skip Profile with Documented Warning

### Context

SR-08 and OQ-04 ask: when `unimatrix eval run` is given a candidate profile with `nli_enabled = true` but the model file is absent (no network in CI, no cached model), should it:
(a) Abort with error — stops the entire eval run
(b) Fall back to cosine for that profile — makes the NLI comparison invalid
(c) Require a `--nli-model-path` flag — CLI design change
(d) Skip that profile with a warning — produce partial results

NFR-12 already specifies option (d): "skip that profile with a documented warning in the report, rather than aborting the entire eval run invocation."

The `EvalServiceLayer::from_profile()` stub (which crt-023 fills in) constructs `NliServiceHandle`. The handle's `Loading → Failed` transition naturally handles absent model files — the handle transitions to `Failed` during construction if the model file cannot be found and `nli_enabled = true`.

The issue is what `unimatrix eval run` does when an `EvalServiceLayer` for a profile has `NliServiceHandle` in `Failed` state at eval-start time.

**Options for eval run behavior:**

Option A (skip profile): If `NliServiceHandle` is in `Failed` state at the time `eval run` begins executing scenarios for a profile, skip the profile entirely. Record a `SKIPPED: NLI model not available` entry in the report for that profile. Other profiles continue normally.

Option B (run with degradation): Run the profile anyway; NLI-enabled profile silently falls back to cosine (because `get_provider()` returns `Err`). The results are identical to the baseline profile. This makes the comparison meaningless.

Option C (download on demand): If the model is absent and network is available, `eval run` triggers a download before running. If network is unavailable, fall back to option A.

Option B is deceptive: the report shows "candidate NLI profile" results that are actually baseline cosine results. This is worse than no data. Option C adds complexity to the eval CLI and creates unpredictable behavior in CI environments.

Option A is the correct choice: a skipped profile with an explicit warning is honest and clear. The operator knows they need to download the model before running the candidate comparison. The baseline profile (NLI disabled) still runs and produces valid results.

**Implementation:**

`EvalServiceLayer::from_profile()` is called with a profile TOML. If `nli_enabled = true` and the model file is absent, the construction path calls `NliServiceHandle::start_loading()` which spawns a background load. By eval run time (synchronous eval scenario execution follows), the handle may have transitioned to `Failed`.

For the eval CLI specifically, the handle load should be awaited synchronously (blocking until Ready or Failed) before the first scenario is executed. This is acceptable in the eval CLI path — it is not an MCP handler path and does not have the `MCP_HANDLER_TIMEOUT` constraint.

The `EvalServiceLayer` gains a method `wait_for_nli_ready(timeout: Duration) -> Result<(), NliNotReadyError>` that polls `NliServiceHandle::get_provider()` until ready or timeout. If the timeout fires and the handle is still `Loading` (slow download) or `Failed` (model absent), the profile is skipped.

### Decision

**Skip the profile with a documented warning when the NLI model is absent or failed to load.** The warning appears in the `unimatrix eval report` output as a `SKIPPED` entry for the affected profile.

**`EvalServiceLayer` waits for NLI readiness** (up to a configurable timeout, default 60s) before beginning scenario execution. If the model is cached locally, this is near-instant. If download is needed and network is available, the 60s window covers the download. If download fails or times out, the profile is marked SKIPPED.

**The baseline profile (NLI disabled)** is never skipped due to NLI model issues — it does not use `NliServiceHandle` at all.

**Partial results are valid for the eval gate.** AC-09 / FR-28: "for the available profiles" — a SKIPPED profile is not an available profile. The gate condition applies only to profiles that executed. If only baseline executed (both NLI candidate profiles were skipped), the gate is effectively waived per D-01 (no NLI evidence available), with the SKIPPED entries as the documented reason.

**No `--skip-profile-on-missing-model` flag is needed** — this is the default behavior, not an opt-in. A `--require-all-profiles` flag could be added in a follow-on feature for CI environments that want hard failure on missing models.

### Consequences

**Easier:**
- CI environments without cached models get partial results rather than a hard failure.
- Eval report explicitly shows SKIPPED with reason, making the situation visible to reviewers.
- Baseline profile always runs; any regression introduced by the code change (not NLI) is detectable.

**Harder:**
- `EvalServiceLayer` must implement the NLI readiness wait. This is new logic in the eval path.
- Profile skipping must be propagated through the eval report format — the report generator must handle missing profiles gracefully.
- Operators may misread "baseline profile passed" as "NLI validated" when NLI was actually skipped. The SKIPPED annotation in the report is the mitigation.
