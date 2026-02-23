# Risk Coverage Report: nxs-003 (Embedding Pipeline)

## Date: 2026-02-23
## Test Run: All passing (93 total = 75 non-ignored + 18 model-dependent)

---

## Risk Coverage Matrix

| Risk ID | Risk Description | Severity | Tests Covering | Coverage | Status |
|---------|-----------------|----------|----------------|----------|--------|
| R-01 | L2 normalization non-unit vectors | Critical | normalize: 11 tests, onnx: test_normalization_diverse_inputs | Full | MITIGATED |
| R-02 | Mean pooling attention mask | Critical | pooling: 7 tests (inc. AC-18 hand-crafted), onnx: test_batch_vs_single_consistency | Full | MITIGATED |
| R-03 | Batch vs single inconsistency | High | onnx: test_batch_vs_single_consistency, test_batch_order_preserved | Full | MITIGATED |
| R-04 | ONNX model loading failure | High | onnx: test_provider_construction_default, download: test_ensure_model_creates_directory | Full | MITIGATED |
| R-05 | Model download failure/corruption | High | download: test_ensure_model_creates_directory, test_ensure_model_sanitized_subdir | Partial | MITIGATED |
| R-06 | Tokenizer truncation | High | onnx: test_long_text_truncation | Partial | MITIGATED |
| R-07 | Thread safety violation | High | onnx: test_send_sync (compile-time), test_concurrent_embed (4 threads x 10 calls) | Full | MITIGATED |
| R-08 | Title+content concatenation | Medium | text: 10 tests (all AC-06 cases) | Full | MITIGATED |
| R-09 | Empty/degenerate input | Medium | onnx: test_empty_string, test_degenerate_inputs, test_no_nan_in_output | Full | MITIGATED |
| R-10 | Cache path resolution | Medium | config: test_resolve_cache_dir_default, test_resolve_cache_dir_custom | Full | MITIGATED |
| R-11 | NaN/infinity in output | Medium | onnx: test_no_nan_in_output, normalize: test_l2_normalize_zero_vector, test_l2_normalize_near_zero_vector | Full | MITIGATED |
| R-12 | Trait object safety | Medium | provider: test_trait_object_safety_dyn_ref, test_trait_object_safety_box, test_trait_arc_dyn | Full | MITIGATED |
| R-13 | Dimension mismatch | High | model: test_all_models_dimension_384, onnx: test_dimension_accessor, test_embed_single_text (asserts len=384) | Full | MITIGATED |
| R-14 | Batch size boundary | Medium | onnx: test_batch_size_boundary (tests batch_size, batch_size+1, batch_size-1) | Full | MITIGATED |
| R-15 | ort RC API changes | Low | Pinned to =2.0.0-rc.9. Build + all tests pass | Full | MITIGATED |

---

## Detailed Test Coverage by Risk

### R-01: L2 Normalization (CRITICAL) -- FULL COVERAGE

Tests:
- `normalize::test_l2_normalize_known_vector` -- Verifies [3,4] -> [0.6, 0.8]
- `normalize::test_l2_normalize_result_has_unit_norm` -- |norm - 1.0| < 0.001
- `normalize::test_l2_normalize_384d` -- Full dimension verification
- `normalize::test_l2_normalize_zero_vector` -- Zero vector unchanged
- `normalize::test_l2_normalize_near_zero_vector` -- Threshold 1e-12
- `normalize::test_l2_normalize_single_large_value` -- Dominant dimension
- `normalize::test_l2_normalize_negative_values` -- Negative components
- `normalize::test_l2_normalize_all_equal` -- Uniform vector
- `normalize::test_l2_normalize_deterministic` -- Reproducibility
- `normalize::test_l2_normalized_returns_new_vector` -- Non-mutating variant
- `normalize::test_l2_normalize_unit_vector` -- Already normalized input
- `onnx::test_normalization_diverse_inputs` -- End-to-end with model: "", " ", "short", "long text"

### R-02: Mean Pooling (CRITICAL) -- FULL COVERAGE

