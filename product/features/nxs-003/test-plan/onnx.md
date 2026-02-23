# C9: ONNX Provider Module -- Test Plan

## Tests

All tests in this module require ONNX model files and are marked `#[ignore]` for offline runs.

```
test_provider_construction_default:
    // AC-02: OnnxProvider loads model successfully
    config = EmbedConfig::default()
    result = OnnxProvider::new(config)
    ASSERT result.is_ok()
    provider = result.unwrap()
    ASSERT provider.dimension() == 384
    ASSERT provider.name() == "sentence-transformers/all-MiniLM-L6-v2"

test_embed_single_text_ac03:
    // AC-03: Returns 384-d vector
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    result = provider.embed("hello world")
    ASSERT result.is_ok()
    embedding = result.unwrap()
    ASSERT embedding.len() == 384
    // All elements finite
    FOR v IN &embedding:
        ASSERT v.is_finite()

test_embed_varied_texts_ac03:
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    texts = ["hello world", "a", "This is a longer paragraph about software development practices."]
    FOR text IN texts:
        embedding = provider.embed(text).unwrap()
        ASSERT embedding.len() == 384

test_embed_batch_ac04:
    // AC-04: Returns one embedding per input
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    texts = ["hello", "world", "rust", "embedding", "test"]
    result = provider.embed_batch(&texts)
    ASSERT result.is_ok()
    embeddings = result.unwrap()
    ASSERT embeddings.len() == 5
    FOR emb IN &embeddings:
        ASSERT emb.len() == 384

test_normalization_diverse_inputs_ac05:
    // AC-05: All embeddings L2-normalized
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    inputs = ["short", "a much longer text about various topics",
              "", " ", "!", "unicode test 384-d vectors"]
    FOR text IN inputs:
        embedding = provider.embed(text).unwrap()
        norm = embedding.iter().map(|v| v * v).sum::<f32>().sqrt()
        ASSERT (norm - 1.0).abs() < 0.001

test_semantic_similarity_ac08:
    // AC-08: High sim for related, low for unrelated
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    e1 = provider.embed("Rust error handling best practices").unwrap()
    e2 = provider.embed("How to handle errors in Rust").unwrap()
    e3 = provider.embed("Recipe for chocolate cake").unwrap()

    sim_related = cosine_similarity(&e1, &e2)
    sim_unrelated = cosine_similarity(&e1, &e3)

    ASSERT sim_related > 0.7
    ASSERT sim_unrelated < 0.3

test_send_sync_ac10:
    // AC-10: Compile-time assertion
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OnnxProvider>()

test_concurrent_embed_ac10:
    // AC-10: Arc<OnnxProvider> across threads
    provider = Arc::new(OnnxProvider::new(EmbedConfig::default()).unwrap())
    handles = vec![]
    FOR i IN 0..4:
        p = provider.clone()
        handle = thread::spawn(move || {
            FOR j IN 0..10:
                result = p.embed(&format!("thread {i} text {j}"))
                ASSERT result.is_ok()
                ASSERT result.unwrap().len() == 384
        })
        handles.push(handle)
    FOR h IN handles:
        h.join().unwrap()

test_batch_vs_single_consistency_ac11:
    // AC-11: Batch must match individual embeddings
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    texts = ["hello world", "rust programming", "embedding pipeline",
             "vector search", "machine learning"]

    // Individual
    individual: Vec<Vec<f32>> = texts.iter()
        .map(|t| provider.embed(t).unwrap())
        .collect()

    // Batch
    batch = provider.embed_batch(&texts).unwrap()

    ASSERT batch.len() == individual.len()
    FOR i IN 0..texts.len():
        FOR j IN 0..384:
            ASSERT (individual[i][j] - batch[i][j]).abs() < 1e-5

test_empty_string_ac12:
    // AC-12: Empty string returns valid embedding
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    result = provider.embed("")
    ASSERT result.is_ok()
    embedding = result.unwrap()
    ASSERT embedding.len() == 384
    norm = embedding.iter().map(|v| v * v).sum::<f32>().sqrt()
    ASSERT (norm - 1.0).abs() < 0.001

test_custom_cache_dir_ac13:
    // AC-13: Custom cache directory
    temp = tempdir()
    config = EmbedConfig {
        cache_dir: Some(temp.path().to_path_buf()),
        ..Default::default()
    }
    result = OnnxProvider::new(config)
    ASSERT result.is_ok()
    // Model files in temp dir
    subdir = temp.path().join("sentence-transformers_all-MiniLM-L6-v2")
    ASSERT subdir.join("model.onnx").exists()

test_degenerate_inputs_r09:
    // R-09: Various degenerate inputs
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    inputs = [" ", "\t\n", "a", "!@#$%^&*()", "\u{1f600}\u{1f600}"]
    FOR text IN inputs:
        result = provider.embed(text)
        ASSERT result.is_ok()
        ASSERT result.unwrap().len() == 384

test_embed_batch_empty:
    // R-09 scenario 4: Empty batch
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    result = provider.embed_batch(&[])
    ASSERT result.is_ok()
    ASSERT result.unwrap().is_empty()

test_batch_size_boundary_r14:
    // R-14: batch_size boundary conditions
    config = EmbedConfig { batch_size: 3, ..Default::default() }
    provider = OnnxProvider::new(config).unwrap()

    // Exactly batch_size
    texts3: Vec<&str> = (0..3).map(|i| ["a", "b", "c"][i]).collect()
    result = provider.embed_batch(&texts3)
    ASSERT result.unwrap().len() == 3

    // batch_size + 1
    texts4 = vec!["a", "b", "c", "d"]
    result = provider.embed_batch(&texts4)
    ASSERT result.unwrap().len() == 4

    // batch_size - 1
    texts2 = vec!["a", "b"]
    result = provider.embed_batch(&texts2)
    ASSERT result.unwrap().len() == 2

test_batch_order_preserved:
    // R-14 scenario 6: Output order matches input order
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    texts = ["alpha", "beta", "gamma"]
    batch = provider.embed_batch(&texts).unwrap()
    FOR i IN 0..3:
        individual = provider.embed(texts[i]).unwrap()
        FOR j IN 0..384:
            ASSERT (batch[i][j] - individual[j]).abs() < 1e-5

test_no_nan_in_output_r11:
    // R-11: No NaN or infinity in any output
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    texts = ["", " ", "normal text", "!@#"]
    FOR text IN texts:
        embedding = provider.embed(text).unwrap()
        FOR v IN &embedding:
            ASSERT NOT v.is_nan()
            ASSERT NOT v.is_infinite()

test_deterministic_output:
    // R-01 scenario 7: Same input produces same output
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    emb1 = provider.embed("deterministic test").unwrap()
    emb2 = provider.embed("deterministic test").unwrap()
    ASSERT emb1 == emb2

test_dimension_accessor_ac16:
    // AC-16: dimension() returns 384
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    ASSERT provider.dimension() == 384

test_embed_entry_ac07:
    // AC-07: embed_entry returns same as prepare_text + embed
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    entry_emb = embed_entry(&provider, "Auth", "Use JWT").unwrap()
    manual_emb = provider.embed(&prepare_text("Auth", "Use JWT", ": ")).unwrap()
    ASSERT entry_emb == manual_emb

test_long_text_truncation_r06:
    // R-06: Very long text truncated but still produces valid embedding
    provider = OnnxProvider::new(EmbedConfig::default()).unwrap()
    long_text = "word ".repeat(1000)  // ~1000 words, exceeds 256 token limit
    result = provider.embed(&long_text)
    ASSERT result.is_ok()
    ASSERT result.unwrap().len() == 384
```

## Risks Covered

- R-03 (High): Batch vs single consistency (AC-11)
- R-04 (High): Model loading success
- R-06 (High): Truncation behavior
- R-07 (High): Thread safety (AC-10)
- R-09 (Medium): Empty/degenerate inputs (AC-12)
- R-11 (Medium): No NaN/infinity
- R-14 (Medium): Batch boundaries (AC-04)
- AC-02, AC-03, AC-04, AC-05, AC-07, AC-08, AC-10, AC-11, AC-12, AC-13, AC-16
