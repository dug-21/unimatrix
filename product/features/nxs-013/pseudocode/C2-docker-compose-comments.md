# C2: docker-compose.yml Comments

## Purpose

Replace the config bind-mount comment block with documentation explaining per-project config in the data volume, a commented `UNIMATRIX_CONFIG` env var example for advanced use, and backup guidance including config. Per ADR-003: target new users setting up first deployment, not a migration path.

## Target File

`docker-compose.yml`, lines 14-17 (comment block under `volumes:` inside the `unimatrix` service).

## Current State

```yaml
    volumes:
      - unimatrix-data:/data
      # Optional: bind-mount a custom config.toml (read-only).
      # The daemon loads this via UNIMATRIX_CONFIG=/etc/unimatrix/config.toml.
      # Uncomment and set the host path:
      # - ./config.toml:/etc/unimatrix/config.toml:ro
```

## Pseudocode

```
REPLACE lines 14-17 (the four comment lines after "- unimatrix-data:/data") WITH:

  # Config: per-project config.toml lives inside the data volume.
  # Written automatically on first run. Edit via:
  #   docker run --rm -v unimatrix-data:/data busybox vi /data/.unimatrix/<hash>/config.toml
  # For external config (Kubernetes ConfigMap, secrets manager):
  #   environment:
  #     - UNIMATRIX_CONFIG=/path/to/config.toml
```

Additionally, UPDATE the named volume comment block (lines 25-29) to mention config:

```
REPLACE the existing volume comment block WITH:

  unimatrix-data:
    # Named volume for all Unimatrix data (databases, vector indexes,
    # config, model cache, PID files, sockets, logs).
    # Persists across container restarts.
    # Backup = snapshot this volume (includes config):
    #   docker run --rm -v unimatrix-data:/data -v $(pwd):/backup \
    #     busybox tar czf /backup/unimatrix-backup.tar.gz /data
```

Key changes to the volume comment:
- ADD "config, " to the contents list (after "vector indexes,")
- CHANGE backup command comment to "Backup = snapshot this volume (includes config):"

## Constraints

- C-07: Comments target new users -- explain the correct pattern, not migration.
- No reference to `/etc/unimatrix/config.toml` bind mount patterns.
- The commented env var example must be valid YAML when uncommented.
- No structural YAML changes -- only `#`-prefixed comment lines modified.

## Error Handling

Not applicable (static file edit). Risk R-08 (YAML syntax error) mitigated by keeping changes to comment lines only and ensuring the commented `environment:` block uses correct YAML indentation.

## Key Test Scenarios

1. `docker compose -f docker-compose.yml config` validates YAML syntax after edits.
2. No `/etc/unimatrix/` references remain in the file.
3. Backup guidance is present in the volume comment block.
4. Commented `UNIMATRIX_CONFIG` example is present under the service volumes section.
5. When the `environment:` example is uncommented, it forms valid YAML.
