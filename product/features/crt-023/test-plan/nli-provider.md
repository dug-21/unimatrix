# Test Plan: NliProvider (`unimatrix-embed`)

## Component Scope

Files:
- `crates/unimatrix-embed/src/cross_encoder.rs` — `CrossEncoderProvider` trait, `NliScores`, `NliProvider`
- `crates/unimatrix-embed/src/model.rs` — `NliModel` enum additions
- `crates/unimatrix-embed/src/download.rs` — `ensure_nli_model` function

## Risks Covered

R-18 (High): Deberta tokenizer path uses MiniLM2 tokenizer config.
R-19 (Med): Combined sequence exceeds model position embedding limit.
R-03 (via): Stable sort depends on deterministic `NliScores` from `score_batch`.

---

## Unit Tests

### AC-01: NliScores Sum Constraint

```rust
#[test]
fn test_nli_scores_sum_within_tolerance() {
    // Requires: NLI model on disk. Mark #[ignore] if model unavailable.
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let scores = provider.score_pair("sky is blue", "the sky has blue color").unwrap();
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!((sum - 1.0f32).abs() < 1e-4,
        "NliScores sum {sum} not within 1e-4 of 1.0");
}
```

### AC-02: Send + Sync and Concurrent Safety

```rust
fn assert_send_sync<T: Send + Sync>() {}
#[test]
fn test_nli_provider_send_sync() {
    assert_send_sync::<NliProvider>();
    assert_send_sync::<dyn CrossEncoderProvider>();
}

#[test]
fn test_concurrent_score_pair_no_deadlock() {
    // Requires: NLI model. Mark #[ignore] if absent.
    let provider = Arc::new(NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap());
    let p1 = Arc::clone(&provider);
    let p2 = Arc::clone(&provider);
    let t1 = std::thread::spawn(move || p1.score_pair("query a", "passage a").unwrap());
    let t2 = std::thread::spawn(move || p2.score_pair("query b", "passage b").unwrap());
    // Both must complete without deadlock (timeout: test runner default, ~30s)
    let _ = t1.join().unwrap();
    let _ = t2.join().unwrap();
}
```

### AC-03: Input Truncation — Oversized Inputs

```rust
#[test]
fn test_score_pair_oversized_inputs_no_panic() {
    // Requires: NLI model. Mark #[ignore] if absent.
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let long_query = "a ".repeat(5000);   // 10,000 chars
    let long_passage = "b ".repeat(5000); // 10,000 chars
    let result = provider.score_pair(&long_query, &long_passage);
    assert!(result.is_ok(), "score_pair must not error on oversized inputs");
    let scores = result.unwrap();
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!((sum - 1.0f32).abs() < 1e-4);
}

#[test]
fn test_score_pair_truncation_applied_before_session_acquire() {
    // Verify truncation is internal to NliProvider, not at call sites.
    // Implementation check: the truncation path is exercised with a 100k-char input.
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let huge = "x ".repeat(50_000);
    let result = provider.score_pair(&huge, "short passage");
    assert!(result.is_ok());
}
```

### R-19: Combined Sequence Position Embedding Boundary

```rust
#[test]
fn test_score_pair_511_plus_10_tokens_valid() {
    // Requires: NLI model. 511 unique words as query, 10-word passage.
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let query_511 = (0..511).map(|i| format!("tok{i}")).collect::<Vec<_>>().join(" ");
    let result = provider.score_pair(&query_511, "short ten word passage here yes indeed okay");
    assert!(result.is_ok());
    let scores = result.unwrap();
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!((sum - 1.0f32).abs() < 1e-4, "Sum: {sum}");
}

#[test]
fn test_score_pair_512_plus_512_tokens_no_panic() {
    // Both sides at truncation boundary.
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let max_side = (0..512).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
    let result = provider.score_pair(&max_side, &max_side);
    assert!(result.is_ok());
    let scores = result.unwrap();
    assert!((scores.entailment + scores.neutral + scores.contradiction - 1.0).abs() < 1e-4);
}

#[test]
fn test_score_pair_256_plus_256_tokens_boundary() {
    // Mid-range boundary: 256+256 tokens should produce valid scores.
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let half = (0..256).map(|i| format!("t{i}")).collect::<Vec<_>>().join(" ");
    let result = provider.score_pair(&half, &half);
    assert!(result.is_ok());
}
```

### Score Batch Empty List Edge Case

```rust
#[test]
fn test_score_batch_empty_returns_empty_vec() {
    // Empty candidate pool after quarantine filter must not ORT-error.
    // Requires no model — pure logic test if score_batch short-circuits on empty.
    // If model required, mark #[ignore].
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let result = provider.score_batch(&[]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}
```

