# Scope Risk Assessment: nan-015

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Cache path precedence chain has three layers (UNIMATRIX_MODEL_CACHE env var > EmbedConfig.cache_dir > dirs::cache_dir). Incorrect precedence causes models to land on the wrong volume, breaking backup separation silently. | High | Med | Architect must define and test the full resolution chain. ADR-004 (#70) and ADR-005 (#4573) document the existing chain -- the new env var must slot in unambiguously. |
| SR-02 | Shared writable volume widens supply-chain attack surface vs baked-in models. A compromised container or volume mount can replace ONNX model files between restarts. Lesson #4642: hash verification must precede loading. | High | Low | Architect should ensure embedding model SHA-256 verification exists at load time (currently only NLI has it). AC-11 documents `:ro` hardening but the default is `:rw`. |
| SR-03 | First-run cold start adds network dependency and unbounded startup latency (~166 MB download). If HuggingFace is unreachable or rate-limited, the daemon enters degraded state indefinitely. | Med | Med | Architect should define a maximum retry/timeout policy for first-run download and document expected startup time with empty volume. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope excludes embedding model SHA-256 verification (#651) but AC-11 requires documenting hash pinning for both models. Operators following AC-11 guidance for the embedding model will find it has no enforcement mechanism. | Med | High | Spec writer should clarify that AC-11 documentation acknowledges #651 as the enforcement gap, so operators are not misled. |
| SR-05 | CI/CD constraint (Constraint 7) -- smoke tests and health checks may assume models are present in the image. If any downstream job (not just the build) depends on baked-in models, it will fail silently after this change. | Med | Med | Architect should audit release.yml and all CI jobs that run the container image for model-presence assumptions. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Three call sites (daemon embed, NLI, model-download CLI) must all resolve to `/shared/models/` in-container. If any call site bypasses the env var (e.g., by constructing EmbedConfig with explicit cache_dir), models split across volumes. | High | Med | Architect should ensure all three paths go through a single resolution function, not parallel logic. |
| SR-07 | Concurrent first-run download from multiple containers mounting the same empty volume. SCOPE acknowledges this is "safe but wasteful." However, partial writes from a crashed download could leave a non-zero-size corrupt file that passes the existence check but fails ONNX session load. | Med | Low | Spec writer should define behavior when ONNX session load fails on a present-but-corrupt file (re-download vs error). |
| SR-08 | Non-container usage must be unaffected (Constraint 5). If the env var `UNIMATRIX_MODEL_CACHE` is accidentally set in a developer's shell, it silently redirects model storage. | Low | Low | Document the env var as container-internal only. Consider a container-specific prefix or guard. |

## Assumptions

- **SCOPE line 80-85**: Auto-download handles "download if not present" via file existence + non-zero size checks. Assumes non-zero size implies a valid model file. This assumption fails if a download is interrupted mid-write (partial file > 0 bytes). See SR-07.
- **SCOPE line 145-146**: "ONNX sessions open model files read-only after initial load." Assumes no model hot-reload or file re-read. If future features (GGUF, model updates) introduce hot-reload, the read-only concurrency assumption breaks.
- **SCOPE line 109**: `/shared` directory creation with ownership 65532:65532. Assumes Docker named volumes inherit directory ownership from the image layer. This is true for the local storage driver but may differ with NFS or cloud volume drivers.

## Design Recommendations

- **SR-01, SR-06**: Implement a single `resolve_model_cache_dir()` function with documented precedence: env var > config field > dirs fallback. All three call sites must use it.
- **SR-02**: Even though #651 is out of scope, the architect should ensure the verify-then-load ordering (lesson #4642) is preserved through the path change. The shared volume makes this ordering more critical, not less.
- **SR-05**: Add a CI smoke test that starts the container with an empty shared volume and verifies health check passes after model download completes.
- **SR-07**: Consider atomic write (download to temp file, rename) to prevent partial-file corruption on the shared volume.
