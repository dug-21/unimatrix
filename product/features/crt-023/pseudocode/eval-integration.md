# Eval Integration — Pseudocode

**Files**:
- `crates/unimatrix-server/src/eval/profile/layer.rs` (modified — fills W1-4 stub)
- `crates/unimatrix-server/src/eval/runner/layer.rs` (modified — adds NLI readiness wait)
- `crates/unimatrix-server/src/eval/runner/mod.rs` (modified — calls NLI wait after embed wait)

**Purpose**: Fill the `EvalServiceLayer::from_profile()` W1-4 stub. When an eval profile
specifies `nli_enabled = true` with a resolvable model, construct `NliServiceHandle` and
wire it into `SearchService`. Wait for NLI readiness (up to 60s) before replay begins.
Skip the profile with a `SKIPPED` annotation if the model fails to load (ADR-006, FR-26-29,
AC-18, AC-22).

**Critical constraint**: The eval CLI is NOT an MCP handler path. `wait_for_nli_ready()` is
a blocking poll loop — acceptable because eval runs do not have `MCP_HANDLER_TIMEOUT` constraints.

---

## `EvalProfile` Type Extension

The `EvalProfile` struct already carries `config_overrides: UnimatrixConfig`. The `UnimatrixConfig`
is extended via `InferenceConfig` (see `config-extension.md`). No new fields are needed in
`EvalProfile` itself — the NLI config is read from `profile.config_overrides.inference`.

---

## `EvalServiceLayer` Struct Extension

Add two new fields to `EvalServiceLayer`:

```
// Add to EvalServiceLayer struct:

/// crt-023: NLI handle for NLI-enabled eval profiles.
/// None when the profile has nli_enabled = false (baseline profiles).
/// Some(...) when the profile has nli_enabled = true; may be in Loading or Failed state.
///
/// Used by runner.rs to poll for NLI readiness before scenario replay begins.
pub(crate) nli_handle: Option<Arc<NliServiceHandle>>,
```

Note: A second existing `embed_handle` field already exists (holds `Arc<EmbedServiceHandle>`).
The `nli_handle` field follows the same pattern.

---

## `EvalServiceLayer::from_profile()` Extension

The existing `from_profile()` has 13 steps (C-01 through Step 13). crt-023 adds Step 2b
(NLI model path validation) and Step 6b (NLI handle construction). Steps 1-13 are unchanged
except where noted.

### Step 2b: NLI model path validation (new — fills W1-4 stub at Step 2)

Insert between existing Step 2 (inference model path validation) and Step 3 (ConfidenceWeights check):

```
// Step 2b: Validate NLI inference fields (C-14, FR-26, W1-4 stub fill)
// The existing Step 2 comment says: "When W1-4 adds nli_model, validation goes here."
// This is that fill.

let nli_cfg = &profile.config_overrides.inference

// Validate NLI fields the same as startup validation (InferenceConfig::validate pattern).
// For eval, range errors are surfaced as EvalError::InvalidProfile, not startup abort.
if nli_cfg.nli_enabled:
    // Check that nli_model_name is a recognized variant if set.
    if let Some(ref name) = nli_cfg.nli_model_name:
        if NliModel::from_config_name(name).is_none():
            return Err(EvalError::InvalidProfile {
                reason: format!(
                    "nli_model_name '{}' is not a recognized model variant; valid: minilm2, deberta",
                    name
                )
            })

    // If nli_model_path is set, check the file exists and is readable.
    // If not set, auto-resolution from cache (ensure_nli_model) happens in NliServiceHandle.
    if let Some(ref path) = nli_cfg.nli_model_path:
        if !path.exists():
            // Not an immediate EvalError — NliServiceHandle handles this by transitioning to
            // Failed, which triggers SKIPPED profile handling in run_eval_async.
            tracing::warn!(
                profile = profile.name,
                path = %path.display(),
                "eval: nli_model_path not found; profile may be SKIPPED if model unavailable"
            )
    // No error here: ADR-006 mandates SKIP behavior, not hard failure on missing model.
```

