## ADR-002: The Smoke Tests the Pushed GHCR Per-Arch Bytes via IMAGE=, Never a Rebuild

### Context

The feature exists because the **shipped image** misbehaved on first run
(#783 booted HTTP-off; #774 host-allowlist missing). A smoke that rebuilds the
image from the `Dockerfile` on the smoke runner tests *a rebuild*, which is
definitionally not the artifact operators pull — it would miss exactly the
class nan-019 exists to close (SR-02, High). The smoke script already supports
reusing a prebuilt tag via `IMAGE=` (default behavior builds locally; this is
the path we must NOT use in CI). OQ-2 is DECIDED: test the pushed bytes;
self-build is rejected (AC-06).

The release already pushes per-arch tags to GHCR:
`build-container-x64` pushes `ghcr.io/<owner>/unimatrix:v<version>-amd64`,
`build-container-arm64` pushes `:v<version>-arm64` (plus `:latest-<arch>`), via
`docker/metadata-action` `type=semver,pattern=v{{version}}-<arch>` — the
pattern's literal `v` is retained, so the pushed tag KEEPS the `v` (tag `v0.8.3`
→ `:v0.8.3-amd64`, not `:0.8.3-amd64`). The smoke jobs must consume those exact
tags.

### Decision

Each smoke job authenticates to GHCR and invokes the smoke with `IMAGE=` set to
its own-arch pushed tag — no `docker build` runs on the smoke runner:

1. **GHCR login** in each smoke job, identical to the build/manifest jobs:
   `docker/login-action@v3` with `registry: ghcr.io`,
   `username: ${{ github.actor }}`, `password: ${{ secrets.GITHUB_TOKEN }}`.
   Read scope on `GITHUB_TOKEN` suffices to pull; no new secret (SCOPE: no new
   secrets).
2. **Resolve the version tag** the same way the build jobs do. On a `v*` tag
   push, `${{ github.ref_name }}` is the `v<version>` string the metadata-action
   used (`type=semver,pattern=v{{version}}-<arch>` emits the tag verbatim as
   `v<version>-<arch>`). On `workflow_dispatch` (ADR-005), `github.ref_name` is a
   branch, not a version — so the smoke job derives the tag the same way it must
   on dispatch (see ADR-005 for the dispatch tag source). The job MUST resolve to
   the **same** string the build job pushed; a mismatch makes the smoke pull a
   stale or absent image.
3. **Invoke** `IMAGE=ghcr.io/${{ github.repository_owner }}/unimatrix:<resolved-tag>-<arch> \
   bash product/test/infra-001/scripts/docker-http-posture-smoke.sh`.
   With `IMAGE=` set, the script logs `using prebuilt image` and skips its build
   branch entirely — the pulled bytes are the bytes under test.
4. **No duplicate build.** The smoke step contains no `docker build`; the only
   image acquisition is the implicit `docker pull` GHCR performs when the script
   `docker run`s the `IMAGE` tag (or an explicit `docker pull` before, for a
   clearer failure if the tag is unpullable). AC-06: no second full image build.

The smoke's existing `docker image inspect ... UNIMATRIX_HTTP_ENABLED=true`
sanity check then runs against the **pulled** image ENV — proving the posture is
baked into the *shipped* layer, not a local rebuild.

### Consequences

- Easier: The gate validates the literal artifact operators receive — closes the
  #783/#5130 "shipped image misbehaves on first run" class for real.
- Easier: No ONNX-bearing rebuild on the smoke runner — the costly build already
  happened in `build-container-*`; the smoke pays only pull + boot (SR-03 cost
  mitigation).
- Harder: The smoke job is coupled to the metadata-action tag scheme. If a future
  change alters `pattern=v{{version}}-<arch>`, the smoke's tag derivation must
  move in lockstep — documented in the Integration Surface so it is not silently
  drifted (a wrong tag = pull failure, which fails the gate loudly, not silently).
- Harder: `workflow_dispatch` has no `v*` tag, so the dispatch path needs an
  explicit version/tag input or a fixed target (ADR-005) — added complexity, but
  required to let a human dry-run the exact pushed bytes.
- Related: ADR-001 (each smoke `needs:` its push so the bytes exist before pull),
  ADR-003 (the pulled image must then pass the run-marker gate), ADR-005
  (dispatch tag resolution).
