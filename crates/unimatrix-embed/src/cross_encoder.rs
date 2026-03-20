use std::path::Path;
use std::sync::{Arc, Mutex};

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

use crate::error::{EmbedError, Result};
use crate::model::NliModel;

// Label order for cross-encoder/nli-MiniLM2-L6-H768:
//
// Verified from the model's config.json "id2label" field:
//   "0": "contradiction", "1": "entailment", "2": "neutral"
//
// Source: https://huggingface.co/cross-encoder/nli-MiniLM2-L6-H768/blob/main/config.json
//
// Therefore logit indices are:
const LOGIT_IDX_CONTRADICTION: usize = 0;
const LOGIT_IDX_ENTAILMENT: usize = 1;
const LOGIT_IDX_NEUTRAL: usize = 2;

/// Per-side character limit for input truncation (NFR-08, FR-06, AC-03).
///
/// Applied to each side of the (query, passage) pair independently before tokenization.
/// Prevents adversarial inputs from causing OOM in the tokenizer or ONNX runtime.
/// Token-level truncation (512 tokens) is also enforced by `TruncationParams`.
pub(crate) const PER_SIDE_CHAR_LIMIT: usize = 2000;

/// Normalized NLI classification output.
///
/// Sum of three fields ≈ 1.0 (within 1e-4). Produced by softmax over the
/// 3-element ONNX logit output of the cross-encoder model.
#[derive(Debug, Clone, PartialEq)]
pub struct NliScores {
    /// P(premise entails hypothesis). Sort key for search re-ranking (ADR-002).
    pub entailment: f32,
    /// P(premise and hypothesis are unrelated).
    pub neutral: f32,
    /// P(premise contradicts hypothesis). Edge creation key for `GRAPH_EDGES`.
    pub contradiction: f32,
    // INVARIANT: entailment + neutral + contradiction ≈ 1.0 (within 1e-4)
}

/// Abstraction over any NLI cross-encoder ONNX model.
///
/// Both methods are **synchronous** — implementations are called from rayon threads (W1-2).
/// All implementations must be `Send + Sync`.
pub trait CrossEncoderProvider: Send + Sync {
    /// Score a single `(query, passage)` pair.
    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>;

    /// Score a batch of `(query, passage)` pairs.
    ///
    /// Returns a `Vec<NliScores>` of the same length as `pairs`.
    /// Returns `Ok(vec![])` for an empty input slice — never an error.
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>;

    /// Human-readable model identifier (e.g., `"cross-encoder/nli-MiniLM2-L6-H768"`).
    fn name(&self) -> &str;
}

/// ONNX-backed NLI cross-encoder provider.
///
/// Mirrors `OnnxProvider` structure (ADR-001). Thread-safe via `Mutex<Session>` for
/// inference serialization. The tokenizer lives outside the mutex for lock-free
/// tokenization.
///
/// Per-side input truncation (512 tokens / ~2000 chars) is enforced inside
/// `score_batch` before the `Mutex<Session>` is acquired (NFR-08, security).
pub struct NliProvider {
    /// Single shared ONNX session — serializes inference (ADR-001).
    session: Mutex<Session>,
    /// Lock-free tokenizer — tokenization happens before mutex acquisition.
    tokenizer: Tokenizer,
    /// Model identifier returned by `CrossEncoderProvider::name()`.
    model_name: String,
}

impl std::fmt::Debug for NliProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NliProvider")
            .field("model_name", &self.model_name)
            .finish_non_exhaustive()
    }
}

impl NliProvider {
    /// Construct an `NliProvider` from model files on disk.
    ///
    /// Hash verification (ADR-003) must happen **before** this call in
    /// `NliServiceHandle`. The `model_path` directory must contain `model.onnx`
    /// and `tokenizer.json`.
    pub fn new(model: NliModel, model_path: &Path) -> Result<Self> {
        let tokenizer_path = model_path.join("tokenizer.json");
        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbedError::Tokenizer(format!("failed to load tokenizer: {e}")))?;

