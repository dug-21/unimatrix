## ADR-004: Container CI Jobs Independent of Binary/npm Release Jobs

### Context

SR-10 (Medium severity) identifies that adding container build jobs to `release.yml` creates coupling risk. If ARM64 runner availability is flaky (GHA ARM runners are newer infrastructure), the entire release pipeline could block — preventing binary and npm releases from reaching users even though they are unaffected by container build issues.

The existing `release.yml` dependency chain is:

```
build-linux-x64 ──┬── package-npm ── create-release
build-linux-arm64 ─┘
```

### Decision

Container build jobs form a separate, independent branch in the workflow dependency graph. No `needs` dependency exists between the container branch and the binary/npm branch:

```
build-linux-x64 ──┬── package-npm ── create-release
build-linux-arm64 ─┘

build-container-x64 ──┬── create-container-manifest
build-container-arm64 ─┘
```

Both branches trigger on the same `v*` tag push. Both run in the same `release.yml` workflow file (avoiding workflow coordination complexity). But they are independent subgraphs.

`create-release` (the GitHub Release with changelog) does not depend on `create-container-manifest`. If container builds fail, the binary release and npm packages still ship. The container manifest is a separate artifact.

Container jobs use `docker/build-push-action@v6` with `docker/login-action@v3` for GHCR authentication. Cache strategy: `cache-from: type=gha` and `cache-to: type=gha,mode=max` for Docker layer caching via GitHub Actions cache backend.

The `packages: write` permission is added at the workflow level alongside existing `contents: write` and `id-token: write`. This is the minimum scope needed for GHCR push via `GITHUB_TOKEN`.

### Consequences

- **Easier**: Binary and npm releases are never blocked by container infrastructure issues (ARM runner unavailability, Docker cache eviction, GHCR outages).
- **Easier**: Container builds can be individually re-run without triggering binary rebuilds.
- **Easier**: Future addition of container tests (e.g., `docker run ... health` smoke test) adds to the container branch only.
- **Harder**: A release where container builds fail silently requires manual monitoring. Mitigation: GitHub Actions UI shows job status per branch; a failure notification is inherent. A follow-up can add a Slack/Discord notification on container job failure.
- **Harder**: The `create-release` changelog does not mention container availability. Acceptable: the container tag matches the release tag, making discovery straightforward.