### Step 6b: NLI handle construction (new — inserted between Steps 6 and 7)

Insert between existing Step 6 (build embedding handle) and Step 7 (build inference pool):

```
// Step 6b: Build NLI handle for NLI-enabled profiles (crt-023, FR-26, ADR-006).
// Baseline profiles (nli_enabled = false) set nli_handle = None.
// The NliServiceHandle may be in Loading state; runner.rs polls for readiness.
let nli_handle: Option<Arc<NliServiceHandle>> = if profile.config_overrides.inference.nli_enabled {
    let handle = NliServiceHandle::new()

    // Extract NliConfig for this profile.
    // Cache dir: use the default cache dir (same as server startup path).
    // Eval runs use the same HuggingFace cache — model downloaded once is reused.
    let cache_dir = EmbedConfig::default().resolve_cache_dir()
    let nli_config = profile.config_overrides.inference.nli_config_for_handle(cache_dir)

    // Start background model loading. If the model file is absent:
    //   - hf-hub downloads it (if network available)
    //   - on failure: NliServiceHandle transitions to Failed
    //   - runner.rs polls and returns SKIPPED if still Failed after 60s
    handle.start_loading(nli_config)

    Some(handle)
} else {
    None  // baseline profile — no NLI handle needed
}
```

### Step 13 extension: Pass nli_handle into SearchService

The existing `ServiceLayer::with_rate_config()` call in Step 13 must be extended to pass the
NLI handle. This requires extending `ServiceLayer::with_rate_config()` to accept an optional
`nli_handle` parameter (mirroring the crt-023 changes to `ServiceLayer::new()` described in
`search-reranking.md`).

```
// Modified Step 13 call (existing parameters unchanged; nli_handle added):
let inner = ServiceLayer::with_rate_config(
    Arc::clone(&store_arc),
    Arc::clone(&vector_index),
    Arc::clone(&async_vector_store),
    Arc::clone(&store_arc),
    Arc::clone(&embed_handle),
    Arc::clone(&adapt_svc),
    Arc::clone(&audit),
    Arc::clone(&usage_dedup),
    rate_config,
    boosted_categories,
    Arc::clone(&rayon_pool),
    nli_handle.clone(),                                    // new — Option<Arc<NliServiceHandle>>
    profile.config_overrides.inference.nli_top_k,          // new — from InferenceConfig
    profile.config_overrides.inference.nli_enabled,        // new — from InferenceConfig
)
```

### Return value extension

```
Ok(EvalServiceLayer {
    inner,
    pool,
    embed_handle,
    db_path: db_resolved,
    profile_name: profile.name.clone(),
    analytics_mode: AnalyticsMode::Suppressed,
    nli_handle,     // new field
})
```

---

## `wait_for_nli_ready` (new function in `eval/runner/layer.rs`)

Follows the exact same pattern as `wait_for_embed_model` in the same file.

