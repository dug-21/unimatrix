## ADR-004: workflow_dispatch Trigger and Per-Arch Tag Resolution Across Both Trigger Surfaces

### Context

OQ-5 is DECIDED: the gate fires on **tag `v*` push** (the release) AND on
**`workflow_dispatch`** (a human dry-run before cutting a release, e.g. after a
release-config change), but explicitly **NOT** on `pull_request` (SR-06). The
release workflow currently triggers only on `push.tags: ['v*']`.

This creates a tag-resolution problem unique to the smoke jobs (ADR-002): the
smoke must pull the **exact per-arch pushed tag**, and the tag string differs by
trigger surface.

- On `v*` push, `${{ github.ref_name }}` is `v<version>` and the build jobs'
  `docker/metadata-action` (`type=semver,pattern=v{{version}}-<arch>`) pushes
  `:v<version>-<arch>` — the pattern's literal `v` is RETAINED: `{{version}}`
  resolves to `0.8.3` for tag `v0.8.3`, and the leading `v` in the pattern makes
  the pushed tag `v0.8.3-amd64` (NOT `0.8.3-amd64`). The existing
  `create-container-manifest` job confirms this from the other side: it uses
  `version=${GITHUB_REF_NAME}` (UN-stripped, = `v0.8.3`) and assembles
  `:v0.8.3-amd64` (release.yml 410, 421–423). Both definers agree and KEEP the
  `v`. Whatever the smoke pulls MUST equal what the build pushed.
- On `workflow_dispatch`, `github.ref_name` is a branch (e.g. `main`), there is
  no semver tag, and `type=semver` emits **no** tag — so the build jobs push only
  `:latest-<arch>` (the `type=raw,value=latest-<arch>` line). A dispatch run that
  tried to pull `:<branch>-<arch>` would 404.

The smoke jobs therefore cannot blindly use `github.ref_name`; they need one
resolution rule that yields the correct pushed tag on **both** surfaces, and that
stays in lockstep with what the build jobs actually push.

### Decision

1. **Add `workflow_dispatch` to the workflow triggers**, alongside the existing
   tag push:

   ```yaml
   "on":
     push:
       tags: ['v*']
     workflow_dispatch: {}
   ```

   No inputs are required for the default dry-run (it targets `:latest-<arch>`,
   see below). `pull_request` is NOT added (SR-06).

2. **Resolve the per-arch tag once, in each smoke job, with an explicit rule that
   mirrors the build jobs' metadata-action output:**

   - If `github.event_name == 'push'` (tag): the version tag is
     `${GITHUB_REF_NAME}` (UN-stripped — KEEP the leading `v`), giving
     `v<version>`; the pulled image is `:v<version>-<arch>` — byte-identical to
     the `pattern=v{{version}}-<arch>` tag the build pushed and to the
     `version=${GITHUB_REF_NAME}` tag the manifest consumes. Do **not** strip the
     `v` (`${GITHUB_REF_NAME#v}` would resolve to `:0.8.3-amd64`, which is never
     pushed → smoke pull 404 → manifest blocked on every release).
   - If `github.event_name == 'workflow_dispatch'`: pull `:latest-<arch>` — the
     `type=raw,value=latest-<arch>` tag pushed verbatim by the build jobs on
     every run regardless of trigger. This is exactly the dry-run intent: smoke
     whatever the build branch just pushed as `latest` on this dispatch.

   Expressed as a step that emits `TAG` for the smoke invocation:

   ```bash
   if [ "${{ github.event_name }}" = "push" ]; then
     TAG="${GITHUB_REF_NAME}"          # v0.8.3 (KEEP v) -> :v0.8.3-<arch>
   else
     TAG="latest"                      # dispatch dry-run -> :latest-<arch>
   fi
   IMAGE="ghcr.io/${{ github.repository_owner }}/unimatrix:${TAG}-${ARCH}"
   ```

   where `ARCH` is the job's literal arch (`amd64` / `arm64`).

   This keeps the smoke and build push in lockstep through the **same** version
   source the build's metadata-action uses, on each trigger. Because both build
   and smoke run in the same workflow invocation, the `latest-<arch>` the smoke
   pulls on dispatch is the one just pushed by this run's build jobs (ordered by
   the `needs:` edge, ADR-001).

3. **Manifest is SKIPPED on dispatch** (refines ADR-001). The manifest keeps
   `needs: [smoke-amd64, smoke-arm64]` but ALSO gains
   `if: github.event_name != 'workflow_dispatch'`. Rationale: on a dispatch off a
   branch, semver yields nothing off-tag, so the build jobs push only
   `:latest-<arch>`; the smokes pull `:latest-<arch>` and pass, but the manifest
   job's existing logic resolves `version=${GITHUB_REF_NAME}` = `<branch-name>`
   and would `imagetools create :<branch>` from `:<branch>-amd64`/`-arm64` —
   tags that were never pushed → the manifest job fails, reddening the dry-run
   downstream and masking the real signal. Gating the manifest off on dispatch
   makes the **smoke-* job statuses the only meaningful dispatch signal**. On a
   `v*` push the manifest runs normally and is gated on both smokes (ADR-001). It
   does not create a release object on either surface (no `create-release`
   coupling, ADR-004 #4572).

4. **Pre-merge tag-parity assertion** (honors the verify-by-name thesis): a
   bounded shell/unit check in the existing local gate-logic test surface asserts
   the smoke's resolved per-arch tag equals what `build-container-*` pushes —
   push: `${GITHUB_REF_NAME}` ⇒ `v<version>-<arch>` == `pattern=v{{version}}-<arch>`;
   dispatch: `latest-<arch>` == `value=latest-<arch>`. This converts a re-stripped-`v`
   regression (defect class above) from a post-tag discovery into a pre-merge
   failure. No new framework — just the two-surface formula-equality check.

### Consequences

- Easier: A maintainer can validate the deployability gate on demand (after a
  release-config change) without cutting a tag — the human dry-run OQ-5 asked for.
- Easier: One resolution rule covers both surfaces and is anchored to the build
  jobs' actual push behavior — UN-stripped `${GITHUB_REF_NAME}` on push, byte-
  identical to the metadata-action pattern AND the existing manifest job — so
  smoke-pulls-wrong-tag (a silent 404) is structurally avoided and now also
  asserted pre-merge (point 4).
- Easier: Gating the manifest off on dispatch keeps the dry-run signal honest —
  a green dispatch means "both arches smoked clean," not muddied by a manifest
  job that can never succeed off-tag.
- Harder: The dispatch path smokes `:latest-<arch>` rather than a version tag —
  acceptable because dispatch is a dry-run, not a release; but it means a
  dispatch run validates "the latest pushed bytes," which is the correct dry-run
  semantics, not a specific historical version.
- Harder: The resolution rule is coupled to the metadata-action tag patterns
  (`pattern=v{{version}}-<arch>` and `value=latest-<arch>`). Both are named in the
  Integration Surface; a change to either must move the smoke rule in lockstep.
- Related: ADR-002 (pushed-bytes — this ADR resolves *which* pushed tag),
  ADR-001 (the `needs:` ordering that makes the just-pushed tag available).
