use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

use crate::config::EmbedConfig;
use crate::download::ensure_model;
use crate::error::{EmbedError, Result};
use crate::model::EmbeddingModel;
use crate::normalize::l2_normalize;
use crate::pooling::mean_pool;
use crate::provider::EmbeddingProvider;

/// ONNX-based embedding provider using raw `ort` + `tokenizers`.
///
/// Thread-safe via `Mutex<Session>` for ONNX inference serialization.
/// The tokenizer is lock-free (`&self` methods).
///
/// Implements `EmbeddingProvider` and is `Send + Sync`, shareable via `Arc<OnnxProvider>`.
pub struct OnnxProvider {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    model: EmbeddingModel,
    config: EmbedConfig,
}

impl OnnxProvider {
    /// Create a new OnnxProvider. Downloads model on first use.
    ///
    /// Construction flow:
    /// 1. Resolve cache directory from config.
    /// 2. Ensure model files exist (download from HuggingFace Hub if needed).
    /// 3. Load tokenizer from `tokenizer.json`.
    /// 4. Configure tokenizer truncation and padding.
    /// 5. Build ONNX session with `GraphOptimizationLevel::Level3`.
    pub fn new(config: EmbedConfig) -> Result<Self> {
        let cache_dir = config.resolve_cache_dir();
        let model_dir = ensure_model(config.model, &cache_dir)?;

        // Load tokenizer
        let tokenizer_path = model_dir.join("tokenizer.json");
        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbedError::Tokenizer(format!("failed to load tokenizer: {e}")))?;

        // Configure truncation
        let truncation = tokenizers::TruncationParams {
            max_length: config.model.max_seq_length(),
            strategy: tokenizers::TruncationStrategy::LongestFirst,
            ..Default::default()
        };
        tokenizer
            .with_truncation(Some(truncation))
            .map_err(|e| EmbedError::Tokenizer(format!("truncation config failed: {e}")))?;

        // Configure padding
        let padding = tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        };
        tokenizer.with_padding(Some(padding));

        // Build ONNX session
        let onnx_path = model_dir.join(config.model.onnx_filename());
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .commit_from_file(&onnx_path)?;

        Ok(OnnxProvider {
            session: Mutex::new(session),
            tokenizer,
            model: config.model,
            config,
        })
    }

    /// Internal: run inference on a single text.
    fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| EmbedError::Tokenizer(format!("{e}")))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        let token_type_ids = encoding.get_type_ids();

        let seq_len = input_ids.len();

        // Convert to i64 for ONNX tensor format
        let input_ids_i64: Vec<i64> = input_ids.iter().map(|&v| v as i64).collect();
        let attention_mask_i64: Vec<i64> = attention_mask.iter().map(|&v| v as i64).collect();
        let token_type_ids_i64: Vec<i64> = token_type_ids.iter().map(|&v| v as i64).collect();

        // Create tensors with shape [1, seq_len]
        let shape = vec![1_i64, seq_len as i64];
        let ids_tensor =
            Tensor::from_array((shape.clone(), input_ids_i64.clone()))?;
        let mask_tensor =
            Tensor::from_array((shape.clone(), attention_mask_i64.clone()))?;
        let type_tensor =
            Tensor::from_array((shape, token_type_ids_i64.clone()))?;

        let inputs = ort::inputs![
            "input_ids" => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => type_tensor,
        ]?;

        // Run inference under lock, extract data, then release
        let (output_flat, actual_seq_len) = {
            let session = self.session.lock().expect("session lock poisoned");
            let outputs = session.run(inputs)?;

            let output_value = &outputs[0];
            let (shape, output_data) = output_value.try_extract_raw_tensor::<f32>()?;

            let hidden_dim = self.model.dimension();

            if shape.len() != 3 || shape[0] != 1 || shape[2] as usize != hidden_dim {
                return Err(EmbedError::DimensionMismatch {
                    expected: hidden_dim,
                    got: if shape.len() == 3 { shape[2] as usize } else { 0 },
                });
            }

            let actual_seq_len = shape[1] as usize;
            // Copy data out before session lock drops
            (output_data.to_vec(), actual_seq_len)
        };

        let hidden_dim = self.model.dimension();

        // Mean pool with attention mask
        let pooled = mean_pool(
            &output_flat,
            &attention_mask_i64,
            1,
            actual_seq_len,
            hidden_dim,
        );
        let mut embedding = pooled.into_iter().next().expect("pooled result empty");

        // L2 normalize
        l2_normalize(&mut embedding);

        Ok(embedding)
    }

    /// Internal: run inference on a batch of texts.
    fn embed_batch_internal(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let batch_size_limit = self.config.batch_size.max(1);

        for chunk in texts.chunks(batch_size_limit) {
            let batch_size = chunk.len();

            // Batch tokenize (pads to longest in batch)
            let encodings = self
                .tokenizer
                .encode_batch(chunk.to_vec(), true)
                .map_err(|e| EmbedError::Tokenizer(format!("{e}")))?;

            let seq_len = encodings[0].get_ids().len();

            // Flatten to contiguous arrays [batch_size, seq_len]
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

            // Create tensors with shape [batch_size, seq_len]
            let shape = vec![batch_size as i64, seq_len as i64];
            let ids_tensor =
                Tensor::from_array((shape.clone(), input_ids_flat.clone()))?;
            let mask_tensor =
                Tensor::from_array((shape.clone(), attention_mask_flat.clone()))?;
            let type_tensor =
                Tensor::from_array((shape, token_type_ids_flat.clone()))?;

            let inputs = ort::inputs![
                "input_ids" => ids_tensor,
                "attention_mask" => mask_tensor,
                "token_type_ids" => type_tensor,
            ]?;

            let hidden_dim = self.model.dimension();

            // Run inference under lock, extract data, then release
            let (output_flat, actual_seq_len) = {
                let session = self.session.lock().expect("session lock poisoned");
                let outputs = session.run(inputs)?;

                let output_value = &outputs[0];
                let (shape, output_data) = output_value.try_extract_raw_tensor::<f32>()?;

                let actual_seq_len = shape[1] as usize;
                // Copy data out before session lock drops
                (output_data.to_vec(), actual_seq_len)
            };

            // Mean pool each sequence
            let pooled = mean_pool(
                &output_flat,
                &attention_mask_flat,
                batch_size,
                actual_seq_len,
                hidden_dim,
            );

            // L2 normalize each embedding
            for embedding in pooled {
                let mut emb = embedding;
                l2_normalize(&mut emb);
                results.push(emb);
            }
        }

        Ok(results)
    }
}