```
/// Maximum poll timeout for NLI model readiness in eval (ADR-006).
/// 60 seconds covers worst-case download time on a slow network connection.
const MAX_NLI_WAIT_SECS: u64 = 60

/// Poll interval for NLI model readiness.
const NLI_POLL_INTERVAL: Duration = Duration::from_millis(500)  // 500ms per poll

/// Wait for the NLI model to finish loading before scenario replay.
///
/// Polls `handle.get_provider()` until Ready or until MAX_NLI_WAIT_SECS elapses.
/// Returns Ok(()) when ready.
/// Returns Err("SKIPPED") when the model is unavailable or fails to load (ADR-006).
///
/// Called from run_eval_async ONLY when nli_handle is Some (NLI-enabled profile).
/// Never called for baseline profiles (nli_handle = None → skips this function).
pub(super) async fn wait_for_nli_ready(
    handle: &Arc<NliServiceHandle>,
    profile_name: &str,
) -> Result<(), NliNotReadyForEval>
    let deadline = tokio::time::Instant::now() + Duration::from_secs(MAX_NLI_WAIT_SECS)
    let mut attempt: u32 = 0

    loop:
        match handle.get_provider().await:
            Ok(_) ->
                tracing::debug!(
                    profile = profile_name,
                    attempt = attempt,
                    "NLI model ready for eval"
                )
                return Ok(())

            Err(ServerError::NliFailed(_)) ->
                // Permanent failure: model not found or hash mismatch or retries exhausted.
                // ADR-006: trigger SKIPPED profile handling.
                tracing::warn!(
                    profile = profile_name,
                    "eval: NLI model failed to load; profile will be SKIPPED"
                )
                return Err(NliNotReadyForEval::Failed { profile_name: profile_name.to_string() })

            Err(ServerError::NliNotReady) ->
                // Still loading — poll again if within deadline.
                if tokio::time::Instant::now() >= deadline:
                    tracing::warn!(
                        profile = profile_name,
                        timeout_secs = MAX_NLI_WAIT_SECS,
                        "eval: NLI model not ready within timeout; profile will be SKIPPED"
                    )
                    return Err(NliNotReadyForEval::Timeout { profile_name: profile_name.to_string() })

                tracing::debug!(
                    profile = profile_name,
                    attempt = attempt + 1,
                    "NLI model loading, polling"
                )
                tokio::time::sleep(NLI_POLL_INTERVAL).await
                attempt += 1

            Err(other) ->
                // Unexpected error — treat as Failed for eval purposes.
                return Err(NliNotReadyForEval::Failed { profile_name: profile_name.to_string() })


/// Reason why an NLI-enabled eval profile was skipped.
pub(super) enum NliNotReadyForEval {
    /// NLI model failed to load (missing file, hash mismatch, retries exhausted).
    Failed { profile_name: String },
    /// NLI model did not become ready within MAX_NLI_WAIT_SECS.
    Timeout { profile_name: String },
}

impl NliNotReadyForEval {
    pub fn profile_name(&self) -> &str {
        match self {
            NliNotReadyForEval::Failed { profile_name }
            | NliNotReadyForEval::Timeout { profile_name } -> profile_name.as_str()
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            NliNotReadyForEval::Failed { .. } -> "NLI model failed to load (missing or hash mismatch)",
            NliNotReadyForEval::Timeout { .. } -> "NLI model not ready within 60s timeout",
        }
    }
}
```

---

## `run_eval_async` Extension (in `eval/runner/mod.rs`)

The existing `run_eval_async` constructs one `EvalServiceLayer` per profile, waits for embed
model readiness, then replays scenarios. crt-023 inserts an NLI readiness wait after the embed
model wait. Profiles that fail NLI readiness are collected as SKIPPED rather than aborting the
entire run.

