# nan-014-researcher Report

## Deliverable
- `/workspaces/unimatrix/product/features/nan-014/SCOPE.md`

## Key Findings

1. **ASS-043 is exhaustive** — all 8 research questions answered with directional confidence. No further research spikes needed before design.

2. **W2-2/W2-3 are NOT hard blockers** — the container skeleton (Dockerfile, compose, CI) is independent of HTTPS transport and auth middleware. Delivering the container first enables iterative W2-2 testing in container environments. The compose file needs only additive changes (port mapping, TLS cert mount) when W2-2 ships.

3. **Foreground mode required** — containers expect PID 1 to be the main process. `serve --daemon` forks and the container would exit immediately. A `--foreground` flag on the `serve` subcommand is a small code change: run `tokio_main_daemon` without the fork/setsid preamble.

4. **Health subcommand required** — no `unimatrix health` CLI subcommand exists. HEALTHCHECK in the Dockerfile needs it. UDS-based ping (connect to daemon socket, verify response) is the right approach since no HTTP endpoint exists yet.

5. **Volume ownership challenge** — distroless `nonroot` (UID 65534) must own `/data`, but Docker named volumes are created as root. Distroless has no shell for `chown`. Solution: create `/data` with correct ownership in the builder stage and COPY it, or use `--chown` in COPY directives.

6. **Model baking viable** — embedding (87 MB) + NLI quantized (79 MB) = 166 MB. Small enough to bake into image layers for true zero-config startup. GGUF models (1+ GB) explicitly excluded.

7. **infra-001 Dockerfile is stale reference** — uses old binary name (`unimatrix-server`), no cargo-chef, no SHA-256 verification, Python test runtime (not production). Useful as ORT installation pattern reference only.

8. **GHCR permissions needed** — existing release.yml has `contents: write` + `id-token: write` but not `packages: write`. Container push to GHCR requires this addition.

## Proposed Scope Boundaries

**In scope**: Dockerfile (3-stage cargo-chef), docker-compose.yml, .dockerignore, `serve --foreground` flag, `health` subcommand, container CI jobs in release.yml, ORT SHA-256 verification, model baking, multi-arch manifest.

**Out of scope**: Enterprise image, HTTPS/TLS, bearer token, GGUF, Kubernetes manifests, config.toml enforcement, release.yml ORT checksum backport.

## Open Questions for Human

1. **OQ-01**: `--foreground` as new flag vs. reusing existing serve paths?
2. **OQ-04**: Volume ownership strategy with distroless nonroot?
3. **OQ-05**: Config mount path: `/etc/unimatrix/config.toml` vs `/data/config.toml`?

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 20 entries returned; #4554 (W2-1 feature), #4274 (ORT SHA-256 gap), #1199 (binary rename ADR) were directly relevant
- Stored: nothing novel to store -- all findings are feature-specific scope details that belong in SCOPE.md, not generalizable patterns
