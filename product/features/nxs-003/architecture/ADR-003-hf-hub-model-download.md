## ADR-003: hf-hub Crate for Model Download

### Context

ONNX models (~90MB) and tokenizer files (~700KB) must be downloaded from HuggingFace Hub on first use. Three approaches were considered:

1. **hf-hub crate**: The official Rust client for HuggingFace Hub. Handles repository structure, file resolution, revision tracking, and caching.

2. **Raw HTTP via reqwest**: Download files directly via HTTPS URLs. ruvector uses this approach -- constructing URLs manually and downloading with reqwest + indicatif progress bars.

3. **Embed models in binary**: Include the ONNX model as a `include_bytes!` resource. Bloats the binary by ~90MB per model. Not viable for a library crate with 7 model options.

### Decision

Use `hf-hub` crate (v0.4) for model downloading.

Download flow:
1. Create `hf_hub::api::sync::Api` (anonymous, no auth token for public models).
2. Get repo handle via `api.model(model_id)` (e.g., `"sentence-transformers/all-MiniLM-L6-v2"`).
3. Download files via `repo.get("model.onnx")` and `repo.get("tokenizer.json")`.
4. hf-hub handles caching to its own directory structure.
5. Copy or symlink files to our custom cache directory (`~/.cache/unimatrix/models/`).

Alternative considered: Use hf-hub's built-in cache directly. Rejected because hf-hub's cache layout (`blobs/`, `refs/`, `snapshots/`) is complex and couples us to its internal structure. See ADR-004 for the custom cache decision.

### Consequences

**Easier:**
- Handles HuggingFace's repository layout (model files may be at root or in `onnx/` subfolder).
- Supports revision tracking and model versioning.
- Anonymous access works for all public sentence-transformer models.
- Well-maintained crate with active development.
- Handles authentication if needed in the future (private models, rate limits).

**Harder:**
- Adds `hf-hub` as a dependency (~moderate dependency tree including reqwest).
- hf-hub has its own internal cache that may duplicate storage with our custom cache directory.
- Network errors surface as hf-hub error types that must be mapped to `EmbedError::Download`.
