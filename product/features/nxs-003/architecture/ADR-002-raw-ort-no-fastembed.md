## ADR-002: Raw ort + tokenizers (No fastembed Wrapper)

### Context

Two approaches exist for ONNX-based embedding generation in Rust:

1. **fastembed-rs**: A high-level wrapper around ort + tokenizers. Provides `TextEmbedding` with `embed()` and `passage_embed()`. Manages model downloads, tokenization, pooling, and normalization internally.

2. **Raw ort + tokenizers + hf-hub**: Build the pipeline from individual crates. Handle tokenization, tensor creation, inference, pooling, and normalization manually.

fastembed concerns:
- **Exact-pinned ort dependency**: fastembed pins `ort = "=2.0.0-rc.11"` (exact version). Any workspace or sibling crate wanting a different ort version creates a conflict.
- **Edition 2024 risk**: fastembed may not yet be tested with edition 2024. Its transitive dependencies (especially native build scripts) may break.
- **Limited control**: fastembed handles pooling and normalization internally. If we need custom pooling strategies or debugging access to raw model output, fastembed is a black box.
- **Model catalog coupling**: fastembed's model list is fixed at its release version. Adding custom models or new HuggingFace models requires a fastembed release.

Raw approach validation:
- ruvector (85+ crate Rust vector database) uses raw `ort` + `tokenizers` in production.
- Their implementation is ~800 lines for tokenization, pooling, normalization, and model management.
- The raw approach gives full control over every step of the pipeline.

### Decision

Use raw `ort` (2.0.0-rc.11) + `tokenizers` + `hf-hub`. No fastembed dependency.

The pipeline components are:
- `tokenizers::Tokenizer` for text tokenization (word-piece, truncation, padding).
- `ort::Session` for ONNX model inference.
- Custom `mean_pool()` function for attention-masked mean pooling.
- Custom `l2_normalize()` for unit-length normalization.
- `hf-hub` for model downloading from HuggingFace Hub.

### Consequences

**Easier:**
- Full control over tokenization, pooling, and normalization. Can debug and tune each step.
- No exact-pinned ort version constraint. The workspace can use any compatible ort version.
- No edition 2024 risk from fastembed's dependency tree.
- Model catalog is our own -- can add/remove models without waiting for a fastembed release.
- Simpler dependency tree with fewer transitive dependencies.

**Harder:**
- Must implement ~300-400 lines of pipeline code (tokenization → tensors → inference → pooling → normalization) that fastembed provides for free.
- Must handle tokenizer configuration (truncation, padding, special tokens) manually.
- Must maintain our own model catalog with correct HuggingFace IDs and ONNX filenames.
- If a new model needs a different pooling strategy (CLS pooling, max pooling), we implement it ourselves. However, all 7 catalog models use mean pooling, so this is theoretical.
