## ADR-003: Shared Volume Default Read-Write with Optional Read-Only Hardening

### Context

SR-02 (High severity) identifies that a shared writable volume widens the supply-chain attack surface compared to baked-in models. A compromised container or volume mount can replace ONNX model files between restarts. Lesson #4642 establishes that hash verification must precede loading — the verify-then-load ordering becomes more critical with writable shared storage.

Two usage modes conflict:
- **Zero-config startup** (Goal 4): First run with an empty volume must auto-download models. This requires `:rw`.
- **Security hardening** (AC-11): Operators who pre-populate the volume should be able to mount it `:ro` to prevent tampering.

The NLI model has SHA-256 verification at load time (NliServiceHandle, ADR-003 crt-023). The embedding model does not have hash verification — that is tracked separately as #651 and explicitly out of scope for nan-015 (SCOPE.md Non-Goals).

### Decision

Default to `:rw` in docker-compose.yml. Document `:ro` as an optional hardening step.

**docker-compose.yml mount**:
```yaml
volumes:
  - unimatrix-shared:/shared
  # Optional hardening after initial model population:
  #   - unimatrix-shared:/shared:ro
```

**Security guidance in docker-compose.yml comments** (AC-11):
- After first run populates models, operators MAY switch to `:ro` to prevent model tampering.
- Operators using shared volumes SHOULD set `nli_model_sha256` in config.toml to pin NLI model integrity.
- Embedding model hash pinning (`embedding_model_sha256`) is tracked as #651 — not yet enforced.
- Air-gap operators who pre-populate the volume can mount `:ro` from the start.

**Verify-then-load ordering** (lesson #4642): The existing NLI SHA-256 verification in `NliServiceHandle::spawn_load_task()` runs before ONNX session construction. This ordering is preserved through the path change. The `cache_dir` field in `NliConfig` is populated from `resolve_cache_dir()` — changing the resolved path does not affect the verification sequence.

### Consequences

**Easier:**
- Zero-config startup preserved. New users run `docker compose up` and models download automatically.
- Air-gap operators can pre-populate the volume and mount `:ro` — no runtime behavior change since `ensure_model()` finds existing files and skips download.
- `:ro` mount with missing models produces a clear error: `fs::create_dir_all` fails with `io::Error::PermissionDenied`, propagated through `EmbedError::Io`, triggering the retry/fallback state machine.

**Harder:**
- Default `:rw` means a compromised container can overwrite model files. The NLI hash check catches tampering for NLI; the embedding model has no hash check until #651 ships.
- Operators must actively opt into `:ro` — the secure posture is not the default. This is a deliberate tradeoff: zero-config startup (Goal 4) is the primary user experience requirement.
- Documentation must clearly explain the gap: NLI has hash verification, embedding does not (yet). AC-11 guidance must not imply both models are equally protected.
