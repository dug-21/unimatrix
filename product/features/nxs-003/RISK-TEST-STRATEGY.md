# Risk-Based Test Strategy: nxs-003 (Embedding Pipeline)

**Feature**: nxs-003 (Nexus Phase)
**Agent**: nxs-003-agent-3-risk
**Date**: 2026-02-23

---

## 1. Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | L2 normalization produces non-unit vectors | Critical | Medium | Critical |
| R-02 | Mean pooling incorrectly handles attention mask | Critical | Medium | Critical |
| R-03 | Batch vs single embedding inconsistency | High | Medium | High |
| R-04 | ONNX model loading failure | High | Medium | High |
| R-05 | Model download failure or corruption | High | Medium | High |
| R-06 | Tokenizer truncation loses critical content | High | Medium | High |
| R-07 | ONNX session thread safety violation | High | Low | High |
| R-08 | Title+content concatenation edge cases | Medium | High | Medium |
| R-09 | Empty/degenerate input handling | Medium | High | Medium |
| R-10 | Model cache path resolution failure | Medium | Medium | Medium |
| R-11 | ONNX inference produces NaN/infinity | Medium | Low | Medium |
| R-12 | EmbeddingProvider trait object safety | Medium | Low | Medium |
| R-13 | Model catalog dimension mismatch | High | Low | Medium |
| R-14 | Batch size boundary conditions | Medium | Medium | Medium |
| R-15 | ort RC API breaking changes | Medium | Low | Low |

---

## 2. Risk-to-Scenario Mapping

### R-01: L2 Normalization Produces Non-Unit Vectors (CRITICAL)

**Severity**: Critical
**Likelihood**: Medium — floating-point arithmetic can accumulate error; zero-vector edge case produces division by zero; very small norms amplify noise.
**Impact**: nxs-002's DistDot metric requires L2-normalized vectors. Non-unit vectors produce incorrect similarity scores. At 0.92 near-duplicate threshold (vnc-002), even small normalization errors cause false positives or false negatives. This silently corrupts every downstream search result.

**Test Scenarios**:
1. Embed a known text. Compute L2 norm of output. Verify |norm - 1.0| < 0.001 (AC-05).
2. Embed 100 diverse texts. Verify all output norms satisfy |norm - 1.0| < 0.001.
3. Embed a very short text (single word "a"). Verify normalization holds.
4. Embed a very long text (fills entire 256-token window). Verify normalization holds.
5. Verify that the normalization function handles a near-zero input vector (all values < 1e-12) without producing NaN or infinity — returns zero vector or raises error.
6. Verify that the normalization function handles a vector with one very large value and rest near-zero — output is still unit-length.
7. Embed the same text twice. Verify identical normalized output (determinism).

**Coverage Requirement**: Every embedding output path (single, batch, embed_entry) must produce L2-normalized vectors. Normalization function must handle degenerate inputs (zero vector, near-zero norm). AC-05 tolerance (0.001) verified across diverse inputs.

---

### R-02: Mean Pooling Incorrectly Handles Attention Mask (CRITICAL)

**Severity**: Critical
**Likelihood**: Medium — attention mask weighting during mean pooling is a subtle operation. Off-by-one in mask application, wrong broadcasting, or using token_type_ids instead of attention_mask would produce silently wrong embeddings.
**Impact**: If padding tokens contribute to the pooled embedding, shorter texts padded within a batch get diluted embeddings. Semantic similarity scores degrade. The embedding quality is silently worse — no error is raised, results are just wrong.

**Test Scenarios**:
1. Embed "hello world" as a single text (no padding needed). Record embedding.
2. Embed "hello world" in a batch with a much longer text. Verify "hello world" embedding matches the single-text embedding within floating-point tolerance (AC-11). If padding tokens leaked into pooling, the batch embedding would differ.
3. Embed a text that is exactly at the token limit (256 tokens for MiniLM). Verify no padding applied — attention mask is all 1s.
4. Embed a very short text ("a") in a batch with a long text. Verify the short text's embedding is not dominated by zero-padding dimensions.
5. Verify that mean pooling divides by the sum of the attention mask (count of real tokens), not by sequence length.

**Coverage Requirement**: Batch embedding must produce identical results to individual embedding (AC-11). This is the primary signal that attention mask pooling is correct.

---

### R-03: Batch vs Single Embedding Inconsistency (HIGH)

**Severity**: High
**Likelihood**: Medium — batch processing involves padding to the longest sequence in the batch. Different batch compositions change the padding, which interacts with attention mask pooling. If pooling is wrong (R-02), batch results diverge from single results.
**Impact**: Downstream consumers cannot trust that bulk-import embeddings match incremental-insert embeddings. Semantic search quality depends on which code path generated the embedding.