```
// Modified run_eval_async (existing structure preserved; NLI wait added):

async fn run_eval_async(
    db: &Path,
    scenarios: &Path,
    profiles: Vec<EvalProfile>,
    k: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

    let mut layers: Vec<EvalServiceLayer> = Vec::with_capacity(profiles.len())
    let mut skipped_profiles: Vec<(String, String)> = Vec::new()  // (name, reason)

    for profile in &profiles:
        eprintln!("eval run: constructing EvalServiceLayer for profile '{}'", profile.name)
        let layer = EvalServiceLayer::from_profile(db, profile, None::<&Path>)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?

        // Wait for embedding model (existing, unchanged).
        let embed = layer.embed_handle()
        layer::wait_for_embed_model(&embed, &profile.name).await?

        // crt-023: Wait for NLI model if profile is NLI-enabled (ADR-006).
        // Baseline profiles have nli_handle = None; skip this block.
        if let Some(ref nli_handle) = layer.nli_handle:
            match layer::wait_for_nli_ready(nli_handle, &profile.name).await:
                Ok(()) ->
                    // NLI ready — proceed with this profile normally.
                    layers.push(layer)
                Err(skip_reason) ->
                    // ADR-006: SKIP this profile; continue with remaining profiles.
                    // Baseline profile (index 0) never reaches this branch (nli_handle = None).
                    eprintln!(
                        "eval run: SKIPPED profile '{}' — {}",
                        skip_reason.profile_name(), skip_reason.reason()
                    )
                    skipped_profiles.push((
                        profile.name.clone(),
                        skip_reason.reason().to_string()
                    ))
                    // Do NOT push layer — this profile is excluded from replay.
        else:
            // Baseline profile (NLI disabled): no NLI wait needed.
            layers.push(layer)

    // Report skipped profiles (for eval report generator to annotate).
    // Write skipped.json to the output directory if any profiles were skipped.
    if !skipped_profiles.is_empty():
        let skipped_path = out.join("skipped.json")
        let json = serde_json::to_string_pretty(&skipped_profiles)?
        std::fs::write(&skipped_path, json)?
        eprintln!("eval run: {} profile(s) skipped; see {}", skipped_profiles.len(), skipped_path.display())

    // Guard: must have at least one layer to replay against.
    // If all NLI profiles were skipped AND the baseline is absent (should not happen),
    // abort with an informative error.
    if layers.is_empty():
        return Err("eval run: all profiles were skipped; nothing to evaluate".into())

    // Existing: load scenarios, print summary, replay.
    let scenario_records = replay::load_scenarios(scenarios)?
    eprintln!("eval run: {} profiles × {} scenarios", layers.len(), scenario_records.len())
    replay::run_replay_loop(&profiles, &layers, &scenario_records, k, out).await?

    eprintln!("eval run: complete. results in {}", out.display())
    Ok(())
```

**Note**: `profiles` and `layers` are now potentially different lengths (profiles that were
SKIPPED are in `profiles` but not `layers`). The `run_replay_loop` signature may need to
accept `&[EvalServiceLayer]` rather than using profile-index alignment. Verify the existing
`run_replay_loop` implementation in `eval/runner/replay.rs` before implementing — ensure it
does not depend on `profiles[i]` / `layers[i]` 1:1 correspondence.

---

## `EvalError` Extension

Add `InvalidProfile` variant for eval-specific validation errors (Step 2b):

```
// Add to EvalError enum (in eval/profile/error.rs):

/// An eval profile TOML contains an invalid configuration value.
InvalidProfile {
    reason: String,
}
```

Add to `EvalError` Display impl:
```
InvalidProfile { reason } -> "invalid eval profile: {}", reason
```

---

## Eval Profile TOML Format Extension

The eval profile TOML format (documented in `EvalProfile` docstring in types.rs) is extended
to support NLI fields under the `[inference]` section:

```toml
[profile]
name = "candidate-nli-minilm2"
description = "NLI cross-encoder re-ranking with MiniLM2"

[confidence.weights]
# ... existing weight fields if desired ...

[inference]
nli_enabled = true
nli_model_name = "minilm2"
# nli_model_path = "/path/to/model.onnx"  # override cache resolution if needed
# nli_model_sha256 = "abc123..."           # hash from model-download --nli output
nli_top_k = 20
nli_contradiction_threshold = 0.6
nli_entailment_threshold = 0.6
max_contradicts_per_tick = 10
```

Baseline profile TOML (no NLI fields — all defaults):
```toml
[profile]
name = "baseline"
description = "Cosine-only baseline (no NLI)"
# No [inference] section needed — nli_enabled defaults to true,
# BUT NliServiceHandle will fail gracefully if model absent,
# and the baseline should explicitly disable NLI:

[inference]
nli_enabled = false
```

---

## `ServiceLayer::with_rate_config` Extension

`ServiceLayer::with_rate_config()` is the constructor called by `EvalServiceLayer::from_profile()`.
It must be extended to accept NLI wiring parameters (same change as the server startup path):

```
// Modified with_rate_config signature (in services/mod.rs):
pub(crate) fn with_rate_config(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    audit: Arc<AuditLog>,
    usage_dedup: Arc<UsageDedup>,
    rate_config: RateLimitConfig,
    boosted_categories: HashSet<String>,
    ml_inference_pool: Arc<RayonPool>,
    nli_handle: Option<Arc<NliServiceHandle>>,   // new — None for baseline profiles
    nli_top_k: usize,                             // new — from InferenceConfig
    nli_enabled: bool,                            // new — from InferenceConfig
) -> Self
```

