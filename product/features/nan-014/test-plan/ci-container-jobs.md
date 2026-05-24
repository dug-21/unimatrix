# Test Plan: ci-container-jobs

## Component

Modified file: `.github/workflows/release.yml`. Adds 3 new jobs: `build-container-x64`, `build-container-arm64`, `create-container-manifest`. Adds `packages: write` permission.

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-10 (Med) | Container jobs block binary/npm release | `needs` dependency graph static analysis |
| R-10 (Med) | Container jobs are independent branch | Job dependency review |

## Static Analysis Tests

All ci-container-jobs tests are static YAML analysis (grep/review). No runtime execution.

### AC-08: Container jobs exist with correct structure

**Act**: Parse `.github/workflows/release.yml`.

**Assert**:
- Job `build-container-x64` exists with `runs-on: ubuntu-22.04`
- Job `build-container-arm64` exists with `runs-on: ubuntu-22.04-arm` (or equivalent ARM runner)
- Job `create-container-manifest` exists with `needs: [build-container-x64, build-container-arm64]`
- All three jobs have `permissions` or inherit workflow-level `packages: write`

### R-10: No cross-dependency between container and binary branches

**Act**: Extract `needs` arrays from all jobs.

**Assert**:
- `create-release` does NOT list `build-container-x64`, `build-container-arm64`, or `create-container-manifest` in its `needs`
- `package-npm` does NOT list any container job in its `needs`
- `build-container-x64` does NOT list `build-linux-x64` or `build-linux-arm64` in its `needs`
- `build-container-arm64` does NOT list `build-linux-x64` or `build-linux-arm64` in its `needs`

The dependency graph must be:
```
build-linux-x64 ---+--- package-npm --- create-release
build-linux-arm64 -+
build-container-x64 ---+--- create-container-manifest
build-container-arm64 -+
```

### Permissions check

**Act**: Grep `permissions:` block in release.yml.

**Assert**:
- `packages: write` is present at workflow level (or on each container job individually)
- Existing `contents: write` and `id-token: write` are preserved

### GHCR login configuration

**Act**: Review container job steps.

**Assert**:
- Uses `docker/login-action` with `registry: ghcr.io`
- Uses `${{ github.token }}` or `${{ secrets.GITHUB_TOKEN }}` (no PAT)
- Uses `docker/build-push-action` with `push: true`
- Uses `cache-from: type=gha` and `cache-to: type=gha,mode=max`

### Image tag configuration

**Act**: Review `create-container-manifest` job.

**Assert**:
- Creates manifest at `ghcr.io/dug-21/unimatrix:v{version}`
- Also tags `ghcr.io/dug-21/unimatrix:latest`
- Uses `docker/metadata-action` or manual `docker manifest create`

## Validation Checklist (Code Review)

- [ ] Three new jobs added: `build-container-x64`, `build-container-arm64`, `create-container-manifest`
- [ ] Triggered on `v*` tag push (same trigger as existing binary jobs)
- [ ] `packages: write` permission added
- [ ] No `needs` link between container branch and binary/npm branch
- [ ] ARM64 job uses native ARM runner (no QEMU)
- [ ] Both per-arch jobs push to GHCR with platform-specific suffix tags
- [ ] Manifest job merges both arch images
- [ ] All existing jobs unchanged (no modification to binary/npm pipeline)

## Integration Tests

No infra-001 tests. CI pipeline validation is static analysis of YAML structure.
