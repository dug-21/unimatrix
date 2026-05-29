# Test Plan: compose-config

**File**: `docker-compose.yml`

## Volume Definitions (R-09, Med)

### Test 1: Both named volumes defined

```
Name: verify_both_volumes_defined
Method: Parse docker-compose.yml or run `docker compose config`.
Assert:
  1. Top-level `volumes:` section defines `unimatrix-data`.
  2. Top-level `volumes:` section defines `unimatrix-shared`.
  3. Both are named volumes (not bind mounts).
Risk: R-09
AC: AC-02
```

### Test 2: Volume mount points correct

```
Name: verify_volume_mount_points
Method: Parse service definition in docker-compose.yml.
Assert:
  1. Service `unimatrix` mounts `unimatrix-data` at `/data`.
  2. Service `unimatrix` mounts `unimatrix-shared` at `/shared`.
  3. No other volumes mount at `/shared` (prevents conflicts).
Risk: R-09
AC: AC-02
```

### Test 3: docker compose config validates

```
Name: verify_compose_config_parses
Method: Run `docker compose config` against the modified file.
Assert:
  1. Command exits 0 (valid YAML, valid compose schema).
  2. Output shows both volume definitions.
  3. Output shows both mount points in the service.
Risk: R-09
Note: Requires docker compose CLI -- Stage 3c only.
```

## Comment Quality

### Test 4: Backup guidance present

```
Name: verify_backup_comments
Method: Grep docker-compose.yml for documentation comments.
Assert:
  1. Comment explains that `unimatrix-data` requires backup (integrity-critical).
  2. Comment explains that `unimatrix-shared` backup is optional (re-downloadable).
  3. Backup command example is present or referenced.
Risk: Operational correctness -- operators must know which volume to back up.
```

### Test 5: Security hardening guidance present

```
Name: verify_hardening_comments
Method: Grep docker-compose.yml.
Assert:
  1. Comment mentions `:ro` mount as optional hardening after initial population.
  2. Comment mentions `nli_model_sha256` for hash pinning on shared volumes.
  3. Comment references #651 for embedding model hash gap.
Risk: R-03, AC-11
```

## Multi-Container Sharing (R-13, Med)

### Test 6: Compose structure supports multi-container

```
Name: verify_multi_container_compatible
Method: Review compose file structure.
Assert:
  1. `unimatrix-shared` is a top-level named volume (not service-scoped), so multiple services can mount it.
  2. No `driver_opts` or `labels` prevent sharing.
Risk: R-13 (AC-06 verified at container runtime in Stage 3c)
```

## Compose Validation Summary

| Check | Method | Blocks Gate? |
|-------|--------|-------------|
| unimatrix-shared volume defined | grep/parse | Yes |
| Mounted at /shared | grep/parse | Yes |
| docker compose config exits 0 | shell | Yes |
| Backup guidance comment | grep | No |
| :ro hardening comment | grep | Yes (AC-11) |
| #651 gap referenced | grep | Yes (AC-11) |
| nli_model_sha256 referenced | grep | Yes (AC-11) |