**Test Scenarios**:
1. Embed 10 texts individually via `embed()`. Embed the same 10 texts via `embed_batch()`. Compare all 10 embeddings pairwise — verify element-wise difference < 1e-5 (AC-11).
2. Embed 10 texts in a single batch of 10. Embed in two batches of 5. Compare — verify identical results.
3. Embed a single text via `embed()` and via `embed_batch(&[text])`. Verify identical output.
4. Embed texts of vastly different lengths in one batch (3 words, 50 words, 200 words). Verify each embedding matches its individual `embed()` result.
5. `embed_entry(provider, title, content)` must produce the same result as `embed(prepare_text(title, content, ": "))`.

**Coverage Requirement**: AC-11 compliance verified across diverse batch compositions. Floating-point tolerance: element-wise difference < 1e-5.

---

### R-04: ONNX Model Loading Failure (HIGH)

**Severity**: High
**Likelihood**: Medium — model loading depends on: ONNX file existing at expected path, file not corrupted, ort runtime initialized, model compatible with ort version, tokenizer.json present and valid.
**Impact**: `OnnxProvider::new()` fails. No embedding capability. The entire write path (store → embed → vector insert) is broken.

**Test Scenarios**:
1. Create `OnnxProvider` with default config (AllMiniLmL6V2) — verify successful construction (AC-02).
2. Create `OnnxProvider` with each catalog model variant — verify all 7 load successfully (AC-17).
3. Point config at non-existent cache directory — verify `EmbedError` (not panic) (AC-14).
4. Point config at a directory with corrupted/truncated model.onnx — verify `EmbedError`.
5. Point config at a directory with missing tokenizer.json — verify `EmbedError`.
6. Point config at a directory with a valid ONNX file but wrong architecture (not a sentence-transformer) — verify `EmbedError` or dimension mismatch.
7. Verify error types distinguish model loading errors from tokenization errors from inference errors (AC-14).

**Coverage Requirement**: All failure modes during construction produce typed `EmbedError` variants. No panics. Happy path verified for all 7 catalog models.

---

### R-05: Model Download Failure or Corruption (HIGH)

**Severity**: High
**Likelihood**: Medium — first-use download depends on network connectivity, HuggingFace Hub availability, DNS resolution, and sufficient disk space. Download may be interrupted, producing partial files.
**Impact**: First-time users cannot create an `OnnxProvider`. If partial files are cached, subsequent attempts may load corrupted data instead of re-downloading.

**Test Scenarios**:
1. Verify that when the model cache directory is empty, `OnnxProvider::new()` triggers a download (AC-02). (Requires network; may be integration-only test.)
2. Verify that when the model is already cached, `OnnxProvider::new()` succeeds without network access.
3. Create a partial/corrupted model file in the cache directory. Verify that loading fails with a descriptive error (not silent corruption).
4. Verify that the download path creates the cache directory if it doesn't exist.
5. Verify that the model cache path uses the sanitized model ID as subdirectory name (e.g., `sentence-transformers_all-MiniLM-L6-v2/`).

**Coverage Requirement**: Download success path tested (integration test with network). Corrupted cache detection tested (unit test with planted bad files). Cache directory creation tested.

---

### R-06: Tokenizer Truncation Loses Critical Content (HIGH)

**Severity**: High
**Likelihood**: Medium — the 256-token limit (MiniLM models) is easily exceeded by content-heavy entries. Truncation from the end preserves title but silently drops content tail.
**Impact**: Embeddings for long entries represent only the first ~256 tokens. Two entries with identical beginnings but different endings produce near-identical embeddings, reducing search quality. The truncation is silent — no warning or error.

**Test Scenarios**:
1. Create text that is exactly at the 256-token limit. Verify embedding is produced without truncation.
2. Create text that exceeds the limit by 1 token. Verify embedding is produced (truncated, not error).
3. Create text with 1000 tokens. Verify embedding is produced. Compare to the first-256-tokens version — verify high similarity (confirming truncation behavior, not random failure).
4. Verify that title is always preserved: `prepare_text("important title", <very_long_content>, ": ")` — the title tokens appear at the start and survive truncation.
5. Test with models that have 512-token limit (BgeSmallEnV15, E5SmallV2, GteSmall) — verify their longer context is actually used.

**Coverage Requirement**: Truncation behavior documented and tested. Title preservation verified. Different max sequence lengths across catalog models verified.

---

### R-07: ONNX Session Thread Safety Violation (HIGH)

**Severity**: High
**Likelihood**: Low — the SCOPE notes ONNX session requires `&mut self`. Thread safety is achieved via Mutex or serialized access. If the Mutex is omitted or bypassed, concurrent inference causes undefined behavior.
**Impact**: Data races in ONNX Runtime. Potential crashes, corrupted output tensors, or segfaults in the native ONNX Runtime library (beneath Rust's safety layer, inside `ort`'s FFI).

