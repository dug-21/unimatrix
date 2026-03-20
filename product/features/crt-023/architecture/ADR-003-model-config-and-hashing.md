## ADR-003: Config-Driven NLI Model Selection with Per-Variant Hash Pinning

### Context

D-03 in SCOPE.md requires: (a) model selection via configuration string rather than hardcoded model choice, and (b) a 3-profile eval comparing baseline, MiniLM2, and deberta-v3-small. The eval harness (nan-007) uses profile TOML files with `[inference]` section overrides.

Two design tensions exist:

**Tension 1: Config-string model selection vs hash pinning.** If the operator selects `nli_model_name = "deberta"` but the `nli_model_sha256` field is a single value, it cannot simultaneously pin two different models. Options:
- A single `nli_model_sha256` field: binds one hash per config file. For multi-model eval, each profile TOML has its own hash. This is simple but means the hash is per-config, not per-model-variant.
- A per-variant hash map in config (`nli_model_hashes = { minilm2 = "abc...", deberta = "def..." }`): flexible but complex TOML syntax, unusual for this codebase.
- Single `nli_model_sha256` + `nli_model_name`: the hash refers to the specific model selected in this config file. Each eval profile includes its own hash. Production config has exactly one model and one hash.

**Tension 2: `nli_model_path` vs `nli_model_name`.** The operator may provide an absolute path to a cached ONNX file (bypassing HuggingFace Hub), or specify a model name that resolves to the standard cache directory. These are independent: `nli_model_path` overrides the resolved path; `nli_model_name` selects the HF model ID and cache subdir. Both can coexist: `nli_model_name = "minilm2"` + `nli_model_path = "/opt/models/nli.onnx"` uses the MiniLM2 tokenizer config with the specified ONNX file.

**Deberta ONNX availability (SR-01 / OQ-05):**

`cross-encoder/nli-deberta-v3-small` on HuggingFace Hub — the repository exists at `cross-encoder/nli-deberta-v3-small` but ONNX export availability must be verified at implementation time. The `optimum` library is the standard export tool; some model repos include pre-exported ONNX files in an `onnx/` subdirectory. The architect notes that:

- `cross-encoder/nli-MiniLM2-L6-H768` has ONNX exports confirmed available (stated in SCOPE.md background research).
- `cross-encoder/nli-deberta-v3-small` ONNX availability is NOT confirmed. DeBERTa-v3's architecture uses disentangled attention with position embeddings that require specific ONNX export flags; some exports exist but are not universally available on all model repos.

The `NliDebertaV3Small` enum variant MUST be implemented for future use, but if its ONNX file is absent from the Hub at implementation time, the 3-profile eval degrades to 2-profile and this is explicitly documented in the delivery report. The `NliModel::onnx_filename()` implementation for `NliDebertaV3Small` should return `"model.onnx"` (the standard optimum export filename) as the best-effort value, with a note that the implementer must verify the actual filename at download time.

**NliModel enum design:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NliModel {
    /// cross-encoder/nli-MiniLM2-L6-H768 (~85MB, Apache 2.0)
    /// ONNX export confirmed available.
    NliMiniLM2L6H768,
    /// cross-encoder/nli-deberta-v3-small (~180MB)
    /// ONNX availability must be verified at implementation time (SR-01).
    NliDebertaV3Small,
}

impl NliModel {
    pub fn from_config_name(name: &str) -> Option<Self> {
        match name {
            "minilm2" => Some(Self::NliMiniLM2L6H768),
            "deberta"  => Some(Self::NliDebertaV3Small),
            _ => None,
        }
    }

    pub fn model_id(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768  => "cross-encoder/nli-MiniLM2-L6-H768",
            Self::NliDebertaV3Small => "cross-encoder/nli-deberta-v3-small",
        }
    }

    pub fn onnx_repo_path(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768  => "cross-encoder/nli-MiniLM2-L6-H768",
            Self::NliDebertaV3Small => "cross-encoder/nli-deberta-v3-small",
        }
    }

    pub fn onnx_filename(&self) -> &'static str {
        // Both use the standard optimum-exported filename.
        // Implementer must verify actual filename for deberta at download time.
        "model.onnx"
    }

    pub fn cache_subdir(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768  => "nli-minilm2-l6-h768",
            Self::NliDebertaV3Small => "nli-deberta-v3-small",
        }
    }
}
```

**Config fields (final set):**

```toml
[inference]
nli_enabled = true                    # bool, default true
nli_model_name = "minilm2"            # Option<String>, default None (resolves to NliMiniLM2L6H768)
nli_model_path = "/path/to/model.onnx"  # Option<PathBuf>, default None (uses cache dir)
nli_model_sha256 = "abc123...64chars"   # Option<String>, default None
nli_top_k = 20                        # usize, range [1, 100]
nli_post_store_k = 10                 # usize, range [1, 100]
nli_entailment_threshold = 0.6        # f32, range (0.0, 1.0)
nli_contradiction_threshold = 0.6     # f32, range (0.0, 1.0)
max_contradicts_per_tick = 10         # usize, range [1, 100]; per-call cap on context_store
```

When `nli_model_name` is absent, the default resolves to `NliMiniLM2L6H768`. This matches D-03's requirement: the primary model is MiniLM2; operators swap to deberta by changing `nli_model_name = "deberta"` without code changes.

### Decision

**Single `nli_model_sha256` field, per-config-file binding.** Each `config.toml` (including each eval profile TOML) specifies its own hash for the model it selects. Multi-model eval uses separate profile files, each with its own hash. This is consistent with how the eval harness already works (separate baseline/candidate TOML files).

**`NliModel::from_config_name` validates model name strings at startup.** An unrecognized string (e.g., `nli_model_name = "gpt4"`) fails the `InferenceConfig::validate()` check with a structured error. Valid identifiers are `"minilm2"` and `"deberta"`.

**`NliDebertaV3Small` variant implemented unconditionally.** It is present in the enum and reachable via config even if the ONNX file is not cached. If selected and the ONNX file is absent, `NliServiceHandle` transitions to `Failed` with a clear error message noting the file was not found. This is the same failure path as any missing model.

**SHA-256 hash verification order**: hash check happens before `Session::builder().commit_from_file()`. The ONNX file is read once to compute the hash; if it matches, the same file path is passed to `Session::builder()`. If the hash does not match, `NliServiceHandle` transitions to `Failed` and logs a `tracing::error!` containing both "security" and "hash mismatch". The server continues on cosine fallback.

**`unimatrix model-download --nli`** downloads the model selected by `nli_model_name` (default: MiniLM2), prints its SHA-256 hash, and exits. The operator copies the hash into `config.toml`. If `--nli-model = deberta` is passed to the download command, it downloads deberta instead.

### Consequences

**Easier:**
- Config-string model selection enables model swap with a single `config.toml` edit.
- Per-config-file hash binding is simple: one model, one hash, one config file. No nested TOML structures.
- Eval multi-profile comparison works naturally: each profile TOML has `nli_model_name` + `nli_model_sha256`.
- `NliDebertaV3Small` variant is future-proof without inflating complexity now.

**Harder:**
- If deberta ONNX is unavailable on HuggingFace Hub at implementation time, the 3-profile eval cannot run. Implementer must verify at download time and document the finding. The feature ships with 2-profile eval if deberta is unavailable.
- Hash pinning is per-file, not per-model in a registry. Rotating the model file requires updating `nli_model_sha256` in every config file that references it. For most deployments (one production config), this is a single edit.
- `from_config_name` must be updated when new model variants are added. This is a small maintenance cost.