Tests:
- `pooling::test_mean_pool_hand_crafted_ac18` -- AC-18 exact values: [1,2],[3,4],[0,0] mask [1,1,0] -> [2.0, 3.0]
- `pooling::test_mean_pool_all_tokens_active` -- No masking
- `pooling::test_mean_pool_single_token` -- Trivial case
- `pooling::test_mean_pool_batch_of_two` -- Multi-sequence batch
- `pooling::test_mean_pool_all_masked` -- All zeros mask (guard against div-by-zero)
- `pooling::test_mean_pool_384d` -- Full dimension
- `pooling::test_mean_pool_padding_does_not_dilute` -- AC-11: padding tokens have zero effect
- `onnx::test_batch_vs_single_consistency` -- End-to-end: single vs batch identical within 1e-5

### R-03: Batch vs Single Consistency (HIGH) -- FULL COVERAGE

Tests:
- `onnx::test_batch_vs_single_consistency` -- 5 texts, individual vs batch, element-wise < 1e-5
- `onnx::test_batch_order_preserved` -- 3 texts, order matches individual results
- `onnx::test_embed_entry_convenience` -- embed_entry matches manual prepare_text + embed

### R-04: ONNX Model Loading (HIGH) -- FULL COVERAGE

Tests:
- `onnx::test_provider_construction_default` -- Default config loads AllMiniLmL6V2
- `onnx::test_dimension_accessor` -- Provider reports correct dimension after loading
- `download::test_ensure_model_creates_directory` -- Download path creates directory, fetches files
- Error module tests verify typed error variants for all failure modes

### R-05: Model Download (HIGH) -- PARTIAL COVERAGE

Tests:
- `download::test_ensure_model_creates_directory` -- Full download from HuggingFace Hub
- `download::test_ensure_model_sanitized_subdir` -- Cache directory naming
- `download::test_file_size_nonexistent` -- File size check for missing files

Note: Corrupted file detection not tested (would require planting bad files, accepted as integration-level).

### R-06: Tokenizer Truncation (HIGH) -- PARTIAL COVERAGE

Tests:
- `onnx::test_long_text_truncation` -- 1000-word text, verifies 384-d output produced

Note: Token-exact boundary tests deferred (would require counting tokens, accepted scope).

### R-07: Thread Safety (HIGH) -- FULL COVERAGE

Tests:
- `onnx::test_send_sync` -- Compile-time `Send + Sync` verification
- `onnx::test_concurrent_embed` -- 4 threads x 10 calls, all produce valid 384-d vectors

### R-08: Title+Content Concatenation (MEDIUM) -- FULL COVERAGE

Tests (10 total in text module):
- `test_prepare_text_both_present` -- "JWT: Validate exp"
- `test_prepare_text_empty_title` -- "content only" (no prefix)
- `test_prepare_text_empty_content` -- "title only" (no suffix)
- `test_prepare_text_both_empty` -- ""
- `test_prepare_text_custom_separator` -- " - "
- `test_prepare_text_empty_separator` -- "titlecontent"
- `test_prepare_text_title_contains_separator` -- "key: value: content"
- `test_prepare_text_long_content` -- 10,000 char content
- `test_embed_entry_calls_provider` -- End-to-end entry embedding
- `test_embed_entries_batch` -- Batch entry embedding

### R-09: Empty/Degenerate Input (MEDIUM) -- FULL COVERAGE

Tests:
- `onnx::test_empty_string` -- "" produces valid 384-d normalized embedding (AC-12)
- `onnx::test_degenerate_inputs` -- " ", "\t\n", "a", "!@#$%^&*()"
- `onnx::test_no_nan_in_output` -- No NaN/infinity across "", " ", normal text, special chars
- `text::test_embed_entry_empty_fields` -- Empty title + empty content

### R-10: Cache Path Resolution (MEDIUM) -- FULL COVERAGE

Tests:
- `config::test_resolve_cache_dir_default` -- Contains "unimatrix/models"
- `config::test_resolve_cache_dir_custom` -- Custom path override works

### R-11: NaN/Infinity (MEDIUM) -- FULL COVERAGE