**Test Scenarios**:
1. Verify `OnnxProvider` is `Send + Sync` (AC-10). Compile-time test: `fn assert_send_sync<T: Send + Sync>() {} assert_send_sync::<OnnxProvider>();`.
2. Share `Arc<OnnxProvider>` across 4 threads. Each thread calls `embed()` 50 times concurrently. Verify all produce valid 384-d normalized vectors (no panics, no NaN).
3. Share `Arc<OnnxProvider>` across 4 threads. Each thread calls `embed_batch()` with 10 texts. Verify all produce correct results.
4. Verify that concurrent callers do not get each other's results (embed different texts, verify different embeddings).

**Coverage Requirement**: `Send + Sync` compile-time assertion. Concurrent correctness test with meaningful load. No panics or data races under concurrent access.

---

### R-08: Title+Content Concatenation Edge Cases (MEDIUM)

**Severity**: Medium
**Likelihood**: High — many entries will have unusual title/content combinations (empty title, empty content, very long titles, titles with special characters, same text in title and content).
**Impact**: Incorrect concatenation produces unexpected embeddings. If separator is included when title or content is empty, the embedding captures the separator as semantic content.

**Test Scenarios**:
1. `prepare_text("title", "content", ": ")` → `"title: content"` (AC-06).
2. `prepare_text("", "content", ": ")` → `"content"` (not `": content"`) (AC-06).
3. `prepare_text("title", "", ": ")` → `"title"` (not `"title: "`) (AC-06).
4. `prepare_text("", "", ": ")` → `""` (empty string).
5. `prepare_text("title", "content", " - ")` → `"title - content"` (custom separator).
6. Title with separator characters: `prepare_text("key: value", "content", ": ")` → `"key: value: content"`.
7. Very long title (500 chars) + short content → verify title dominates the token window.
8. Unicode title + ASCII content → verify concatenation preserves encoding.

**Coverage Requirement**: All AC-06 edge cases (empty title, empty content, both empty). Custom separator. Unicode handling.

---

### R-09: Empty/Degenerate Input Handling (MEDIUM)

**Severity**: Medium
**Likelihood**: High — empty strings, whitespace-only strings, and single-character strings are common edge cases. The SCOPE specifies empty string returns an embedding (AC-12), not an error.
**Impact**: If empty input causes a panic or NaN output, downstream consumers crash or insert corrupted vectors into nxs-002 (which validates for NaN and would reject them).

**Test Scenarios**:
1. `embed("")` returns a valid 384-d embedding (AC-12). Not an error.
2. `embed(" ")` (whitespace only) returns a valid embedding.
3. `embed("a")` (single character) returns a valid 384-d normalized embedding.
4. `embed_batch(&[])` (empty batch) returns empty Vec (not error).
5. `embed_batch(&["", "hello", ""])` — all three produce valid embeddings; middle one matches `embed("hello")`.
6. `embed_entry(provider, "", "")` — returns valid embedding (concatenation is empty string).
7. Very long input (10,000 characters) — tokenizer truncates, embedding is valid.
8. Input with only special characters (`"!@#$%^&*()"`).
9. Input with newlines, tabs, null-like characters.
10. Input with emoji and CJK characters.

**Coverage Requirement**: All degenerate inputs produce valid 384-d normalized embeddings (not errors, not NaN). AC-12 explicitly requires empty string returns embedding.

---

### R-10: Model Cache Path Resolution Failure (MEDIUM)

**Severity**: Medium
**Likelihood**: Medium — `dirs::cache_dir()` can return `None` on some platforms or configurations (no HOME variable, sandboxed environments). Custom cache path may point to read-only directory.
**Impact**: `OnnxProvider::new()` fails with an unclear error. User cannot generate embeddings.

**Test Scenarios**:
1. Verify default cache path resolves to `~/.cache/unimatrix/models/` on Linux.
2. Verify custom cache path in `EmbedConfig` overrides the default (AC-13).
3. Verify cache path creates intermediate directories if they don't exist.
4. Point cache path to a read-only directory — verify descriptive error.
5. Verify model files are stored in sanitized subdirectory (e.g., `sentence-transformers_all-MiniLM-L6-v2/`).
6. Verify that slashes in HuggingFace model IDs are converted to underscores or similar safe characters in the directory name.

**Coverage Requirement**: Path resolution for default and custom paths. Directory creation. Sanitized subdirectory naming. Error on inaccessible paths.

---

### R-11: ONNX Inference Produces NaN/Infinity (MEDIUM)

**Severity**: Medium
**Likelihood**: Low — sentence-transformer models are well-behaved for typical text input. However, degenerate tokenizer output (all padding, empty attention mask) could produce NaN in the model's internal softmax or layer norm.
**Impact**: NaN propagates through mean pooling and normalization, producing a NaN embedding. nxs-002's `VectorIndex::insert` validates for NaN and rejects the embedding — the insert fails with an error. Not silent corruption, but a broken write path for that entry.