        // Configure truncation to model max_position_embeddings (512 for MiniLM2).
        // This is the final safety net for combined pair length; per-side pre-truncation
        // (NFR-08) happens in score_batch before tokenization.
        let truncation = tokenizers::TruncationParams {
            max_length: 512,
            strategy: tokenizers::TruncationStrategy::LongestFirst,
            ..Default::default()
        };
        tokenizer
            .with_truncation(Some(truncation))
            .map_err(|e| EmbedError::Tokenizer(format!("truncation config failed: {e}")))?;

        // Configure padding to batch longest (enables efficient batching).
        let padding = tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        };
        tokenizer.with_padding(Some(padding));

        // Build ONNX session with level-3 graph optimization.
        let onnx_path = model_path.join(model.onnx_filename());
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .commit_from_file(&onnx_path)?;

        Ok(NliProvider {
            session: Mutex::new(session),
            tokenizer,
            model_name: model.model_id().to_string(),
        })
    }

    /// Load an `NliProvider` from disk and wrap in `Arc`.
    ///
    /// Convenience constructor for use in `NliServiceHandle`.
    pub fn load(model: NliModel, model_path: &Path) -> Result<Arc<Self>> {
        Ok(Arc::new(Self::new(model, model_path)?))
    }
}

/// Enforce per-side input truncation before pair tokenization (NFR-08, FR-06).
///
/// Truncates at a character boundary (not byte boundary) to preserve UTF-8 validity.
/// Silent — no warning emitted per call. This is a security requirement.
///
/// The character limit (~2000) is a safe proxy for 512 tokens on most tokenizers.
/// Token-level truncation is also enforced by `TruncationParams` as a second layer.
pub(crate) fn truncate_input(text: &str) -> &str {
    if text.len() <= PER_SIDE_CHAR_LIMIT {
        return text;
    }
    // Walk back from the byte limit to find a valid char boundary.
    let mut end = PER_SIDE_CHAR_LIMIT.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Numerically stable softmax over 3 logits.
///
/// Max-subtraction before `exp` prevents overflow for very large logits (edge case
/// documented in RISK-TEST-STRATEGY).
///
/// Label order for `cross-encoder/nli-MiniLM2-L6-H768` (from config.json "id2label"):
///   logit[LOGIT_IDX_CONTRADICTION=0] = contradiction
///   logit[LOGIT_IDX_ENTAILMENT=1]    = entailment
///   logit[LOGIT_IDX_NEUTRAL=2]       = neutral
///
/// See `LOGIT_IDX_*` constants at the top of this file.
pub(crate) fn softmax_3class(logits: &[f32]) -> NliScores {
    debug_assert_eq!(logits.len(), 3, "softmax_3class expects exactly 3 logits");

    let max_logit = logits[0].max(logits[1]).max(logits[2]);
    let exps = [
        (logits[LOGIT_IDX_CONTRADICTION] - max_logit).exp(),
        (logits[LOGIT_IDX_ENTAILMENT] - max_logit).exp(),
        (logits[LOGIT_IDX_NEUTRAL] - max_logit).exp(),
    ];
    let sum = exps[0] + exps[1] + exps[2];

    // Guard against degenerate sum (should not happen with finite logits after max-subtraction).
    if sum == 0.0 || sum.is_nan() {
        // Return uniform distribution as safe fallback; do not panic.
        return NliScores {
            entailment: 1.0 / 3.0,
            neutral: 1.0 / 3.0,
            contradiction: 1.0 / 3.0,
        };
    }

    NliScores {
        entailment: exps[LOGIT_IDX_ENTAILMENT] / sum,
        neutral: exps[LOGIT_IDX_NEUTRAL] / sum,
        contradiction: exps[LOGIT_IDX_CONTRADICTION] / sum,
    }
}

impl CrossEncoderProvider for NliProvider {
    fn name(&self) -> &str {
        &self.model_name
    }

    /// Score a single `(query, passage)` pair.
    ///
    /// Delegates to `score_batch` for code reuse.
    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores> {
        let mut scores = self.score_batch(&[(query, passage)])?;
        scores
            .pop()
            .ok_or_else(|| EmbedError::InferenceFailed("empty batch result".into()))
    }

    /// Score a batch of `(query, passage)` pairs.
    ///
    /// All pairs run under a **single** `Mutex<Session>` acquisition (ADR-001).
    /// Per-side truncation is applied before tokenization (NFR-08).
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>> {
        if pairs.is_empty() {
            // IMPORTANT: empty batch must return Ok(vec![]) not an ORT session error.
            // Empty candidate pool after quarantine filter is a valid call-site scenario.
            return Ok(vec![]);
        }

        // Step 1: Per-side truncation BEFORE tokenization (NFR-08, security, R-19).
        // Applied to each side independently. The combined pair may still approach the
        // 512-token model limit; TruncationParams handles the final combined length.
        let truncated_pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(q, p)| (truncate_input(q).to_string(), truncate_input(p).to_string()))
            .collect();

        // Step 2: Tokenize all pairs outside the mutex lock (lock-free tokenization).
        // Cross-encoder concatenation: "[CLS] query [SEP] passage [SEP]"
        // The tokenizer handles pair encoding automatically when supplied as Dual input.
        let encode_inputs: Vec<tokenizers::EncodeInput> = truncated_pairs
            .iter()
            .map(|(q, p)| {
                tokenizers::EncodeInput::Dual(
                    tokenizers::InputSequence::from(q.as_str()),
                    tokenizers::InputSequence::from(p.as_str()),
                )
            })
            .collect();

        let encodings = self
            .tokenizer
            .encode_batch(encode_inputs, true)
            .map_err(|e| EmbedError::Tokenizer(format!("{e}")))?;

        let batch_size = encodings.len();
        let seq_len = encodings[0].get_ids().len(); // padded to longest in batch

        // Step 3: Flatten to contiguous [batch_size, seq_len] arrays (i64 for ONNX).
        let mut input_ids_flat: Vec<i64> = Vec::with_capacity(batch_size * seq_len);
        let mut attention_mask_flat: Vec<i64> = Vec::with_capacity(batch_size * seq_len);
        let mut token_type_ids_flat: Vec<i64> = Vec::with_capacity(batch_size * seq_len);

        for enc in &encodings {
            for &id in enc.get_ids() {
                input_ids_flat.push(id as i64);
            }
            for &mask in enc.get_attention_mask() {
                attention_mask_flat.push(mask as i64);
            }
            for &tid in enc.get_type_ids() {
                token_type_ids_flat.push(tid as i64);
            }
        }

        let shape = vec![batch_size as i64, seq_len as i64];
        let ids_tensor = Tensor::from_array((shape.clone(), input_ids_flat))?;
        let mask_tensor = Tensor::from_array((shape.clone(), attention_mask_flat))?;
        let types_tensor = Tensor::from_array((shape, token_type_ids_flat))?;

        let inputs = ort::inputs![
            "input_ids"      => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => types_tensor,
        ]?;

        // Step 4: Acquire mutex, run inference, release mutex.
        // Tokenization is complete (lock-free). Entire batch runs under ONE acquisition.
        let logits_flat: Vec<f32> = {
            let session = self
                .session
                .lock()
                .map_err(|_| EmbedError::InferenceFailed("session mutex poisoned".into()))?;

            let outputs = session.run(inputs)?;

            // Output shape: [batch_size, 3]
            let output_value = &outputs[0];
            let (out_shape, data) = output_value.try_extract_raw_tensor::<f32>()?;

            // Validate: must be [batch_size, 3]
            if out_shape.len() != 2 || out_shape[0] != batch_size as i64 || out_shape[1] != 3 {
                return Err(EmbedError::DimensionMismatch {
                    expected: 3,
                    got: if out_shape.len() >= 2 {
                        out_shape[1] as usize
                    } else {
                        0
                    },
                });
            }

            // Copy data out before session lock drops.
            data.to_vec()
            // Session lock released here.
        };

        // Step 5: Apply softmax per sample (outside the mutex lock).
        // Numerically stable: max-subtraction prevents overflow.
        let scores: Vec<NliScores> = logits_flat.chunks(3).map(softmax_3class).collect();

        Ok(scores)
    }
}

#[cfg(test)]
#[path = "cross_encoder_tests.rs"]
mod tests;
