## ADR-004: Custom Cache Directory (~/.cache/unimatrix/models/)

### Context

Downloaded ONNX models and tokenizer files need a persistent cache location. Three strategies were considered:

1. **HuggingFace default cache**: Let hf-hub cache to its default location (`~/.cache/huggingface/hub/`). Files are stored in a complex `blobs/refs/snapshots` structure.

2. **Custom directory via `dirs` crate**: Use `~/.cache/unimatrix/models/` with a flat per-model subdirectory layout (e.g., `sentence-transformers_all-MiniLM-L6-v2/model.onnx`).

3. **Project-local cache**: Store models in the project's data directory (e.g., `~/.unimatrix/{project_hash}/models/`). Models would be per-project.

ruvector uses approach #2: `~/.cache/ruvector/onnx-models/` with sanitized model IDs as subdirectory names. This is validated in production.

### Decision

Use `~/.cache/unimatrix/models/` as the default cache directory, resolved cross-platform via the `dirs` crate's `cache_dir()`.

**Platform resolution:**
- Linux: `~/.cache/unimatrix/models/`
- macOS: `~/Library/Caches/unimatrix/models/`
- Windows: `{FOLDERID_LocalAppData}\unimatrix\models\`

**Cache layout:**
```
~/.cache/unimatrix/models/
+-- sentence-transformers_all-MiniLM-L6-v2/
|   +-- model.onnx        (~90 MB)
|   +-- tokenizer.json    (~700 KB)
+-- BAAI_bge-small-en-v1.5/
|   +-- model.onnx
|   +-- tokenizer.json
```

Model ID sanitization: Replace `/` with `_` in the HuggingFace model ID to create the subdirectory name.

**Override:** `EmbedConfig.cache_dir` allows callers to specify a custom location (useful for testing and CI).

### Consequences

**Easier:**
- Predictable, simple layout. Easy to inspect, debug, and manage manually.
- Cross-platform via `dirs` crate (no platform-specific code).
- Shared across all Unimatrix projects on the same machine (models are project-agnostic).
- Easy to clear cache: `rm -rf ~/.cache/unimatrix/models/`.
- Configurable via `EmbedConfig.cache_dir` for testing.

**Harder:**
- May duplicate storage with hf-hub's internal cache (if hf-hub caches downloaded files before we copy them to our directory). Mitigation: hf-hub's cache can be cleaned independently.
- If `dirs::cache_dir()` returns `None` (rare edge case on unusual platforms), we must fall back to a reasonable default (current directory or temp directory).
- Model files are shared across projects. If two projects use different model versions (unlikely with our fixed catalog), they share the same files. This is by design -- all 7 catalog models are fixed versions.