The `SearchService::new()` call inside `with_rate_config()` must also be extended to pass these
through (see `search-reranking.md` pseudocode for the SearchService::new signature).

**Note for implementation**: The `nli_handle` is `Option<Arc<NliServiceHandle>>` in the eval
path (None for baseline profiles), but `Arc<NliServiceHandle>` in the server startup path
(always constructed, regardless of nli_enabled). The `SearchService` struct holds
`Arc<NliServiceHandle>` (not Option). For eval baseline profiles, pass a
`NliServiceHandle::new()` handle that was never started (get_provider() returns NliNotReady
immediately). This avoids special-casing in `SearchService::search`.

Revised approach for eval:
```
let nli_handle_arc: Arc<NliServiceHandle> = match nli_handle {
    Some(h) -> h       // NLI-enabled profile: use the loaded handle
    None    -> NliServiceHandle::new()  // Baseline profile: unstarted handle → NliNotReady
}
```

---

## Error Handling

| Condition | Log Level | Behavior |
|-----------|-----------|----------|
| `nli_model_name` unrecognized in profile | n/a | `EvalError::InvalidProfile` (immediate return) |
| `nli_model_path` not found at validation time | warn | Proceed; handle transitions to Failed during load |
| NLI handle transitions to Failed during wait | warn | Profile marked SKIPPED; skipped.json written |
| NLI model not ready within 60s | warn | Profile marked SKIPPED; skipped.json written |
| All NLI profiles skipped, baseline remains | info | Eval proceeds on baseline only; human reviewer sees SKIPPED annotation |
| ALL profiles skipped (no layers) | error | Abort with descriptive error message |
| Baseline profile (nli_enabled = false) | n/a | No NLI wait; NliServiceHandle::new() (unstarted) passed to SearchService |

---

## Key Test Scenarios

1. **AC-18 / NLI-enabled profile wiring**: Create two eval profiles (baseline: nli_enabled=false, candidate: nli_enabled=true, nli_model_name="minilm2"); run `eval run` with a fixture snapshot; assert two result files produced in output directory (baseline and candidate).
2. **AC-18 / NLI handle is in SearchService**: With NLI-enabled eval profile, confirm `SearchService.nli_handle.get_provider()` returns Ok (NLI Ready) after the 60s wait window.
3. **AC-22 / gate waiver**: Run `eval run` with zero eval scenarios; assert delivery report documents waiver with reason "no query history available".
4. **ADR-006 / model missing → SKIPPED**: Create candidate profile with `nli_enabled=true` but no model file in cache and no network; run `eval run`; assert: (a) baseline profile runs; (b) candidate profile is skipped with SKIPPED annotation in skipped.json; (c) `eval run` exits 0 (not error).
5. **ADR-006 / model load timeout → SKIPPED**: Mock NLI loading to never complete; run `eval run`; assert candidate profile is marked SKIPPED after 60s timeout and baseline continues.
6. **ADR-006 / baseline never SKIPPED**: Baseline profile always uses `nli_handle = None` path; assert it never enters the NLI wait block regardless of model state.
7. **FR-29 / eval gate waiver documentation**: When `eval scenarios` returns zero rows, assert the delivery report includes the waiver entry and AC-01 still passes.
8. **EvalError::InvalidProfile**: Profile with `nli_model_name = "gpt4"` returns `EvalError::InvalidProfile` at `from_profile()` time (before any model loading).
9. **ServiceLayer baseline wiring**: Baseline profile uses unstarted `NliServiceHandle::new()` — assert `SearchService.get_provider()` returns `Err(NliNotReady)` and search uses `rerank_score` fallback.
10. **AC-09 / eval gate**: Full integration test running both profiles against real snapshot; assert aggregate MRR for candidate >= baseline (this is the human-reviewed gate scenario).
