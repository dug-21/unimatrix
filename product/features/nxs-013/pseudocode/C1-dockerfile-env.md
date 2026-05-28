# C1: Dockerfile ENV Block

## Purpose

Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from the Dockerfile runtime ENV block so the container no longer advertises a misleading default config path. After removal, the daemon loads per-project config from the data volume naturally via `HOME=/data` (ADR-005, Unimatrix #4573).

## Target File

`Dockerfile`, lines 128-131.

## Current State

```dockerfile
# Environment (ADR-005).
ENV HOME=/data \
    LD_LIBRARY_PATH=/usr/local/lib \
    UNIMATRIX_LOG=info \
    UNIMATRIX_CONFIG=/etc/unimatrix/config.toml
```

## Pseudocode

```
REMOVE the fourth line of the ENV block: "    UNIMATRIX_CONFIG=/etc/unimatrix/config.toml"
REMOVE the trailing backslash from the UNIMATRIX_LOG line (it becomes the last entry)

Result:
  ENV HOME=/data \
      LD_LIBRARY_PATH=/usr/local/lib \
      UNIMATRIX_LOG=info
```

The line continuation backslash on `UNIMATRIX_LOG=info \` must be removed since it is now the final ENV entry.

## Constraints

- C-03: `UNIMATRIX_CONFIG` env var stays in Rust code -- only the Dockerfile default is removed.
- `HOME=/data` MUST remain -- path resolution depends on it (ADR-005).
- `LD_LIBRARY_PATH` and `UNIMATRIX_LOG` MUST remain.

## Error Handling

Not applicable (static file edit).

## Key Test Scenarios

1. Docker build succeeds with the modified Dockerfile.
2. `docker inspect --format '{{.Config.Env}}' <image>` confirms `UNIMATRIX_CONFIG` is absent.
3. `docker inspect` confirms `HOME=/data` is present.
4. Container starts with empty data volume; startup logs show "primary config" messages (not "env override").