**Test Scenarios**:
1. Embed empty string — verify no NaN in output.
2. Embed whitespace-only string — verify no NaN in output.
3. Embed very long repeated text — verify no NaN.
4. Verify normalization function: if input contains NaN, output should either be an error or a zero vector — not propagated NaN.
5. Verify normalization function: if input contains infinity, return error.

**Coverage Requirement**: NaN/infinity checked for all degenerate inputs. Normalization function has explicit NaN/infinity handling.

---

### R-12: EmbeddingProvider Trait Object Safety (MEDIUM)

**Severity**: Medium
**Likelihood**: Low — the proposed trait uses `&self` methods and returns concrete types (`Vec<f32>`, `usize`, `&str`). This should be object-safe. But if a generic method is accidentally added, trait objects break at compile time.
**Impact**: Downstream consumers (vnc-001, vnc-002) cannot use `Box<dyn EmbeddingProvider>` or `&dyn EmbeddingProvider>`. Forces concrete type dependencies, preventing mock providers for testing.

**Test Scenarios**:
1. Compile-time test: `fn assert_object_safe(_: &dyn EmbeddingProvider) {}` (AC-09).
2. Create a `Box<dyn EmbeddingProvider>` from `OnnxProvider` — verify it compiles and works.
3. Create a mock provider implementing `EmbeddingProvider`. Use it as `&dyn EmbeddingProvider` — verify it compiles.
4. Verify `Arc<dyn EmbeddingProvider>` works for shared ownership across threads.

**Coverage Requirement**: Compile-time object safety verified. Mock provider demonstrates trait usability (AC-19).

---

### R-13: Model Catalog Dimension Mismatch (MEDIUM)

**Severity**: High
**Likelihood**: Low — all 7 catalog models are documented as 384-d. But if a model's actual output dimension differs from the catalog's declared dimension (due to model update, wrong ONNX file, or catalog typo), embeddings are silently wrong-dimensioned.
**Impact**: If `dimension()` returns 384 but actual output is 256 or 768, nxs-002's dimension validation catches it at insert time. But the error is confusing — it looks like a vector index bug, not an embedding bug.

**Test Scenarios**:
1. For each of the 7 catalog models: create provider, embed a text, verify output length == 384 (AC-03, AC-16).
2. Verify `dimension()` returns 384 for each catalog model (AC-16).
3. Verify that actual inference output matches `dimension()` — the declared dimension is not just metadata.
4. If a model produces wrong-dimension output, verify the error is raised at the embedding layer (not deferred to nxs-002 insert).

**Coverage Requirement**: All 7 models validated for correct 384-d output. Declared dimension matches actual output.

---

### R-14: Batch Size Boundary Conditions (MEDIUM)

**Severity**: Medium
**Likelihood**: Medium — batch processing chunks texts into groups of `batch_size`. Off-by-one in chunking logic, or improper handling of the last partial batch, could skip texts or produce wrong results.
**Impact**: Missing embeddings for some texts in a batch call. Or duplicate embeddings if a text is processed twice.

**Test Scenarios**:
1. `embed_batch` with exactly `batch_size` texts (e.g., 32). Verify 32 embeddings returned.
2. `embed_batch` with `batch_size + 1` texts (33). Verify 33 embeddings returned — last batch has 1 text.
3. `embed_batch` with `batch_size - 1` texts (31). Verify 31 embeddings returned.
4. `embed_batch` with 1 text. Verify 1 embedding returned.
5. `embed_batch` with `3 * batch_size` texts. Verify `3 * batch_size` embeddings returned, order preserved.
6. Verify output order matches input order — text at index 0 produces embedding at index 0 (AC-04).
7. Custom batch_size = 1 in config — every text is its own batch. Verify results match default batch_size.

**Coverage Requirement**: Boundary conditions at batch_size, batch_size ± 1. Order preservation. Last partial batch correctness.

---

### R-15: ort RC API Breaking Changes (LOW)

**Severity**: Medium
**Likelihood**: Low — ort 2.0.0-rc.11 is validated by ruvector in production. But RC releases have no stability guarantee.
**Impact**: Compilation failure on ort version bump. Or subtle behavioral change in inference output.

**Test Scenarios**:
1. Pin `ort = "=2.0.0-rc.11"` in Cargo.toml — verify build succeeds.
2. Run embedding tests — verify output matches expected semantic similarity patterns.
3. (Future) If upgrading ort, run full test suite to catch behavioral regressions.

**Coverage Requirement**: Build verification. Semantic output validation provides regression safety net.

---

## 3. Integration Risks

### IR-01: nxs-002 Dimension Contract Violation

nxs-002's `VectorIndex::insert(entry_id, &[f32])` validates `embedding.len() == 384`. If nxs-003 produces a different dimension (due to model misconfiguration or catalog error), the insert fails at runtime.

**Mitigation**: Validate output dimension inside the embedding provider, before returning to the caller. The error should be `EmbedError::DimensionMismatch`, not `VectorError::DimensionMismatch`. Fail fast at the source.

