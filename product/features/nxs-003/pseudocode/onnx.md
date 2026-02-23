# C9: ONNX Provider Module -- Pseudocode

## Purpose

Concrete `EmbeddingProvider` implementation using raw `ort` + `tokenizers`. The main inference engine.

## File: `crates/unimatrix-embed/src/onnx.rs`

```
USE std::sync::Mutex
USE ort::Session
USE tokenizers::Tokenizer
USE crate::{
    config::EmbedConfig,
    download::ensure_model,
    error::{EmbedError, Result},
    model::EmbeddingModel,
    normalize::l2_normalize,
    pooling::mean_pool,
    provider::EmbeddingProvider,
}

pub struct OnnxProvider {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    model: EmbeddingModel,
    config: EmbedConfig,
}

IMPL OnnxProvider:
    /// Create a new OnnxProvider. Downloads model on first use.
    pub fn new(config: EmbedConfig) -> Result<Self>:
        // 1. Resolve cache directory
        cache_dir = config.resolve_cache_dir()

        // 2. Ensure model files exist (download if needed)
        model_dir = ensure_model(config.model, &cache_dir)?

        // 3. Load tokenizer
        tokenizer_path = model_dir.join("tokenizer.json")
        tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbedError::Tokenizer(format!("failed to load tokenizer: {e}")))?

        // 4. Configure tokenizer: truncation + padding
        tokenizer.with_truncation(Some(TruncationParams {
            max_length: config.model.max_seq_length(),
            strategy: TruncationStrategy::LongestFirst,
            ..Default::default()
        })).map_err(|e| EmbedError::Tokenizer(format!("truncation config failed: {e}")))?

        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..Default::default()
        }))

        // 5. Load ONNX session
        onnx_path = model_dir.join(config.model.onnx_filename())
        session = Session::builder()?
            .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
            .commit_from_file(&onnx_path)?

        Ok(OnnxProvider {
            session: Mutex::new(session),
            tokenizer,
            model: config.model,
            config,
        })

    /// Internal: run inference on a single text
    fn embed_single(&self, text: &str) -> Result<Vec<f32>>:
        // Tokenize
        encoding = self.tokenizer.encode(text, true)
            .map_err(|e| EmbedError::Tokenizer(format!("{e}")))?

        input_ids = encoding.get_ids().to_vec()
        attention_mask = encoding.get_attention_mask().to_vec()
        token_type_ids = encoding.get_type_ids().to_vec()

        seq_len = input_ids.len()

        // Build tensors [1, seq_len]
        input_ids_i64: Vec<i64> = input_ids.iter().map(|&v| v as i64).collect()
        attention_mask_i64: Vec<i64> = attention_mask.iter().map(|&v| v as i64).collect()
        token_type_ids_i64: Vec<i64> = token_type_ids.iter().map(|&v| v as i64).collect()

        // Create ort tensors with shape [1, seq_len]
        // Use ort's tensor creation API

        // Run inference under lock
        session = self.session.lock().unwrap()
        outputs = session.run(inputs)?    // inputs built from tensors above
        // Release lock
        drop(session)

        // Extract output tensor [1, seq_len, 384]
        output_tensor = extract first output as f32 slice
        hidden_dim = self.model.dimension()

        // Validate dimension
        expected_size = 1 * seq_len * hidden_dim
        IF output_tensor.len() != expected_size:
            return Err(EmbedError::DimensionMismatch {
                expected: expected_size,
                got: output_tensor.len()
            })

        // Mean pool with attention mask
        pooled = mean_pool(output_tensor, &attention_mask_i64, 1, seq_len, hidden_dim)
        embedding = pooled.into_iter().next().unwrap()

        // L2 normalize
        embedding_mut = embedding
        l2_normalize(&mut embedding_mut)

        Ok(embedding_mut)

    /// Internal: run inference on a batch of texts
    fn embed_batch_internal(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>:
        IF texts.is_empty():
            return Ok(Vec::new())

        results = Vec::new()

        // Process in chunks of batch_size
        FOR chunk IN texts.chunks(self.config.batch_size.max(1)):
            batch_size = chunk.len()

            // Batch tokenize (pads to longest in batch)
            encodings = self.tokenizer.encode_batch(chunk.to_vec(), true)
                .map_err(|e| EmbedError::Tokenizer(format!("{e}")))?

            seq_len = encodings[0].get_ids().len()

            // Flatten to contiguous arrays [batch_size, seq_len]
            input_ids_flat: Vec<i64> = Vec with capacity batch_size * seq_len
            attention_mask_flat: Vec<i64> = Vec with capacity batch_size * seq_len
            token_type_ids_flat: Vec<i64> = Vec with capacity batch_size * seq_len

            FOR enc IN &encodings:
                FOR &id IN enc.get_ids():
                    input_ids_flat.push(id as i64)
                FOR &mask IN enc.get_attention_mask():
                    attention_mask_flat.push(mask as i64)
                FOR &tid IN enc.get_type_ids():
                    token_type_ids_flat.push(tid as i64)

            // Build tensors with shape [batch_size, seq_len]
            // Run inference under lock
            session = self.session.lock().unwrap()
            outputs = session.run(inputs)?
            drop(session)

            // Extract output [batch_size, seq_len, hidden_dim]
            output_tensor = extract first output as f32 slice
            hidden_dim = self.model.dimension()

            // Mean pool each sequence
            pooled = mean_pool(output_tensor, &attention_mask_flat, batch_size, seq_len, hidden_dim)

            // L2 normalize each embedding
            FOR embedding IN pooled:
                embedding_mut = embedding
                l2_normalize(&mut embedding_mut)
                results.push(embedding_mut)

        Ok(results)

IMPL EmbeddingProvider for OnnxProvider:
    fn embed(&self, text: &str) -> Result<Vec<f32>>:
        self.embed_single(text)

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>:
        self.embed_batch_internal(texts)

    fn dimension(&self) -> usize:
        self.model.dimension()

    fn name(&self) -> &str:
        self.model.model_id()
```

## Design Notes

- `session: Mutex<Session>` -- locked only during inference, released immediately after (ADR-001).
- `tokenizer: Tokenizer` -- lock-free. `encode()` and `encode_batch()` take `&self`.
- Tokenizer truncation is configured at construction to model's max_seq_length.
- Tokenizer padding is set to `BatchLongest` for batch encoding.
- Input tensor types are i64 (ONNX standard for transformer models).
- Output extraction: first output tensor, shape [batch_size, seq_len, 384].
- R-04: Model loading is high risk -- all construction errors are typed.
- R-07: Thread safety via Mutex -- session lock is held for minimum duration.
- The `Mutex::lock().unwrap()` is acceptable because poisoning only happens if another thread panicked, which indicates a fatal bug.

## ort API Details (to resolve during implementation)

- Session builder: `Session::builder()?.with_optimization_level(Level3)?.commit_from_file(path)?`
- Tensor creation: Need to determine exact API for ort 2.0.0-rc.11 tensor creation from slices with shapes.
- Output extraction: May use index 0 or output name "last_hidden_state". Check actual model output names.
- Architecture OQ-2: Exact API for output tensor extraction depends on ort rc.11 specifics.
