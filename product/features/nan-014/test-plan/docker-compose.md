# Test Plan: docker-compose

## Component

New file: `docker-compose.yml` (repo root). Single-service deployment with named volume.

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-01 (High) | Compose starts foreground mode correctly | AC-03 |
| R-05 (High) | Compose stop triggers graceful shutdown | `docker compose stop` test |

## Shell Tests

### AC-03: docker compose up starts service

**Arrange**: Build image (or pull from GHCR).

**Act**: `docker compose up -d`

**Assert**:
- `docker compose ps` shows container running
- Named volume `unimatrix-data` exists: `docker volume inspect unimatrix-data`
- Volume is mounted at `/data`: `docker inspect <container> --format '{{json .Mounts}}'` shows `/data` target

**Teardown**: `docker compose down -v`

### AC-12: Debug override pattern documented

**Act**: Read `docker-compose.yml` comments.

**Assert**:
- Comments describe `docker-compose.override.yml` pattern
- Example shows swapping image to `debian:12-slim` for shell access
- Example is syntactically valid YAML (can be copy-pasted into an override file)

### Compose stop triggers graceful shutdown

**Arrange**: `docker compose up -d`, wait for health to be `healthy`.

**Act**: `docker compose stop`

**Assert**:
- `docker compose logs` shows graceful shutdown messages
- No SIGKILL in logs
- `docker compose up -d` (restart) succeeds -- volume data intact

## Validation Checklist (Code Review)

- [ ] Service name: `unimatrix`
- [ ] Image: `ghcr.io/dug-21/unimatrix:latest` (or local build reference)
- [ ] Volume: `unimatrix-data` mounted at `/data`
- [ ] Restart policy: `unless-stopped`
- [ ] No port mappings (no EXPOSE until W2-2)
- [ ] Optional config.toml bind mount documented in comments
- [ ] Debug override pattern documented in comments
- [ ] No `UNIMATRIX_CONFIG` override needed in compose (already set in Dockerfile ENV)

## Integration Tests

No infra-001 tests. Compose is a deployment orchestration concern, not an MCP protocol concern.