**Test Scenarios**:
1. Embed text via `OnnxProvider`. Pass result to `VectorIndex::insert`. Verify success.
2. Verify `OnnxProvider::dimension()` == `VectorConfig::default().dimension` == 384.

### IR-02: nxs-002 NaN/Infinity Rejection

nxs-002 validates embeddings for NaN and infinity. If nxs-003 produces NaN (R-11), the insert is rejected. The caller gets a `VectorError`, not an `EmbedError`, making debugging harder.

**Mitigation**: Validate for NaN/infinity in the normalization step. Return `EmbedError::InvalidOutput` before the embedding ever reaches nxs-002.

**Test Scenarios**:
1. Verify that `OnnxProvider::embed()` never returns a vector containing NaN for any valid text input.
2. Verify that `OnnxProvider::embed()` never returns a vector containing infinity.

### IR-03: nxs-002 DistDot Requires Normalized Vectors

nxs-002 uses DistDot, which computes `1.0 - dot(a, b)`. For this to produce correct cosine similarity, both vectors must be L2-normalized. If nxs-003 returns non-unit vectors, similarity scores are wrong.

**Mitigation**: R-01 covers normalization correctness. This integration risk elevates the importance of R-01 — it's not just a quality issue, it's a correctness issue for the entire search pipeline.

**Test Scenarios**:
1. Embed two semantically similar texts. Insert both into VectorIndex. Search with one as query. Verify the other appears as top result with similarity > 0.7 (AC-08).
2. Embed two unrelated texts. Insert into VectorIndex. Search. Verify similarity < 0.3 (AC-08).

### IR-04: unimatrix-store EntryRecord Text Fields

The embedding pipeline reads `title` and `content` from `EntryRecord`. If store returns entries with `None` or empty fields in unexpected ways, concatenation may produce bad input.

**Mitigation**: `prepare_text` handles empty title and empty content (AC-06). Downstream callers (vnc-002) must handle the case where an EntryRecord has no meaningful text.

**Test Scenarios**:
1. Call `embed_entry(provider, "", "")` — verify valid embedding returned.
2. Call `embed_entry(provider, "title only", "")` — verify valid embedding.
3. Call `embed_entry(provider, "", "content only")` — verify valid embedding.

### IR-05: First-Use Download Blocks Synchronous Call Path

Model download on first use can take 30-60 seconds over typical connections (~90 MB model file). Since nxs-003 is synchronous, the first `OnnxProvider::new()` call blocks the calling thread for the entire download duration.

**Mitigation**: Document that first-use construction may take minutes. Downstream async consumers (vnc-001) must use `spawn_blocking` for construction. Consider logging progress or providing a pre-download CLI command in a future feature.

**Test Scenarios**:
1. Verify that construction with a pre-cached model completes in < 5 seconds.
2. Verify that construction logs or reports progress during download (if logging is implemented).

---

## 4. Edge Cases

### EC-01: Unicode and Encoding

| Case | Expected Behavior |
|------|-------------------|
| CJK text (Chinese, Japanese, Korean) | Valid embedding. Tokenizer handles multi-byte UTF-8. |
| Emoji-heavy text ("🔐🔒🔓") | Valid embedding. Tokenizer maps to [UNK] or subword tokens. |
| RTL text (Arabic, Hebrew) | Valid embedding. Tokenizer handles bidirectional text. |
| Mixed scripts ("Auth認証") | Valid embedding. Tokenizer handles code-switching. |
| Combining characters (é as e + combining acute) | Valid embedding. May differ from precomposed é embedding. |
| Zero-width characters (ZWSP, ZWJ) | Valid embedding. Tokenizer may or may not include them. |
| Null bytes in string | Rust strings cannot contain null bytes. If input comes from FFI, it's already sanitized. Not a risk. |

### EC-02: Tokenizer Boundary Behavior

| Case | Expected Behavior |
|------|-------------------|
| Exactly 256 tokens (MiniLM limit) | No truncation. Full text used. |
| 257 tokens | Last token truncated. No error. |
| 1 token (single word) | Valid embedding. Attention mask is mostly zeros (padding). |
| All [UNK] tokens (gibberish text) | Valid embedding. Model handles unknown tokens gracefully. |
| Only special tokens after tokenization | Valid embedding. Mean pooling over attention-masked tokens. |

### EC-03: Batch Composition

| Case | Expected Behavior |
|------|-------------------|
| Batch of 1 | Same result as `embed()`. |
| Batch of 1000 (>> batch_size) | Processed in chunks. All 1000 embeddings returned. |
| All identical texts in batch | All embeddings identical. |
| All empty strings in batch | All valid embeddings. All identical. |
| Batch with one very long text and rest short | Padding to longest in each sub-batch. Results match individual embedding. |

