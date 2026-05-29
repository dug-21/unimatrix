# nan-015-researcher Report

## Task
Research problem space for moving ONNX models from Docker image bake-in to a shared named volume.

## Findings

### Model Loading Architecture
All model path resolution flows through `EmbedConfig::resolve_cache_dir()` which uses `dirs::cache_dir()` (resolves to `$HOME/.cache` on Linux). In the container, `HOME=/data` makes this `/data/.cache/unimatrix/models/`. Three call sites: embed startup, NLI startup, model-download CLI -- all use `EmbedConfig::default()`.

### Current Dockerfile Impact
Models are baked via `model-download` in the builder stage, then COPYed to the runtime stage. This adds ~166 MB to the image and re-downloads on every rebuild. The COPY step cannot be cached independently of source changes.

### Auto-Download Already Works
`ensure_model()` and `ensure_nli_model()` in `download.rs` check for file existence + non-zero size before downloading. Moving models to a volume requires zero changes to the download logic -- it works automatically with any cache path.

### Path Redirection Options
Three approaches identified. Recommended: explicit environment variable (`UNIMATRIX_MODEL_CACHE`) checked by `resolve_cache_dir()` before `dirs::cache_dir()`. This is container-specific (set in Dockerfile), doesn't affect non-container usage, and works for all three call sites.

### Multi-Container Safety
ONNX sessions open model files read-only. Concurrent reads are safe. Concurrent first-run downloads could produce redundant downloads but not corruption.

## SCOPE.md
Written to `/workspaces/unimatrix/product/features/nan-015/SCOPE.md`.
- 6 goals, 6 non-goals, 10 acceptance criteria (AC-01 through AC-10), 7 constraints, 4 open questions.

## Open Questions for Human
1. **OQ-01**: Mount point `/shared` vs `/shared/models` -- determines future volume reuse for GGUF.
2. **OQ-02**: Should `model-download` gain `--cache-dir` flag?
3. **OQ-03**: WAVE2-ROADMAP ASS-060 line annotation correction.
4. **OQ-04**: Shared volume read-only policy after population vs read-write for auto-download.

## Risks
- First-run startup delay: without baked models, the first container start requires ~1-2 min for model downloads before the server is fully operational. This is acceptable given the lazy-loading architecture (server starts immediately, models load in background).
- Concurrent first-download race: rare (only on first-ever start of multiple containers simultaneously), benign (file overwrite with identical content), not worth adding file locking for.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 16 entries returned. Key entries: #4579 (container build pattern), #70 (cache dir ADR), #4570 (ORT SHA-256), #2492 (NLI integration), #4636 (volume description corrections). All highly relevant.
- Stored: nothing novel to store -- findings are feature-specific and belong in SCOPE.md, not as generalized patterns.
