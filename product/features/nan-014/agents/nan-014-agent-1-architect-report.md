# Agent Report: nan-014-agent-1-architect

## Task
Design architecture for nan-014 (Container Packaging — MIT Image).

## Artifacts Produced

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/nan-014/architecture/ARCHITECTURE.md`

### ADR Files
1. `/workspaces/unimatrix/product/features/nan-014/architecture/ADR-001-foreground-mode-implementation.md` (Unimatrix #4569)
2. `/workspaces/unimatrix/product/features/nan-014/architecture/ADR-002-ort-supply-chain-verification.md` (Unimatrix #4570)
3. `/workspaces/unimatrix/product/features/nan-014/architecture/ADR-003-health-check-uds-connect.md` (Unimatrix #4571)
4. `/workspaces/unimatrix/product/features/nan-014/architecture/ADR-004-ci-container-job-independence.md` (Unimatrix #4572)
5. `/workspaces/unimatrix/product/features/nan-014/architecture/ADR-005-container-data-path-resolution.md` (Unimatrix #4573)
6. `/workspaces/unimatrix/product/features/nan-014/architecture/ADR-006-cargo-chef-version-pinning.md` (Unimatrix #4574)

## Key Decisions

1. **Foreground mode** calls `tokio_main_daemon` directly — no refactoring needed because `tokio_main_daemon` already IS the shared daemon core. setsid happens before it in main(), not inside it. Zero blast radius to existing --daemon path (SR-06 mitigated).

2. **ORT SHA-256 verification** via ARG-pinned hashes in Dockerfile, TARGETARCH-conditional selection, sha256sum -c gate before extraction (SR-01 mitigated).

3. **Health check** via sync UDS socket connect to mcp_socket_path. Both daemon and health resolve paths via same ensure_data_directory with same --project-dir flag (SR-11 mitigated).

4. **CI jobs independent** — container build/manifest jobs form separate branch from binary/npm jobs in release.yml dependency graph (SR-10 mitigated).

5. **Data path resolution** via `--project-dir /data` + `HOME=/data` ENV — all data under /data volume, deterministic project hash.

6. **cargo-chef pinned** with version + --locked for build reproducibility (SR-02 mitigated).

## Scope Risks Addressed

| Risk | Severity | Mitigation |
|------|----------|------------|
| SR-06: Foreground mode blast radius | High | Direct tokio_main_daemon call, no refactoring of daemon path |
| SR-01: ORT supply chain | High | SHA-256 + curl -fsSL |
| SR-02: cargo-chef supply chain | High | Version pin + --locked |
| SR-10: CI coupling | Med | Independent job branches |
| SR-11: Socket path consistency | Med | Same --project-dir flag for both daemon and health |
| SR-04: Distroless tag mutability | Med | Recommend digest pin at implementation time |

## Open Questions for Implementation

1. ORT SHA-256 hashes must be captured at implementation time
2. cargo-chef version must be captured at implementation time
3. Distroless digest should be captured for reproducibility
4. Model baking path in builder stage needs verification of exact output directory

## Status
Complete.
