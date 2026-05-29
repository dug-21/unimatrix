# compose-config -- Pseudocode

## Purpose

Add the `unimatrix-shared` named volume to `docker-compose.yml` and mount it at `/shared` on the unimatrix service. Update comments with backup guidance, volume separation explanation, and `:ro` hardening documentation (ADR-003, AC-11).

## File

`docker-compose.yml`

## Changes

### 1. Add Shared Volume Mount to Service (line 13)

**Change** the volumes section of the `unimatrix` service from:

```yaml
    volumes:
      - unimatrix-data:/data
```

To:

```yaml
    volumes:
      - unimatrix-data:/data
      - unimatrix-shared:/shared
      # Security hardening (optional, after initial model download):
      #   - unimatrix-shared:/shared:ro
```

### 2. Update Service Volume Comment (lines 14-16)

**Remove** the existing comment about config.toml location:

```yaml
      # Config: per-project config.toml lives inside the data volume.
      # Written automatically on first run. Edit via:
      #   docker run --rm -v unimatrix-data:/data busybox vi /data/.unimatrix/<hash>/config.toml
```

**Replace** with volume separation explanation:

```yaml
      # unimatrix-data: databases, vector indexes, config, logs (integrity-critical, back up).
      # unimatrix-shared: ONNX models (re-downloadable, backup optional).
      # Config: per-project config.toml lives inside the data volume.
      # Written automatically on first run. Edit via:
      #   docker run --rm -v unimatrix-data:/data busybox vi /data/.unimatrix/<hash>/config.toml
```

### 3. Add unimatrix-shared Volume Definition (after line 26)

**Change** the top-level `volumes:` section from:

```yaml
volumes:
  unimatrix-data:
    # Named volume for all Unimatrix data (databases, vector indexes,
    # config, model cache, PID files, sockets, logs).
    # Persists across container restarts.
    # Backup = snapshot this volume (includes config):
    #   docker run --rm -v unimatrix-data:/data -v $(pwd):/backup \
    #     busybox tar czf /backup/unimatrix-backup.tar.gz /data
```

To:

```yaml
volumes:
  unimatrix-data:
    # Integrity-critical data: databases, vector indexes, config, logs.
    # Persists across container restarts. Back up frequently.
    # Backup:
    #   docker run --rm -v unimatrix-data:/data -v $(pwd):/backup \
    #     busybox tar czf /backup/unimatrix-backup.tar.gz /data
  unimatrix-shared:
    # Re-downloadable assets: ONNX models (~166 MB).
    # Auto-populated on first start (requires internet).
    # Backup optional -- models re-download from HuggingFace if lost.
    # Air-gap: pre-populate before first start.
    #
    # Security notes:
    #   - After initial download, mount :ro to prevent model tampering.
    #   - Set nli_model_sha256 in config.toml to pin NLI model integrity.
    #   - Embedding model hash enforcement tracked as #651.
```

### 4. Update unimatrix-data Comment

The unimatrix-data volume comment must be updated to remove "model cache" from the description since models now live on unimatrix-shared. Change "Named volume for all Unimatrix data (databases, vector indexes, config, model cache, PID files, sockets, logs)" to "Integrity-critical data: databases, vector indexes, config, logs" as shown above.

## Error Handling

No runtime error handling in compose file changes. If `unimatrix-shared` volume definition is missing or the mount is omitted:
- Docker creates an anonymous volume at `/shared` from the Dockerfile's `VOLUME` directive
- Models download to ephemeral storage, lost on container removal
- Functional but not persistent (R-09)

This failure mode is silent. The compose config validation test (`docker compose config`) catches missing volume definitions.

## Key Test Scenarios

### T-01: Both volumes defined (AC-02, R-09)

```
docker compose config
# Output must show:
#   volumes:
#     unimatrix-data: {}
#     unimatrix-shared: {}
```

### T-02: Shared volume mounted at /shared

```
docker compose config
# Service volumes must include:
#   - unimatrix-data:/data
#   - unimatrix-shared:/shared
```

### T-03: Security guidance present (AC-11)

```
# Grep docker-compose.yml for:
#   - ":ro" hardening comment
#   - "nli_model_sha256" pinning guidance
#   - "#651" embedding hash gap acknowledgment
```

### T-04: Backup guidance present

```
# Grep docker-compose.yml for backup command example.
# unimatrix-data has backup command.
# unimatrix-shared documents backup as optional.
```

### T-05: No stale "model cache" reference in unimatrix-data comment (R-15)

```
# Grep docker-compose.yml for "model cache" in unimatrix-data section.
# Should not appear -- models moved to unimatrix-shared.
```