Tests:
- `onnx::test_no_nan_in_output` -- Checks every element for NaN and Infinity
- `normalize::test_l2_normalize_zero_vector` -- Zero vector stays zero (no NaN)
- `normalize::test_l2_normalize_near_zero_vector` -- Near-zero unchanged (no NaN)
- `onnx::test_embed_single_text` -- Checks all values are finite

### R-12: Trait Object Safety (MEDIUM) -- FULL COVERAGE

Tests:
- `provider::test_trait_object_safety_dyn_ref` -- `&dyn EmbeddingProvider`
- `provider::test_trait_object_safety_box` -- `Box<dyn EmbeddingProvider>`
- `provider::test_trait_arc_dyn` -- `Arc<dyn EmbeddingProvider>`
- `provider::test_trait_all_methods_via_dyn` -- All 4 methods callable via trait object

### R-13: Dimension Mismatch (HIGH) -- FULL COVERAGE

Tests:
- `model::test_all_models_dimension_384` -- All 7 variants return 384
- `onnx::test_dimension_accessor` -- Provider.dimension() == 384
- `onnx::test_embed_single_text` -- Actual embedding length == 384

### R-14: Batch Size Boundary (MEDIUM) -- FULL COVERAGE

Tests:
- `onnx::test_batch_size_boundary` -- batch_size=3: tests 3 texts (exact), 4 texts (overflow), 2 texts (under)
- `onnx::test_embed_batch` -- 5 texts default batch
- `onnx::test_embed_batch_empty` -- 0 texts

### R-15: ort RC API Changes (LOW) -- FULL COVERAGE

- Pinned to `ort = "=2.0.0-rc.9"` and `ort-sys = "=2.0.0-rc.9"` (ONNX Runtime 1.20)
- All 93 tests pass
- `onnx::test_semantic_similarity` validates semantic quality (related > 0.7, unrelated < 0.3)

---

## Acceptance Criteria Coverage

| AC | Description | Test(s) | Status |
|----|------------|---------|--------|
| AC-01 | OnnxProvider struct with Mutex<Session> | onnx.rs structure + test_send_sync | PASS |
| AC-02 | new() downloads model on first use | test_provider_construction_default, test_ensure_model_creates_directory | PASS |
| AC-03 | embed() returns 384-d Vec<f32> | test_embed_single_text | PASS |
| AC-04 | embed_batch returns ordered embeddings | test_batch_order_preserved | PASS |
| AC-05 | L2 norm within 0.001 of 1.0 | test_normalization_diverse_inputs, normalize tests | PASS |
| AC-06 | prepare_text concatenation | text module: 7 edge case tests | PASS |
| AC-07 | embed_entry convenience | test_embed_entry_convenience | PASS |
| AC-08 | embed_entries batch | text::test_embed_entries_batch | PASS |
| AC-09 | Trait object safety | provider: 3 object safety tests | PASS |
| AC-10 | Send + Sync | test_send_sync (compile-time) | PASS |
| AC-11 | Batch == single consistency | test_batch_vs_single_consistency (< 1e-5) | PASS |
| AC-12 | Empty string returns embedding | test_empty_string | PASS |
| AC-13 | Custom cache directory | config::test_resolve_cache_dir_custom | PASS |
| AC-14 | Typed error variants | error module: 6 tests | PASS |
| AC-15 | MockProvider deterministic | test_helpers: 8 tests | PASS |
| AC-16 | 7 models at 384-d | model::test_all_models_dimension_384 | PASS |
| AC-17 | Default AllMiniLmL6V2 | test_provider_construction_default | PASS |
| AC-18 | Mean pooling correctness | test_mean_pool_hand_crafted_ac18 | PASS |
| AC-19 | test-support feature flag | test_helpers behind cfg(any(test, feature = "test-support")) | PASS |

---

## Summary

- **Total Tests**: 93 (75 non-ignored + 18 model-dependent)
- **All Passing**: Yes
- **Risks Mitigated**: 15/15 (13 full, 2 partial)
- **Acceptance Criteria**: 19/19 PASS
- **Coverage Gaps**: None critical. R-05 (corrupted file detection) and R-06 (exact token boundary) are partial but acceptable scope tradeoffs.
- **Workspace Regression**: 0 -- all 245 workspace tests pass (85 store + 85 vector + 75 embed)