### EC-04: Floating-Point Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| Normalization of vector where all elements are equal | Valid unit vector. Each element = 1/sqrt(384). |
| Normalization of vector with one large element, rest zero | Valid unit vector = [0, ..., 0, 1, 0, ..., 0]. |
| Normalization of near-zero vector (all elements < 1e-12) | Return zero vector or error. Do NOT divide by near-zero norm (amplifies noise to unit length). |
| Cosine similarity of identical embeddings | ~1.0 (within f32 tolerance). |
| Cosine similarity of unrelated embeddings | Low (< 0.3 for truly unrelated content). |

### EC-05: Config Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| batch_size = 0 | Error or treated as 1. Not panic. |
| batch_size = 1 | Every text is its own batch. Correct but slow. |
| batch_size = usize::MAX | Single batch for any input size. May OOM for very large inputs. |
| Empty separator string | `prepare_text("title", "content", "")` → `"titlecontent"`. |
| Cache directory with spaces in path | Works (Path handles spaces natively). |
| Cache directory path doesn't exist yet | Created automatically. |

---

## 5. Security Risks

### SR-01: Model Supply Chain (Model Tampering)

**Untrusted Input**: ONNX model files downloaded from HuggingFace Hub.
**Threat**: A compromised or malicious model could: (a) produce biased/wrong embeddings that manipulate search results, (b) exploit an ONNX Runtime vulnerability via crafted model tensors, (c) contain unexpected operations that abuse CPU/memory.
**Blast Radius**: The ONNX model runs inside `ort` which links to the native ONNX Runtime C++ library. A malicious model could potentially cause arbitrary code execution if an ONNX Runtime vulnerability exists. However, `#![forbid(unsafe_code)]` only applies to Rust code — `ort` uses FFI internally.
**Mitigation**: (a) Use only well-known, widely-used models from established organizations (sentence-transformers, BAAI, intfloat, thenlper). (b) The pre-configured catalog restricts model selection — users cannot load arbitrary ONNX files through the public API. (c) Future: add SHA256 checksum verification for downloaded model files.
**Current Risk Level**: Low — all catalog models are widely-used open-source models with thousands of downloads. No checksum verification is a gap but acceptable for v1.

### SR-02: Untrusted Text Input to Tokenizer

**Untrusted Input**: Text strings passed to `embed()` and `embed_batch()`.
**Threat**: Adversarial text could: (a) cause tokenizer to allocate excessive memory (very long input), (b) trigger tokenizer bugs with unusual Unicode sequences, (c) cause ONNX inference to produce unexpected outputs.
**Blast Radius**: Memory exhaustion from very long text input. Tokenizer and ONNX Runtime handle text as data, not executable — no injection risk.
**Mitigation**: (a) Tokenizer truncation limits token count (256 or 512 depending on model). This bounds memory usage per text. (b) Batch size is configurable, bounding per-batch memory. (c) The embedding crate is a library — input validation is the caller's responsibility, but the crate must not panic on any input.
**Current Risk Level**: Low — tokenizer truncation provides natural bounds. No code execution from text input.

### SR-03: Cache Directory as Attack Surface

**Untrusted Input**: File system state in the model cache directory.
**Threat**: A local attacker could: (a) replace cached model files with malicious ones (see SR-01), (b) create symlinks in the cache directory pointing to sensitive files, (c) fill the cache directory to exhaust disk space.
**Blast Radius**: Same as SR-01 if model files are replaced. Symlink following could cause model loading to read unexpected files, but ONNX Runtime validates model format.
**Mitigation**: (a) Cache directory permissions should be user-only (0700). (b) Do not follow symlinks when resolving model paths (use canonical path resolution). (c) This is a local attack — if the attacker has local file system access, there are simpler attack vectors.
**Current Risk Level**: Very Low — local-only attack surface. Standard file system permission model.

### SR-04: Network Security During Model Download

**Untrusted Input**: Network responses from HuggingFace Hub during model download.
**Threat**: Man-in-the-middle attack during download could substitute a malicious model file.
**Blast Radius**: Same as SR-01.
**Mitigation**: (a) `hf-hub` crate uses HTTPS by default. (b) Future: SHA256 checksum verification. (c) Download only happens on first use — subsequent runs use cached model.
**Current Risk Level**: Very Low — HTTPS provides transport security.

---

## 6. Failure Modes

### FM-01: Model Not Available (First-Use Download Failure)

**Trigger**: No network connectivity, HuggingFace Hub down, DNS failure, firewall blocking HTTPS.
**Behavior**: `OnnxProvider::new()` returns `EmbedError::Download`. No embedding capability.
**Recovery**: Retry when network is available. Or pre-cache the model by running once with network. Or copy model files from another machine into the cache directory.
**User-Facing**: Clear error message indicating network download failed and suggesting pre-caching.

### FM-02: Corrupted Model Cache

