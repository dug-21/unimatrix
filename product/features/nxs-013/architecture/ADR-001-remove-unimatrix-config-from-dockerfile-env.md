## ADR-001: Remove UNIMATRIX_CONFIG from Dockerfile ENV Defaults

### Context

The Dockerfile runtime ENV block sets `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` (line 131). This causes `load_config` Step 0 to always check `/etc/unimatrix/config.toml` first. When no bind mount exists at that path, the check is a harmless no-op (debug log + fallthrough to per-project config). However, the ENV var appears in `docker inspect` output, misleading operators into thinking config must be placed at `/etc/unimatrix/config.toml` via bind mount.

ADR-005 (Unimatrix #4573) established `HOME=/data` so all project data resolves under `/data`. Per-project config already lives at `/data/.unimatrix/{hash}/config.toml` inside the data volume. The Dockerfile ENV creates a false default that contradicts the co-location principle.

The container image has not been released to external users — this is the initial correct design, not a migration from a prior pattern.

### Decision

Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from the Dockerfile runtime ENV block. The remaining ENV vars are `HOME=/data`, `LD_LIBRARY_PATH=/usr/local/lib`, `UNIMATRIX_LOG=info`.

The `UNIMATRIX_CONFIG` env var mechanism remains in `load_config` code for operators who explicitly need config from outside the data volume (e.g., Kubernetes ConfigMap injection).

### Consequences

- **Easier**: `docker inspect` no longer shows a misleading config path. New users follow the per-project config path naturally.
- **Easier**: Backup = snapshot `unimatrix-data` volume (now includes config). No separate config snapshot needed.
- **Easier**: Resolves the ADR-005 "Harder" consequence about config bind mount support.
- **Neutral**: Advanced operators who need external config injection set `UNIMATRIX_CONFIG` explicitly in their compose/orchestration config.
