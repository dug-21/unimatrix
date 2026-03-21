# NliProvider — Pseudocode

**Files**: `crates/unimatrix-embed/src/cross_encoder.rs` (new),
           `crates/unimatrix-embed/src/model.rs` (extended),
           `crates/unimatrix-embed/src/download.rs` (extended),
           `crates/unimatrix-embed/src/lib.rs` (module export)

**Purpose**: ONNX cross-encoder inference. Takes `(query, passage)` string pairs, tokenizes
them as a concatenated cross-encoder input, runs ONNX inference, applies softmax to the 3-element
logit output, and returns `NliScores`. Enforces per-side input truncation as a security
requirement (NFR-08). Mirrors `OnnxProvider` structure exactly but differs in output processing
(softmax over 3 logits, not mean-pool + L2-normalize over hidden states).

---

## model.rs Extension: `NliModel` enum

Add alongside existing `EmbeddingModel` enum. No changes to `EmbeddingModel`.

```
// Placed after EmbeddingModel in model.rs

/// Catalog of known NLI cross-encoder ONNX model variants.
/// Mirrors EmbeddingModel conventions: model_id, onnx_repo_path, onnx_filename, cache_subdir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NliModel {
    /// cross-encoder/nli-MiniLM2-L6-H768
    /// ~85MB, Apache 2.0. ONNX export confirmed available. Primary model.
    NliMiniLM2L6H768,
    /// cross-encoder/nli-deberta-v3-small
    /// ~180MB. ONNX availability must be verified at implementation time (SR-01, ADR-003).
    NliDebertaV3Small,
}

impl NliModel {
    /// Resolve from config string identifier. Returns None for unrecognized values.
    /// Called by InferenceConfig::validate(); None triggers startup abort (AC-17, R-15).
    fn from_config_name(name: &str) -> Option<Self>
        "minilm2" -> Some(NliMiniLM2L6H768)
        "deberta"  -> Some(NliDebertaV3Small)
        _          -> None

    /// HuggingFace model repository ID.
    fn model_id(&self) -> &'static str
        NliMiniLM2L6H768  -> "cross-encoder/nli-MiniLM2-L6-H768"
        NliDebertaV3Small -> "cross-encoder/nli-deberta-v3-small"

    /// Repo path for ONNX file download via hf-hub.
    /// Same value as model_id for these models (no onnx/ subdirectory prefix).
    fn onnx_repo_path(&self) -> &'static str
        NliMiniLM2L6H768  -> "cross-encoder/nli-MiniLM2-L6-H768"
        NliDebertaV3Small -> "cross-encoder/nli-deberta-v3-small"

    /// Local ONNX filename. Both use standard optimum-exported filename.
    /// Implementer must verify actual filename for deberta at download time (ADR-003).
    fn onnx_filename(&self) -> &'static str
        _ -> "model.onnx"

    /// Local cache subdirectory name (no slashes — safe for filesystem paths).
    fn cache_subdir(&self) -> &'static str
        NliMiniLM2L6H768  -> "nli-minilm2-l6-h768"
        NliDebertaV3Small -> "nli-deberta-v3-small"
}
```

**Key test scenarios for NliModel**:
- `from_config_name("minilm2")` returns `Some(NliMiniLM2L6H768)` (AC-21)
- `from_config_name("deberta")` returns `Some(NliDebertaV3Small)`
- `from_config_name("gpt4")` returns `None` (R-15)
- `cache_subdir()` does not contain '/' for any variant (R-18 tokenizer path isolation)
- `cache_subdir()` values are distinct for the two variants (R-18)

---

## download.rs Extension: `ensure_nli_model`

Add alongside existing `ensure_model`. Pattern is identical; no changes to `ensure_model`.