**Trigger**: Interrupted download, disk corruption, manual file modification.
**Behavior**: `OnnxProvider::new()` returns `EmbedError::ModelLoad` with details about which file is corrupted.
**Recovery**: Delete the model subdirectory from cache. Re-download on next construction.
**Prevention**: Future: checksum verification on load. Current: model format validation by ort during session creation catches most corruption.

### FM-03: ONNX Runtime Initialization Failure

**Trigger**: Missing ONNX Runtime shared library, incompatible platform, resource exhaustion.
**Behavior**: `ort::Session::builder()` fails. `EmbedError::ModelLoad` returned.
**Recovery**: Verify ort installation. The `download-binaries` feature flag in ort handles this for most platforms.
**User-Facing**: Error should include platform information and suggest checking ort compatibility.

### FM-04: Out of Memory During Batch Inference

**Trigger**: Very large batch size with long texts. ONNX Runtime allocates tensors proportional to `batch_size * max_seq_length * hidden_dim`.
**Behavior**: Allocation failure, likely panic in ONNX Runtime (C++ OOM).
**Recovery**: Reduce batch_size in config. Default batch_size of 32 with 256 tokens and 384-d is ~32 * 256 * 384 * 4 bytes ≈ 12 MB per inference — safe for any modern system.
**Prevention**: Default batch_size of 32 is conservative. Document memory requirements for larger batch sizes.

### FM-05: Tokenizer Produces Empty Token Sequence

**Trigger**: Input text that tokenizes to only special tokens ([CLS], [SEP]) with no content tokens. Extremely unlikely but theoretically possible with certain Unicode sequences.
**Behavior**: Mean pooling over zero content tokens. Attention mask sum could be zero if special tokens are masked. Division by zero produces NaN.
**Recovery**: Normalization layer detects NaN/near-zero norm and returns zero vector or error.
**Prevention**: Validate attention mask sum > 0 before pooling. If zero, return a deterministic zero or uniform vector.

### FM-06: Concurrent Provider Construction

**Trigger**: Multiple threads simultaneously construct `OnnxProvider` with the same model, triggering concurrent downloads to the same cache directory.
**Behavior**: Race condition in file download — both threads write to the same file. Possible corruption.
**Recovery**: `hf-hub` crate may handle concurrent downloads via file locking. If not, one construction succeeds and the other may fail with a file IO error.
**Prevention**: Document that `OnnxProvider::new()` should be called once and the result shared via `Arc`. Construction is not designed for concurrent invocation.

---

## 7. Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 12 scenarios |
| High | 5 (R-03, R-04, R-05, R-06, R-07) | 27 scenarios |
| Medium | 6 (R-08, R-09, R-10, R-11, R-12, R-13, R-14) | 32 scenarios |
| Low | 1 (R-15) | 3 scenarios |
| Integration | 5 (IR-01..05) | 10 scenarios |
| Edge Cases | 5 groups (EC-01..05) | ~30 edge case checks |
| Security | 4 (SR-01..04) | Assessed, mitigations documented |
| **Total** | **23 risk items** | **~114 scenarios** |

### Test Priority Order

1. **R-01** (L2 normalization) — Test FIRST. If embeddings aren't normalized, every DistDot similarity score in nxs-002 is wrong. Foundational correctness.
2. **R-02** (Mean pooling attention mask) — Second priority. If padding tokens leak into pooling, batch embeddings diverge from single embeddings. Detected by R-03 tests but root cause is here.
3. **R-03** (Batch vs single consistency) — The primary signal that the entire pipeline is correct. AC-11 compliance.
4. **R-04** (Model loading) — Must work before any other test can run. Gate test.
5. **R-09** (Empty/degenerate input) — High likelihood edge cases that downstream consumers will hit immediately.
6. **R-06** (Tokenizer truncation) — Silent quality degradation for long content.
7. **R-08** (Title+content concatenation) — Simple but fundamental to every embedding.
8. **R-13** (Catalog dimension) — Validate all 7 models produce 384-d.
9. **R-14** (Batch boundaries) — Off-by-one in chunking.
10. **R-07** (Thread safety) — Low likelihood but high severity.
11. **R-05, R-10, R-11, R-12, R-15** — Lower priority, basic tests sufficient.

---

## 8. Test Strategy Recommendations

### 8.1 Model Fixture Strategy

ONNX model tests require the actual model files (~90 MB). Two tiers:

**Tier 1: Unit tests (no model required)**
- `prepare_text` concatenation logic
- L2 normalization function (with synthetic vectors)
- NaN/infinity validation
- Config construction and validation
- Trait object safety (compile-time)
- Mock provider for downstream testing

**Tier 2: Integration tests (model required)**
- Full embedding pipeline: tokenize → infer → pool → normalize
- Batch vs single consistency
- Semantic similarity validation (AC-08)
- All 7 catalog models
- Thread safety under concurrent load

