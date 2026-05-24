# docker-compose: Deployment Configuration

## Purpose

Single-command deployment with named volume, optional config override, and debug documentation. Per FR-2.

## New File

**File**: `docker-compose.yml` (repo root)

### Pseudocode (YAML structure)

```yaml
# Unimatrix — single-service deployment.
# Start: docker compose up -d
# Logs:  docker compose logs -f
# Stop:  docker compose down

services:
  unimatrix:
    image: ghcr.io/dug-21/unimatrix:latest
    # To build locally instead of pulling:
    #   build: .
    restart: unless-stopped
    volumes:
      - unimatrix-data:/data
      # Optional: bind-mount a custom config.toml (read-only).
      # The daemon loads this via UNIMATRIX_CONFIG=/etc/unimatrix/config.toml.
      # Uncomment and set the host path:
      # - ./config.toml:/etc/unimatrix/config.toml:ro
    # No port mappings — UDS mode only until W2-2 (HTTPS transport).
    # When W2-2 lands, add:
    #   ports:
    #     - "8443:8443"

volumes:
  unimatrix-data:
    # Named volume for all Unimatrix data (databases, vector indexes,
    # model cache, PID files, sockets, logs).
    # Persists across container restarts.
    # Back up: docker run --rm -v unimatrix-data:/data -v $(pwd):/backup \
    #   busybox tar czf /backup/unimatrix-backup.tar.gz /data

# ──────────────────────────────────────────────────────────────────────
# Debug override (shell access for troubleshooting)
# ──────────────────────────────────────────────────────────────────────
# The runtime image is distroless (no shell). For debugging, create
# a docker-compose.override.yml alongside this file:
#
#   services:
#     unimatrix:
#       image: debian:12-slim
#       entrypoint: ["/bin/bash", "-c", "sleep infinity"]
#       volumes:
#         - unimatrix-data:/data
#
# Then: docker compose up -d && docker exec -it unimatrix-unimatrix-1 /bin/bash
# This gives shell access to the /data volume for inspection.
# Remove the override file to return to normal operation.
```

### Design Notes

- **Image reference**: `ghcr.io/dug-21/unimatrix:latest` by default. Users can pin to a specific version tag (`ghcr.io/dug-21/unimatrix:v0.8.0`).
- **Build option**: Commented `build: .` allows local builds.
- **Restart policy**: `unless-stopped` — restarts on crash but not after explicit `docker compose down`.
- **No port mappings**: Per constraint C-10, no HTTP listener until W2-2.
- **Config bind mount**: Commented out. When uncommented, the file at the host path is mounted read-only at `/etc/unimatrix/config.toml`. The daemon discovers it via `UNIMATRIX_CONFIG` env var (set in Dockerfile).
- **Debug override**: Documented in comments, not as a separate file. The override swaps to `debian:12-slim` with a sleep entrypoint for shell access.

## Error Handling

Not applicable (static configuration file). Runtime errors are handled by the daemon inside the container.

## Key Test Scenarios

1. **docker compose up**: `docker compose up -d` starts the container. `docker compose ps` shows it running.

2. **Named volume persistence**: `docker compose down` then `docker compose up -d`. Data in `/data` persists across restarts.

3. **Config bind mount**: Uncomment the config volume line, provide a `config.toml`. Restart. Verify daemon logs show config loaded from `/etc/unimatrix/config.toml`.

4. **Debug override**: Create the override file, run `docker compose up -d`. Verify `docker exec -it <name> /bin/bash` provides shell access to `/data`.

5. **Restart policy**: Kill the container process (`docker kill`). Verify Docker restarts it automatically.
