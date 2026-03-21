use super::*;

// ---- NliScores sum invariant (mock softmax, no model required) ----

#[test]
fn test_softmax_sum_invariant_typical() {
    // Typical logits — result must sum to ≈ 1.0
    let scores = softmax_3class(&[2.5_f32, 1.0, -0.5]);
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!(
        (sum - 1.0f32).abs() < 1e-4,
        "sum {sum} not within 1e-4 of 1.0"
    );
}

#[test]
fn test_softmax_sum_invariant_extreme_logits() {
    // Very large logits — max-subtraction must prevent NaN/inf
    let scores = softmax_3class(&[100.0_f32, -50.0, -50.0]);
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!(
        (sum - 1.0f32).abs() < 1e-4,
        "sum {sum} not within 1e-4 of 1.0 for extreme logits"
    );
    assert!(scores.entailment.is_finite(), "entailment must be finite");
    assert!(scores.neutral.is_finite(), "neutral must be finite");
    assert!(
        scores.contradiction.is_finite(),
        "contradiction must be finite"
    );
}

#[test]
fn test_softmax_all_equal_logits() {
    // Equal logits → uniform distribution ≈ 1/3 each
    let scores = softmax_3class(&[0.0_f32, 0.0, 0.0]);
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!((sum - 1.0f32).abs() < 1e-4);
    assert!((scores.entailment - 1.0f32 / 3.0).abs() < 1e-4);
}

#[test]
fn test_softmax_no_nan_no_inf() {
    for logits in [
        [0.0_f32, 0.0, 0.0],
        [100.0, -100.0, 50.0],
        [-1000.0, -1000.0, -1000.0],
        [f32::MAX / 2.0, 0.0, 0.0],
    ] {
        let scores = softmax_3class(&logits);
        assert!(!scores.entailment.is_nan(), "NaN entailment for {logits:?}");
        assert!(!scores.neutral.is_nan(), "NaN neutral for {logits:?}");
        assert!(
            !scores.contradiction.is_nan(),
            "NaN contradiction for {logits:?}"
        );
        assert!(
            scores.entailment.is_finite(),
            "inf entailment for {logits:?}"
        );
        assert!(scores.neutral.is_finite(), "inf neutral for {logits:?}");
        assert!(
            scores.contradiction.is_finite(),
            "inf contradiction for {logits:?}"
        );
    }
}

// ---- Per-side truncation (no model required) ----

#[test]
fn test_truncate_input_short_text_unchanged() {
    let text = "hello world";
    assert_eq!(truncate_input(text), text);
}

#[test]
fn test_truncate_input_exact_limit_unchanged() {
    let text = "a".repeat(PER_SIDE_CHAR_LIMIT);
    let result = truncate_input(&text);
    assert_eq!(result.len(), PER_SIDE_CHAR_LIMIT);
}

#[test]
fn test_truncate_input_over_limit_truncated() {
    let text = "a".repeat(PER_SIDE_CHAR_LIMIT + 100);
    let result = truncate_input(&text);
    assert!(result.len() <= PER_SIDE_CHAR_LIMIT);
}

#[test]
fn test_truncate_input_2001_chars_does_not_panic() {
    // AC-03 security requirement: 2001-char input must not panic
    let text = "x".repeat(2001);
    let result = truncate_input(&text);
    assert!(result.len() <= PER_SIDE_CHAR_LIMIT);
}

#[test]
fn test_truncate_input_valid_utf8_boundary() {
    // Multi-byte UTF-8: truncation must land on a char boundary
    let text = "日本語テスト".repeat(500); // each char = 3 bytes
    let result = truncate_input(&text);
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    assert!(result.len() <= PER_SIDE_CHAR_LIMIT);
}

#[test]
fn test_truncate_input_cjk_no_panic() {
    // 1000 CJK characters (3 bytes each = 3000 bytes, 1000 chars)
    let text = "中".repeat(1000);
    let result = truncate_input(&text);
    assert!(result.len() <= PER_SIDE_CHAR_LIMIT);
    // Verify it's valid UTF-8
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
}

// ---- NliProvider Send + Sync (compile-time, no model required) ----

#[test]
fn test_nli_provider_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<NliProvider>();
}

#[test]
fn test_cross_encoder_provider_object_safe() {
    // CrossEncoderProvider must be object-safe for use as dyn CrossEncoderProvider
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn CrossEncoderProvider>();
}

// ---- Model-dependent tests (require model on disk, marked #[ignore]) ----

fn deberta_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap()
        .join(".cache/unimatrix/models")
        .join(NliModel::NliDebertaV3Small.cache_subdir())
}