```
/// Ensure NLI model files (ONNX model + tokenizer.json) exist in cache directory.
/// Downloads from HuggingFace Hub via hf-hub if not cached.
/// Returns path to model directory containing model.onnx and tokenizer.json.
fn ensure_nli_model(model: NliModel, cache_dir: &Path) -> Result<PathBuf>
    model_dir = cache_dir.join(model.cache_subdir())
    onnx_path = model_dir.join(model.onnx_filename())
    tokenizer_path = model_dir.join("tokenizer.json")

    if onnx_path.exists() AND tokenizer_path.exists()
       AND file_size(onnx_path) > 0 AND file_size(tokenizer_path) > 0:
        return Ok(model_dir)  // already cached

    fs::create_dir_all(model_dir)?

    api = hf_hub::api::sync::Api::new()
           .map_err(|e| EmbedError::Download(...))?
    repo = api.model(model.model_id().to_string())

    // Download ONNX model file
    downloaded_onnx = repo.get(model.onnx_repo_path())
                          .map_err(|e| EmbedError::Download(...))?

    // Download tokenizer.json from the same repo (R-18: tokenizer must match model)
    downloaded_tokenizer = repo.get("tokenizer.json")
                               .map_err(|e| EmbedError::Download(...))?

    // Copy from hf-hub cache to our cache dir if paths differ
    if downloaded_onnx != onnx_path:
        fs::copy(downloaded_onnx, onnx_path)?
    if downloaded_tokenizer != tokenizer_path:
        fs::copy(downloaded_tokenizer, tokenizer_path)?

    // Validate non-empty after copy
    if !onnx_path.exists() OR file_size(onnx_path) == 0:
        return Err(EmbedError::ModelNotFound { path: onnx_path })
    if !tokenizer_path.exists() OR file_size(tokenizer_path) == 0:
        return Err(EmbedError::ModelNotFound { path: tokenizer_path })

    Ok(model_dir)
```

---

## cross_encoder.rs: `NliScores`, `CrossEncoderProvider`, `NliProvider`

This is a new file. All three items live here.

### `NliScores`

```
/// Normalized NLI classification output. Sum of three fields ≈ 1.0 (within 1e-4).
struct NliScores {
    entailment:    f32,   // P(premise entails hypothesis)
    neutral:       f32,   // P(premise and hypothesis are unrelated)
    contradiction: f32,   // P(premise contradicts hypothesis)
}
```

### `CrossEncoderProvider` trait

```
/// Abstraction over any NLI cross-encoder ONNX model.
/// Both methods are SYNCHRONOUS — implementations are called from rayon threads (W1-2).
/// Implementations must be Send + Sync.
trait CrossEncoderProvider: Send + Sync {
    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>
    fn name(&self) -> &str
}
```

### `NliProvider` struct

```
/// ONNX-backed NliProvider. Mirrors OnnxProvider structure (ADR-001).
/// Thread-safe via Mutex<Session> for inference serialization.
/// Tokenizer is outside mutex for lock-free tokenization.
struct NliProvider {
    session:    Mutex<Session>,  // ort Session — single shared instance
    tokenizer:  Tokenizer,       // tokenizers crate — lock-free
    model_name: String,          // for CrossEncoderProvider::name()
}
```

### `NliProvider::new`

```
fn new(model: NliModel, model_path: &Path) -> Result<Self>
    // Called from NliServiceHandle spawn_blocking load task.
    // model_path is the directory containing model.onnx and tokenizer.json.
    // Hash verification happens BEFORE this call in NliServiceHandle (ADR-003).

    tokenizer_path = model_path.join("tokenizer.json")
    tokenizer = Tokenizer::from_file(tokenizer_path)
                    .map_err(|e| EmbedError::Tokenizer(...))?

    // Configure truncation to model's max_position_embeddings.
    // For MiniLM2: 512 tokens. The truncation here is for the tokenizer's internal
    // per-sequence limit. Per-side pre-truncation (NFR-08) happens in score_batch.
    truncation = TruncationParams {
        max_length: 512,
        strategy: LongestFirst,
        ..Default::default()
    }
    tokenizer.with_truncation(Some(truncation))?

    padding = PaddingParams {
        strategy: BatchLongest,
        ..Default::default()
    }
    tokenizer.with_padding(Some(padding))

    onnx_path = model_path.join(model.onnx_filename())
    session = Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .commit_from_file(onnx_path)?

    Ok(NliProvider {
        session: Mutex::new(session),
        tokenizer,
        model_name: model.model_id().to_string(),
    })
```

### Per-side truncation helper

```
/// Enforce per-side input truncation before pair tokenization.
/// 512 tokens or ~2000 characters, whichever fires first (NFR-08, FR-06).
/// Silent — no error or warning per call. Security requirement.
fn truncate_input(text: &str) -> &str
    // Approximate character limit: 2000 chars (safe proxy for 512 tokens on most tokenizers)
    const CHAR_LIMIT: usize = 2000;
    if text.len() <= CHAR_LIMIT:
        return text
    // Truncate at char boundary (not byte boundary) to preserve UTF-8 validity
    // Find the last char boundary at or before CHAR_LIMIT bytes
    let mut end = CHAR_LIMIT.min(text.len());
    while !text.is_char_boundary(end):
        end -= 1
    &text[..end]

    // NOTE: Token-level truncation is also enforced by the tokenizer's TruncationParams.
    // The character pre-check prevents sending excessively large inputs to the tokenizer
    // itself (OOM risk for adversarial inputs exceeding 100,000 chars).
```

