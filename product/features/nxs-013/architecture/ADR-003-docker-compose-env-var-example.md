## ADR-003: docker-compose.yml Shows Commented UNIMATRIX_CONFIG Env Var, Not Bind Mount

### Context

OQ-01 asks whether the docker-compose.yml should include a commented example of `UNIMATRIX_CONFIG` for advanced users, or remove all mention of it.

The current docker-compose.yml (lines 14-17) shows a commented bind-mount example (`./config.toml:/etc/unimatrix/config.toml:ro`). This pattern is the thing being eliminated as the default by nxs-013. However, the `UNIMATRIX_CONFIG` env var mechanism remains in `load_config` code for operators who explicitly need config from outside the data volume (Kubernetes ConfigMap, secrets manager injection).

### Decision

Replace the commented bind-mount example with:
1. A comment explaining that per-project config lives inside the data volume at `/data/.unimatrix/{hash}/config.toml` and is written automatically on first run.
2. A commented `environment:` example showing `UNIMATRIX_CONFIG=/path/to/config.toml` for advanced use.
3. A comment noting that backup = snapshot `unimatrix-data` volume (includes config).

Do not include any bind-mount example to `/etc/unimatrix/`.

### Consequences

- **Easier**: New users see the single-volume model as the default. No bind-mount confusion.
- **Easier**: Advanced operators discover the env var mechanism without re-introducing the split-location pattern.
- **Neutral**: Advanced operators who need external config injection can follow the commented example.