fn model_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    std::path::PathBuf::from(home)
        .join(".cache/unimatrix/models")
        .join(NliModel::NliMiniLM2L6H768.cache_subdir())
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk; run with --include-ignored when cached"]
fn test_nli_scores_sum_within_tolerance() {
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let scores = provider
        .score_pair("sky is blue", "the sky has blue color")
        .unwrap();
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!(
        (sum - 1.0f32).abs() < 1e-4,
        "NliScores sum {sum} not within 1e-4 of 1.0"
    );
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk"]
fn test_score_batch_empty_returns_empty_vec() {
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let result = provider.score_batch(&[]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk"]
fn test_score_pair_oversized_inputs_no_panic() {
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let long_query = "a ".repeat(5000); // 10,000 chars
    let long_passage = "b ".repeat(5000); // 10,000 chars
    let result = provider.score_pair(&long_query, &long_passage);
    assert!(
        result.is_ok(),
        "score_pair must not error on oversized inputs"
    );
    let scores = result.unwrap();
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!((sum - 1.0f32).abs() < 1e-4);
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk"]
fn test_score_pair_huge_input_no_panic() {
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let huge = "x ".repeat(50_000); // 100,000 chars
    let result = provider.score_pair(&huge, "short passage");
    assert!(result.is_ok());
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk"]
fn test_score_batch_sequential_multiple_pairs() {
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let pairs: Vec<(&str, &str)> = vec![
        ("The cat sat on the mat", "A cat is on a mat"),
        ("Water is wet", "Fire is hot"),
        ("Dogs are animals", "A dog is a type of animal"),
    ];
    let result = provider.score_batch(&pairs);
    assert!(result.is_ok());
    let scores = result.unwrap();
    assert_eq!(scores.len(), 3);
    for score in &scores {
        let sum = score.entailment + score.neutral + score.contradiction;
        assert!(
            (sum - 1.0f32).abs() < 1e-4,
            "sum {sum} not within 1e-4 of 1.0"
        );
        assert!(score.entailment.is_finite());
        assert!(score.neutral.is_finite());
        assert!(score.contradiction.is_finite());
    }
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk"]
fn test_concurrent_score_pair_no_deadlock() {
    use std::sync::Arc;
    let provider = Arc::new(NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap());
    let p1 = Arc::clone(&provider);
    let p2 = Arc::clone(&provider);
    let t1 = std::thread::spawn(move || p1.score_pair("query a", "passage a").unwrap());
    let t2 = std::thread::spawn(move || p2.score_pair("query b", "passage b").unwrap());
    // Both must complete without deadlock
    let _ = t1.join().unwrap();
    let _ = t2.join().unwrap();
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk"]
fn test_score_pair_511_plus_10_tokens_valid() {
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let query_511 = (0..511)
        .map(|i| format!("tok{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let result = provider.score_pair(&query_511, "short ten word passage here yes indeed okay");
    assert!(result.is_ok());
    let scores = result.unwrap();
    let sum = scores.entailment + scores.neutral + scores.contradiction;
    assert!((sum - 1.0f32).abs() < 1e-4, "Sum: {sum}");
}

#[test]
#[ignore = "Requires NliMiniLM2L6H768 model on disk"]
fn test_score_pair_extreme_logits_finite_output() {
    // Verify that a strongly contradictory pair produces finite, non-NaN scores
    let provider = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let scores = provider
        .score_pair(
            "The building is on fire",
            "The building is perfectly safe with no hazards",
        )
        .unwrap();
    assert!(scores.entailment.is_finite());
    assert!(scores.neutral.is_finite());
    assert!(scores.contradiction.is_finite());
    assert!(!scores.entailment.is_nan());
    assert!(!scores.contradiction.is_nan());
}

#[test]
#[ignore = "Requires both NLI models on disk; run with --include-ignored"]
fn test_compare_minilm2_vs_deberta_scores() {
    let query = "idempotency sentinel duplicate detection MCP store";
    let passage = "spawn_blocking Pool Saturation from Unbatched Fire-and-Forget DB Writes";

    let ml2 = NliProvider::new(NliModel::NliMiniLM2L6H768, &model_path()).unwrap();
    let deb = NliProvider::new(NliModel::NliDebertaV3Small, &deberta_path()).unwrap();

    let ml2_scores = ml2.score_pair(query, passage).unwrap();
    let deb_scores = deb.score_pair(query, passage).unwrap();

    println!(
        "MiniLM2:  entailment={:.4} neutral={:.4} contradiction={:.4}",
        ml2_scores.entailment, ml2_scores.neutral, ml2_scores.contradiction
    );
    println!(
        "DeBERTa:  entailment={:.4} neutral={:.4} contradiction={:.4}",
        deb_scores.entailment, deb_scores.neutral, deb_scores.contradiction
    );

    // The two models should produce meaningfully different scores for this pair.
    // If they're identical, something is wrong with model loading.
    let entailment_diff = (ml2_scores.entailment - deb_scores.entailment).abs();
    println!("Entailment diff: {:.4}", entailment_diff);
}