### `CrossEncoderProvider` implementation for `NliProvider`

```
impl CrossEncoderProvider for NliProvider {
    fn name(&self) -> &str
        &self.model_name

    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>
        // Delegate to score_batch for code reuse.
        let scores = self.score_batch(&[(query, passage)])?
        // score_batch guarantees len == input len for non-empty input.
        scores.into_iter().next()
              .ok_or(EmbedError::InferenceFailed("empty batch result".into()))

    /// Full batch scoring. All pairs run under a single Mutex<Session> acquisition (ADR-001).
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>
        if pairs.is_empty():
            return Ok(vec![])
            // IMPORTANT: score_batch(&[]) must return Ok(vec![]) not an ORT session error.
            // Empty batch edge case documented in RISK-TEST-STRATEGY.

        // Step 1: Per-side truncation BEFORE tokenization (NFR-08, security, R-19).
        // Applied to each side independently. The combined pair may still be close to the
        // 512-token model limit after truncation; the tokenizer TruncationParams handle
        // the final combined sequence length.
        truncated_pairs: Vec<(String, String)> = pairs.iter().map(|(q, p)| {
            (truncate_input(q).to_string(), truncate_input(p).to_string())
        }).collect()

        // Step 2: Tokenize all pairs outside the mutex lock (lock-free).
        // Cross-encoder concatenation: "[CLS] query [SEP] passage [SEP]"
        // The tokenizer handles this automatically when is_pair=true.
        pair_refs: Vec<(&str, &str)> = truncated_pairs.iter()
            .map(|(q, p)| (q.as_str(), p.as_str()))
            .collect()

        // encode_batch_char_offsets for pairs:
        encodings = self.tokenizer
                        .encode_batch(
                            pair_refs.iter().map(|(q, p)| {
                                // tokenizers crate supports (text_a, text_b) pair encoding
                                tokenizers::EncodeInput::Dual(
                                    tokenizers::InputSequence::from(q.as_ref()),
                                    tokenizers::InputSequence::from(p.as_ref()),
                                )
                            }).collect(),
                            true,  // add_special_tokens = true
                        )
                        .map_err(|e| EmbedError::Tokenizer(format!("{e}")))?

        let batch_size = encodings.len()
        let seq_len = encodings[0].get_ids().len()  // padded to longest in batch

        // Step 3: Flatten to contiguous [batch_size, seq_len] arrays (i64 for ONNX)
        input_ids_flat     = flatten_encodings_ids(&encodings, batch_size, seq_len)
        attention_mask_flat = flatten_encodings_mask(&encodings, batch_size, seq_len)
        // token_type_ids: all zeros for models that don't use segment embeddings.
        // For MiniLM2: token_type_ids needed; cross-encoder models typically set
        // token_type_ids to 0 for query and 1 for passage. The tokenizer sets this.
        token_type_ids_flat = flatten_encodings_type_ids(&encodings, batch_size, seq_len)

        shape = [batch_size as i64, seq_len as i64]
        ids_tensor   = Tensor::from_array((shape, input_ids_flat))?
        mask_tensor  = Tensor::from_array((shape, attention_mask_flat))?
        types_tensor = Tensor::from_array((shape, token_type_ids_flat))?

        inputs = ort::inputs![
            "input_ids"      => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => types_tensor,
        ]?

        // Step 4: Acquire mutex, run inference, release mutex.
        // Tokenization is done (lock-free). Entire batch runs under ONE lock acquisition.
        logits_flat: Vec<f32> = {
            let session = self.session.lock()
                .map_err(|_| EmbedError::InferenceFailed("session mutex poisoned".into()))?
            let outputs = session.run(inputs)?

            // Output shape: [batch_size, 3]
            // 3 classes: [entailment, neutral, contradiction] (model-specific ordering;
            // for MiniLM2 verify from model card: entailment=0, neutral=1, contradiction=2)
            let output_value = &outputs[0]
            let (_shape, data) = output_value.try_extract_raw_tensor::<f32>()?

            // Validate shape: must be [batch_size, 3]
            if _shape.len() != 2 OR _shape[0] != batch_size as i64 OR _shape[1] != 3:
                return Err(EmbedError::DimensionMismatch { expected: 3, got: _shape[1] as usize })

            data.to_vec()
            // Session lock released here (explicit drop at end of block)
        }

        // Step 5: Softmax per sample (outside the mutex lock).
        // Softmax must be numerically stable (subtract max before exp to prevent overflow).
        let scores: Vec<NliScores> = logits_flat
            .chunks(3)
            .map(|logits| softmax_3class(logits))
            .collect()

        Ok(scores)
}

/// Numerically stable softmax over 3 logits.
/// max-subtraction before exp prevents overflow for large logits (edge case in RISK-TEST-STRATEGY).
fn softmax_3class(logits: &[f32]) -> NliScores
    // logits: [entailment_logit, neutral_logit, contradiction_logit]
    // NOTE: MiniLM2 label order must be verified from model config at implementation time.
    // The order here assumes [entailment=0, neutral=1, contradiction=2]; verify against
    // model's config.json "id2label" field before finalizing.

    let max_logit = logits[0].max(logits[1]).max(logits[2])
    let exp_e = (logits[0] - max_logit).exp()
    let exp_n = (logits[1] - max_logit).exp()
    let exp_c = (logits[2] - max_logit).exp()
    let sum   = exp_e + exp_n + exp_c

    // Guard against degenerate sum (should not happen with finite logits after max-subtraction)
    if sum == 0.0 OR sum.is_nan():
        // Return uniform distribution as safe fallback; do not panic.
        return NliScores { entailment: 1.0/3.0, neutral: 1.0/3.0, contradiction: 1.0/3.0 }

    NliScores {
        entailment:    exp_e / sum,
        neutral:       exp_n / sum,
        contradiction: exp_c / sum,
    }
    // Post-condition: entailment + neutral + contradiction ≈ 1.0 (within 1e-4)
```