Tier 2 tests should be gated behind a `#[cfg(feature = "model-tests")]` or `#[ignore]` with a CI job that pre-caches models. This prevents test failures in offline environments.

### 8.2 Mock Provider (AC-19)

A `MockProvider` implementing `EmbeddingProvider` that returns deterministic embeddings without ONNX model loading:

```
MockProvider::new(dimension: 384)
  - embed(text) → deterministic 384-d vector derived from text hash
  - embed_batch(texts) → vec of deterministic embeddings
  - dimension() → 384
  - name() → "mock"
```

This enables downstream consumers (vnc-001, vnc-002) to test embedding-dependent code without model files. Place in `test_helpers` module with `test-support` feature flag, matching nxs-001 and nxs-002 pattern.

### 8.3 Semantic Similarity Test Pairs (AC-08)

Pre-defined test pairs with expected similarity behavior:

**High similarity (> 0.7)**:
- ("authentication tokens", "JWT auth tokens") — same domain, similar terms
- ("database migration", "schema migration for databases") — paraphrase

**Low similarity (< 0.3)**:
- ("authentication tokens", "chocolate cake recipe") — unrelated domains
- ("database migration", "sunset photography tips") — unrelated

These pairs serve as regression tests for embedding quality and model correctness.

### 8.4 Assertion Helpers (AC-19)

Reusable assertion functions for downstream feature tests:

```
assert_valid_embedding(embedding: &[f32], expected_dim: usize)
  - Verify length == expected_dim
  - Verify no NaN
  - Verify no infinity
  - Verify L2 norm within tolerance of 1.0

assert_embeddings_similar(a: &[f32], b: &[f32], min_similarity: f32)
  - Compute cosine similarity
  - Verify >= min_similarity

assert_embeddings_dissimilar(a: &[f32], b: &[f32], max_similarity: f32)
  - Compute cosine similarity
  - Verify <= max_similarity

cosine_similarity(a: &[f32], b: &[f32]) -> f32
  - Utility function for test assertions
```

### 8.5 Test Infrastructure Reuse

Builds on nxs-001 and nxs-002 patterns:
- `test_helpers` module gated behind `#[cfg(any(test, feature = "test-support"))]`
- `test-support` feature flag for downstream crate testing
- MockProvider exported for vnc-001/vnc-002 tests
- Assertion helpers exported for integration testing
- No hardcoded file paths — all cache paths use TempDir in tests

### 8.6 Test Organization

```
src/
├── test_helpers.rs      # MockProvider, assertion helpers, cosine_similarity
tests/
├── normalize.rs         # R-01: L2 normalization unit tests
├── pooling.rs           # R-02: Mean pooling with attention mask
├── consistency.rs       # R-03: Batch vs single embedding
├── model_loading.rs     # R-04, R-05: Model load, download, cache [integration]
├── concatenation.rs     # R-06, R-08: Tokenizer truncation, title+content
├── thread_safety.rs     # R-07: Concurrent embed via Arc<OnnxProvider>
├── edge_cases.rs        # R-09, R-11: Empty input, NaN handling
├── config.rs            # R-10, R-14: Cache paths, batch size boundaries
├── trait_safety.rs      # R-12: Object safety, mock provider
├── catalog.rs           # R-13: All 7 models produce 384-d
├── semantic.rs          # AC-08: Similarity test pairs
└── integration.rs       # IR-01..05: End-to-end with nxs-002
```

---

## 9. Open Questions

1. **Near-zero vector handling**: When the raw ONNX output has near-zero norm (all values < 1e-12), should the normalization function return a zero vector, return an error, or return a uniform unit vector? Returning zero is safe (dot product with any query = 0, never appears as a search result) but semantically meaningless. Returning an error breaks AC-12 (empty string should return embedding). Recommendation: return zero vector for near-zero norms.

2. **Attention mask sum = 0**: If tokenization produces a sequence where the attention mask is all zeros (theoretically possible with certain inputs), mean pooling divides by zero. Should this be pre-checked before pooling? Recommendation: yes, check attention mask sum > 0 and handle gracefully.

3. **Model file integrity**: The SCOPE does not require checksum verification for downloaded models. ruvector also skips this. Should we add SHA256 verification now (hardcoded per catalog model) or defer? Recommendation: defer to a future iteration; the catalog restricts model selection and HTTPS provides transport security.

4. **Concurrent OnnxProvider construction**: If two threads simultaneously call `OnnxProvider::new()` with the same model and the model is not yet cached, both trigger a download. Is `hf-hub` safe for concurrent downloads to the same path? Recommendation: document that construction should be done once, result shared via `Arc`.

5. **Test tier gating**: Should model-dependent tests (Tier 2) use `#[ignore]` or a Cargo feature flag? `#[ignore]` requires `--include-ignored` to run. A feature flag (`model-tests`) is more explicit. Recommendation: use `#[ignore]` with a CI job that runs `cargo test -- --include-ignored`, matching common Rust patterns.