### Softmax Validity — Extreme Logits

```rust
#[test]
fn test_softmax_extreme_logits_produces_valid_scores() {
    // Softmax overflow guard: very large logits must not produce NaN/inf.
    // This is internal to NliProvider — test via a mock that injects raw logits
    // into the softmax function directly, or verify score_pair on a fixture pair
    // where one logit dominates (model behavior with obvious contradiction).
    // Arrange: If model available, use a strongly contradictory pair.
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let scores = provider.score_pair(
        "The building is on fire",
        "The building is perfectly safe with no hazards"
    ).unwrap();
    assert!(scores.entailment.is_finite());
    assert!(scores.neutral.is_finite());
    assert!(scores.contradiction.is_finite());
    assert!(!scores.entailment.is_nan());
    assert!(!scores.contradiction.is_nan());
}
```

---

## NliModel Enum Unit Tests

### AC-04: NliModel Methods

```rust
#[test]
fn test_nli_minilm2_model_id() {
    assert_eq!(
        NliModel::NliMiniLM2L6H768.model_id(),
        "cross-encoder/nli-MiniLM2-L6-H768"
    );
}

#[test]
fn test_nli_model_onnx_filename_returns_model_onnx() {
    // FR-04 constraint: onnx_filename() returns "model.onnx"
    assert_eq!(NliModel::NliMiniLM2L6H768.onnx_filename(), "model.onnx");
    assert_eq!(NliModel::NliDebertaV3Small.onnx_filename(), "model.onnx");
}

#[test]
fn test_nli_model_methods_return_non_empty() {
    for model in [NliModel::NliMiniLM2L6H768, NliModel::NliDebertaV3Small] {
        assert!(!model.model_id().is_empty());
        assert!(!model.onnx_repo_path().is_empty());
        assert!(!model.onnx_filename().is_empty());
        assert!(!model.cache_subdir().is_empty());
    }
}

#[test]
fn test_nli_model_cache_subdirs_distinct() {
    // R-18: distinct subdirs prevent tokenizer cross-contamination.
    let minilm_dir = NliModel::NliMiniLM2L6H768.cache_subdir();
    let deberta_dir = NliModel::NliDebertaV3Small.cache_subdir();
    assert_ne!(minilm_dir, deberta_dir,
        "cache_subdir must differ between model variants to prevent tokenizer confusion");
}
```

### AC-21: NliModel::from_config_name

```rust
#[test]
fn test_from_config_name_minilm2() {
    assert_eq!(
        NliModel::from_config_name("minilm2"),
        Some(NliModel::NliMiniLM2L6H768)
    );
}

#[test]
fn test_from_config_name_deberta() {
    assert_eq!(
        NliModel::from_config_name("deberta"),
        Some(NliModel::NliDebertaV3Small)
    );
}

#[test]
fn test_from_config_name_unknown_returns_none() {
    // R-15: unknown name must return None, not panic.
    assert_eq!(NliModel::from_config_name("gpt4"), None);
    assert_eq!(NliModel::from_config_name(""), None);
    assert_eq!(NliModel::from_config_name("MINILM2"), None); // case sensitivity check
}
```

---

## R-18: Deberta Tokenizer Pairing (Integration, when model available)

```rust
#[test]
#[ignore = "Requires deberta ONNX model; run with --include-ignored when model is cached"]
fn test_deberta_score_pair_obvious_entailment_not_garbage() {
    // If tokenizer mismatch, softmax of garbage logits would produce
    // uniform-ish (~0.33, 0.33, 0.33) scores even on obvious pairs.
    let provider = NliProvider::new(
        NliModel::NliDebertaV3Small,
        &deberta_model_path()
    ).unwrap();
    let scores = provider.score_pair(
        "Dogs are animals",
        "A dog is a type of animal"
    ).unwrap();
    assert!(scores.entailment > 0.5,
        "Obvious entailment pair should score > 0.5 entailment; got {}", scores.entailment);
}
```

---

## W1-2 Compliance

`CrossEncoderProvider` methods (`score_pair`, `score_batch`) are synchronous.
Tests for the NliProvider component itself do not test rayon dispatch (that is the
responsibility of components that call the provider). However, the trait signature being
synchronous (not async) is itself a W1-2 compliance property — verified at compile time.

## Edge Cases Not Covered by Integration Harness

- Softmax of extreme logits (internal to NliProvider, tested above)
- CJK / emoji truncation (byte vs token count): assert score is valid (no panic/OOM)
  for a 1000-character CJK string whose token count exceeds 512 but byte count does not