impl EmbeddingProvider for OnnxProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_single(text)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.embed_batch_internal(texts)
    }

    fn dimension(&self) -> usize {
        self.model.dimension()
    }

    fn name(&self) -> &str {
        self.model.model_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{assert_normalized, cosine_similarity};
    use crate::text::{embed_entry, prepare_text};
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<OnnxProvider>();
    }

    #[test]
    #[ignore]
    fn test_provider_construction_default() {
        let config = EmbedConfig::default();
        let result = OnnxProvider::new(config);
        assert!(result.is_ok(), "OnnxProvider::new failed: {:?}", result.err());
        let provider = result.unwrap();
        assert_eq!(provider.dimension(), 384);
        assert_eq!(provider.name(), "sentence-transformers/all-MiniLM-L6-v2");
    }

    #[test]
    #[ignore]
    fn test_embed_single_text() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let result = provider.embed("hello world");
        assert!(result.is_ok());
        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 384);
        for v in &embedding {
            assert!(v.is_finite(), "non-finite value in embedding");
        }
    }

    #[test]
    #[ignore]
    fn test_embed_varied_texts() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let texts = [
            "hello world",
            "a",
            "This is a longer paragraph about software development practices and conventions.",
        ];
        for text in &texts {
            let embedding = provider.embed(text).unwrap();
            assert_eq!(embedding.len(), 384);
        }
    }

    #[test]
    #[ignore]
    fn test_embed_batch() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let texts = ["hello", "world", "rust", "embedding", "test"];
        let result = provider.embed_batch(&texts);
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 5);
        for emb in &embeddings {
            assert_eq!(emb.len(), 384);
        }
    }

    #[test]
    #[ignore]
    fn test_normalization_diverse_inputs() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let inputs = ["short", "a much longer text about various topics", "", " ", "!"];
        for text in &inputs {
            let embedding = provider.embed(text).unwrap();
            assert_normalized(&embedding, 0.001);
        }
    }

    #[test]
    #[ignore]
    fn test_semantic_similarity() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let e1 = provider.embed("Rust error handling best practices").unwrap();
        let e2 = provider.embed("How to handle errors in Rust").unwrap();
        let e3 = provider.embed("Recipe for chocolate cake").unwrap();

        let sim_related = cosine_similarity(&e1, &e2);
        let sim_unrelated = cosine_similarity(&e1, &e3);

        assert!(
            sim_related > 0.7,
            "related texts similarity too low: {sim_related}"
        );
        assert!(
            sim_unrelated < 0.3,
            "unrelated texts similarity too high: {sim_unrelated}"
        );
    }

    #[test]
    #[ignore]
    fn test_concurrent_embed() {
        let provider = Arc::new(OnnxProvider::new(EmbedConfig::default()).unwrap());
        let mut handles = vec![];
        for i in 0..4 {
            let p = provider.clone();
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    let text = format!("thread {i} text {j}");
                    let result = p.embed(&text);
                    assert!(result.is_ok());
                    assert_eq!(result.unwrap().len(), 384);
                }
            });
            handles.push(handle);
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    #[ignore]
    fn test_batch_vs_single_consistency() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let texts = [
            "hello world",
            "rust programming",
            "embedding pipeline",
            "vector search",
            "machine learning",
        ];

        let individual: Vec<Vec<f32>> =
            texts.iter().map(|t| provider.embed(t).unwrap()).collect();

        let batch = provider.embed_batch(&texts).unwrap();

        assert_eq!(batch.len(), individual.len());
        for i in 0..texts.len() {
            for j in 0..384 {
                assert!(
                    (individual[i][j] - batch[i][j]).abs() < 1e-5,
                    "mismatch at [{i}][{j}]: individual={}, batch={}",
                    individual[i][j],
                    batch[i][j]
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn test_empty_string() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let result = provider.embed("");
        assert!(result.is_ok());
        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 384);
        assert_normalized(&embedding, 0.001);
    }

    #[test]
    #[ignore]
    fn test_degenerate_inputs() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let inputs = [" ", "\t\n", "a", "!@#$%^&*()"];
        for text in &inputs {
            let result = provider.embed(text);
            assert!(result.is_ok(), "failed for input: {text:?}");
            assert_eq!(result.unwrap().len(), 384);
        }
    }

    #[test]
    #[ignore]
    fn test_embed_batch_empty() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let result = provider.embed_batch(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    #[ignore]
    fn test_batch_size_boundary() {
        let config = EmbedConfig {
            batch_size: 3,
            ..Default::default()
        };
        let provider = OnnxProvider::new(config).unwrap();

        let texts3 = ["a", "b", "c"];
        assert_eq!(provider.embed_batch(&texts3).unwrap().len(), 3);

        let texts4 = ["a", "b", "c", "d"];
        assert_eq!(provider.embed_batch(&texts4).unwrap().len(), 4);

        let texts2 = ["a", "b"];
        assert_eq!(provider.embed_batch(&texts2).unwrap().len(), 2);
    }

    #[test]
    #[ignore]
    fn test_batch_order_preserved() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let texts = ["alpha", "beta", "gamma"];
        let batch = provider.embed_batch(&texts).unwrap();
        for (i, text) in texts.iter().enumerate() {
            let individual = provider.embed(text).unwrap();
            for j in 0..384 {
                assert!(
                    (batch[i][j] - individual[j]).abs() < 1e-5,
                    "order mismatch at [{i}][{j}]"
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn test_no_nan_in_output() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let texts = ["", " ", "normal text", "!@#"];
        for text in &texts {
            let embedding = provider.embed(text).unwrap();
            for v in &embedding {
                assert!(!v.is_nan(), "NaN in output for input: {text:?}");
                assert!(!v.is_infinite(), "Infinity in output for input: {text:?}");
            }
        }
    }

    #[test]
    #[ignore]
    fn test_deterministic_output() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let emb1 = provider.embed("deterministic test").unwrap();
        let emb2 = provider.embed("deterministic test").unwrap();
        assert_eq!(emb1, emb2);
    }

    #[test]
    #[ignore]
    fn test_dimension_accessor() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        assert_eq!(provider.dimension(), 384);
    }

    #[test]
    #[ignore]
    fn test_embed_entry_convenience() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let entry_emb = embed_entry(&provider, "Auth", "Use JWT", ": ").unwrap();
        let manual_emb = provider
            .embed(&prepare_text("Auth", "Use JWT", ": "))
            .unwrap();
        assert_eq!(entry_emb, manual_emb);
    }

    #[test]
    #[ignore]
    fn test_long_text_truncation() {
        let provider = OnnxProvider::new(EmbedConfig::default()).unwrap();
        let long_text = "word ".repeat(1000);
        let result = provider.embed(&long_text);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 384);
    }
}