---

## lib.rs Extension

Add `cross_encoder` as a public module. Export `NliScores`, `CrossEncoderProvider`, `NliProvider`,
`NliModel`, and `ensure_nli_model`.

```
// In lib.rs:
pub mod cross_encoder;  // new

pub use cross_encoder::{CrossEncoderProvider, NliProvider, NliScores};
pub use model::NliModel;              // re-export from model.rs
pub use download::ensure_nli_model;   // re-export for CLI model-download subcommand
```

---

## Error Handling

| Error Condition | Error Type | Behavior |
|----------------|-----------|----------|
| `Tokenizer::from_file` fails | `EmbedError::Tokenizer` | Propagated to `NliProvider::new` caller (NliServiceHandle → Failed) |
| `Session::builder()` fails | `EmbedError::Ort(ort::Error)` or mapped `EmbedError::InferenceFailed` | Propagated to caller |
| `session.lock()` returns PoisonError | `EmbedError::InferenceFailed("session mutex poisoned")` | Propagated; NliServiceHandle detects via `try_lock` on next `get_provider()` call |
| `score_batch(&[])` | `Ok(vec![])` | Not an error; empty input returns empty output |
| `session.run()` fails | `EmbedError::Ort(...)` | Propagated via `?`; rayon panic handler converts to `RayonError::Cancelled` if task panics |
| Extreme logits (overflow) | handled by max-subtraction in softmax | No panic; uniform distribution fallback |
| `DimensionMismatch` | `EmbedError::DimensionMismatch` | Shape validation catch; propagated to caller |

**Important**: `NliProvider` does NOT propagate panics. The `Mutex<Session>` lock failure is
returned as an error (not `.expect()`). This preserves the rayon pool's panic containment
contract — a rayon worker that panics inside score_batch poisons the mutex but does not crash
the pool, and the result is `RayonError::Cancelled` at the async boundary.

---

## Key Test Scenarios

1. **AC-01 / score sum invariant**: `score_pair("The cat is on the mat.", "A cat is on a mat.")` returns `NliScores` where `entailment + neutral + contradiction` is within 1e-4 of 1.0.
2. **AC-02 / concurrency**: Two goroutines call `score_pair` simultaneously; no deadlock; `Send + Sync` compile check.
3. **AC-03 / input truncation**: `score_pair` with 10,000-char query and 10,000-char passage returns valid `NliScores` without panic or OOM.
4. **R-19 / combined sequence**: `score_pair` with 511-token query and 10-token passage returns valid `NliScores`.
5. **Empty batch**: `score_batch(&[])` returns `Ok(vec![])`.
6. **Single-word query**: `score_pair("rust", "Rust is a systems programming language...")` returns valid `NliScores`.
7. **Softmax overflow**: Mock session returning logits `[100.0, -50.0, -50.0]` produces valid `NliScores` (no NaN).
8. **R-18 / tokenizer path isolation**: `NliModel::NliDebertaV3Small.cache_subdir()` differs from `NliMiniLM2L6H768.cache_subdir()`.
9. **AC-04 / NliModel methods**: `NliMiniLM2L6H768.model_id()` returns `"cross-encoder/nli-MiniLM2-L6-H768"`; all methods return non-empty strings.
