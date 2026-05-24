# ci-container-jobs: CI Container Build Jobs in release.yml

## Purpose

Add 3 new jobs to `.github/workflows/release.yml` to build and publish dual-arch container images to GHCR on `v*` tag push. Per ADR-004, container jobs are independent of binary/npm jobs.

## Modified File

**File**: `.github/workflows/release.yml`

### 1. Workflow-Level Permission Addition

Add `packages: write` to the existing permissions block:

```yaml
permissions:
  contents: write
  id-token: write
  packages: write    # NEW: GHCR push for container images
```

### 2. New Job: build-container-x64

```yaml
build-container-x64:
  runs-on: ubuntu-22.04
  # No 'needs' — independent of binary/npm jobs (ADR-004).
  steps:
    - uses: actions/checkout@v4

    - name: Set up Docker Buildx
      uses: docker/setup-buildx-action@v3

    - name: Log in to GHCR
      uses: docker/login-action@v3
      with:
        registry: ghcr.io
        username: ${{ github.actor }}
        password: ${{ secrets.GITHUB_TOKEN }}

    - name: Extract metadata
      id: meta
      uses: docker/metadata-action@v5
      with:
        images: ghcr.io/${{ github.repository_owner }}/unimatrix
        tags: |
          type=semver,pattern=v{{version}}-amd64
          type=raw,value=latest-amd64

    - name: Build and push (amd64)
      uses: docker/build-push-action@v6
      with:
        context: .
        push: true
        platforms: linux/amd64
        tags: ${{ steps.meta.outputs.tags }}
        labels: ${{ steps.meta.outputs.labels }}
        cache-from: type=gha,scope=container-amd64
        cache-to: type=gha,mode=max,scope=container-amd64
```

### 3. New Job: build-container-arm64

```yaml
build-container-arm64:
  runs-on: ubuntu-22.04-arm
  # No 'needs' — independent (ADR-004).
  # Native ARM64 runner — no QEMU (constraint C-3).
  steps:
    - uses: actions/checkout@v4

    - name: Set up Docker Buildx
      uses: docker/setup-buildx-action@v3

    - name: Log in to GHCR
      uses: docker/login-action@v3
      with:
        registry: ghcr.io
        username: ${{ github.actor }}
        password: ${{ secrets.GITHUB_TOKEN }}

    - name: Extract metadata
      id: meta
      uses: docker/metadata-action@v5
      with:
        images: ghcr.io/${{ github.repository_owner }}/unimatrix
        tags: |
          type=semver,pattern=v{{version}}-arm64
          type=raw,value=latest-arm64

    - name: Build and push (arm64)
      uses: docker/build-push-action@v6
      with:
        context: .
        push: true
        platforms: linux/arm64
        tags: ${{ steps.meta.outputs.tags }}
        labels: ${{ steps.meta.outputs.labels }}
        cache-from: type=gha,scope=container-arm64
        cache-to: type=gha,mode=max,scope=container-arm64
```

### 4. New Job: create-container-manifest

```yaml
create-container-manifest:
  needs: [build-container-x64, build-container-arm64]
  runs-on: ubuntu-latest
  steps:
    - name: Log in to GHCR
      uses: docker/login-action@v3
      with:
        registry: ghcr.io
        username: ${{ github.actor }}
        password: ${{ secrets.GITHUB_TOKEN }}

    - name: Extract version from tag
      id: version
      run: echo "version=${GITHUB_REF_NAME}" >> "$GITHUB_OUTPUT"

    - name: Create and push multi-arch manifest
      run: |
        VERSION=${{ steps.version.outputs.version }}
        IMAGE=ghcr.io/${{ github.repository_owner }}/unimatrix

        # Create versioned manifest from per-arch images.
        docker manifest create ${IMAGE}:${VERSION} \
          ${IMAGE}:${VERSION}-amd64 \
          ${IMAGE}:${VERSION}-arm64

        docker manifest push ${IMAGE}:${VERSION}

        # Create/update 'latest' manifest.
        docker manifest create ${IMAGE}:latest \
          ${IMAGE}:latest-amd64 \
          ${IMAGE}:latest-arm64

        docker manifest push ${IMAGE}:latest
```

### Resulting Dependency Graph

```
# Existing (unchanged):
build-linux-x64 ──┬── package-npm ── create-release
build-linux-arm64 ─┘

# New (independent branch, ADR-004):
build-container-x64 ──┬── create-container-manifest
build-container-arm64 ─┘
```

**Critical**: `create-release` and `package-npm` have NO `needs` dependency on any container job. If container builds fail, binary/npm releases proceed unblocked.

## Key Design Decisions

### No QEMU (Constraint C-3)

- `build-container-x64` runs on `ubuntu-22.04` (x86_64 native).
- `build-container-arm64` runs on `ubuntu-22.04-arm` (ARM64 native).
- No `docker/setup-qemu-action` — QEMU adds 15-25x build time and risks segfaults.

### GHA Docker Layer Cache

- `cache-from: type=gha` reads from GitHub Actions cache.
- `cache-to: type=gha,mode=max` writes all layers to cache.
- Scoped per architecture to prevent cache key collisions.

### Tag Strategy

- Per-arch images: `v{version}-amd64`, `v{version}-arm64`, `latest-amd64`, `latest-arm64`.
- Multi-arch manifest: `v{version}`, `latest`.
- `docker pull ghcr.io/dug-21/unimatrix:latest` resolves to the correct platform automatically.

### GHCR Authentication

- Uses `docker/login-action@v3` with the built-in `GITHUB_TOKEN`.
- No PAT required — `packages: write` permission on the workflow grants push access.

## Error Handling

| Failure | Impact | User Surface |
|---------|--------|-------------|
| ARM64 runner unavailable | `build-container-arm64` fails | Binary/npm release proceeds (ADR-004). Container manifest creation blocked. |
| GHCR push auth failure | Container build job fails | GitHub Actions UI shows auth error. Binary release unaffected. |
| Dockerfile build failure | Container build job fails | Build logs in GitHub Actions. Binary release unaffected. |
| Manifest creation failure | `create-container-manifest` fails | Per-arch images exist but no multi-arch manifest. Users can pull `-amd64` or `-arm64` directly. |

## Key Test Scenarios

1. **Dependency graph independence**: Inspect the YAML. `create-release.needs` and `package-npm.needs` must NOT contain `build-container-*` or `create-container-manifest`.

2. **Permissions**: Workflow-level `permissions` block includes `packages: write`.

3. **Tag format**: `docker/metadata-action` produces `v{version}-amd64` for x64 and `v{version}-arm64` for ARM64. Manifest merges them into `v{version}`.

4. **Cache scoping**: Each job uses a different `scope=` value in `cache-from`/`cache-to` to prevent cross-arch cache pollution.

5. **Native runners**: `build-container-x64` runs on `ubuntu-22.04` (x86_64). `build-container-arm64` runs on `ubuntu-22.04-arm` (ARM64). No QEMU setup action.
