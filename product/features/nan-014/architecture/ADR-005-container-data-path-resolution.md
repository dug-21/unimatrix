## ADR-005: Container Data Path Resolution via --project-dir /data

### Context

The daemon resolves data paths via `ensure_data_directory(override_dir, base_dir)` in `project.rs`. Without an override, it walks up from `cwd` looking for `.git`, hashes the canonical path, and creates `~/.unimatrix/{hash}/`. In a container:

- There is no `.git` directory (source is not in the image).
- `HOME` is `/home/nonroot` (distroless default).
- The data volume is mounted at `/data`.

Without configuration, the daemon would create data at `/home/nonroot/.unimatrix/{hash-of-cwd}/`, which is not in the `/data` volume and would be lost on container restart.

Options considered:

1. **`--project-dir /data`**: Uses the existing CLI flag. `detect_project_root` returns `/data` (canonicalized). Hash is `SHA-256("/data")[..16]`. Data lands at `~/.unimatrix/{hash}/`. Still not in `/data`.

2. **`--project-dir /data` + set `HOME=/data`**: Hash is same. Data lands at `/data/.unimatrix/{hash}/`. Data is in the volume. But `HOME=/data` may confuse other path resolution (e.g., `dirs::cache_dir()`, `dirs::home_dir()`).

3. **New `--data-dir` flag**: A dedicated override that sets the base directory directly (the `base_dir` parameter). Data lands at `{data-dir}/{hash}/`. Clean but requires a new CLI flag.

4. **`--project-dir /data` + `UNIMATRIX_HOME=/data`**: New env var overriding the `~/.unimatrix` base. Data lands at `/data/{hash}/`. Dedicated, no `HOME` collision. But introduces a new config surface.

### Decision

Use `--project-dir /data` in the container `ENTRYPOINT`/`CMD` and `HEALTHCHECK`. This is the simplest option that requires zero new code.

With `--project-dir /data`:
- `detect_project_root` returns `/data` (no `.git` found, returns override dir).
- `compute_project_hash` produces a deterministic hash of `/data`.
- `ensure_data_directory` creates `/home/nonroot/.unimatrix/{hash}/` (default `base_dir`).

The data directory is under `HOME`, which is inside the container filesystem, not the volume. To get data into the volume, the Dockerfile sets:

```dockerfile
ENV HOME=/data
```

With `HOME=/data`:
- `dirs::home_dir()` returns `/data`.
- `base_dir` defaults to `/data/.unimatrix/`.
- Data lands at `/data/.unimatrix/{hash}/`.
- `dirs::cache_dir()` returns `/data/.cache/` (Linux XDG default when HOME is set).
- Model cache resolves to `/data/.cache/unimatrix/models/`.

The container `--project-dir /data` + `HOME=/data` combination puts all data (databases, vector indexes, PID files, sockets, model cache) under the `/data` volume. The project hash is a stable function of the `/data` path, which is constant across container restarts.

The `HEALTHCHECK` uses the same `--project-dir /data`, ensuring socket path consistency (SR-11).

### Consequences

- **Easier**: No new CLI flags or environment variables beyond the existing `--project-dir` and standard `HOME`.
- **Easier**: All data under one volume mount point. Backup = snapshot the `unimatrix-data` volume.
- **Easier**: Deterministic project hash — same on every container start, every restart, every image version.
- **Harder**: The project hash encodes the string `/data`, not the actual project being served. In a container, all projects share the same hash. This is acceptable for the single-project container model. Multi-project (W2-3) will use a different routing mechanism (TenantRouter) that does not depend on filesystem-based project isolation.
- **Harder**: Setting `HOME=/data` means `dirs::config_dir()` resolves to `/data/.config/`, not `/etc/unimatrix/`. The optional config bind mount at `/etc/unimatrix/config.toml` would be invisible to `HOME`-based discovery. Resolution: the Dockerfile sets `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` as an environment variable. The daemon's config loading checks `UNIMATRIX_CONFIG` env var first, then falls back to `dirs::config_dir()`. This is a small code change in the config loading path — add `std::env::var("UNIMATRIX_CONFIG")` as the highest-priority config source. The env var is set in the Dockerfile but the bind mount is optional; if the file doesn't exist at that path, the daemon starts with defaults (existing behavior).
