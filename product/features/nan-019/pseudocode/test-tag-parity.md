# Component: Tag-parity static assertion (pre-merge, MUST exist — FR-11 / R-09)

> A pre-merge test artifact (NOT release.yml, NOT the smoke script). Extends the existing
> `infra-001` gate-logic test surface (NOT a new framework; C-13). It is the PRIMARY mitigation
> for R-09 — the tag-strip defect that ACTUALLY OCCURRED in the first design draft. It proves,
> at merge time with no tag push, that the smoke job's resolved per-arch tag string is
> **byte-identical** to what `build-container-*` pushes. RED at merge on any mismatch.

## Purpose

Convert the OCCURRED post-tag-surprise into a pre-merge gate (NFR-10, C-13). The first draft
resolved the push tag **stripped** (`${GITHUB_REF_NAME#v}` ⇒ `:1.2.3-<arch>`) while
`build-container-*` pushes **un-stripped** (`pattern=v{{version}}-<arch>` ⇒ `:v1.2.3-<arch>`) —
a guaranteed `docker pull` 404 on every release. This static check makes any re-introduced
strip, missing/extra `v`, or swapped suffix fail RED locally. (Mitigates SR-01.)

## The two surfaces under parity (byte-identity, both arches)

| Surface | Smoke's resolved tag (from `release-smoke-jobs.md`) | Build's pushed tag (metadata-action) | Must be |
|---------|-----------------------------------------------------|--------------------------------------|---------|
| push, amd64 | `${GITHUB_REF_NAME}-amd64` (un-stripped) | `pattern=v{{version}}-amd64` | byte-identical |
| push, arm64 | `${GITHUB_REF_NAME}-arm64` (un-stripped) | `pattern=v{{version}}-arm64` | byte-identical |
| dispatch, amd64 | `latest-amd64` | `type=raw,value=latest-amd64` | byte-identical |
| dispatch, arm64 | `latest-arm64` | `type=raw,value=latest-arm64` | byte-identical |

`{{version}}` for ref `v1.2.3` resolves to `1.2.3`, then `pattern=v{{version}}` re-prepends the
literal `v` ⇒ `v1.2.3`. So for `GITHUB_REF_NAME=v1.2.3`: build pushes `v1.2.3-amd64`; the smoke
(un-stripped) resolves `v1.2.3-amd64`. Equal. A `${...#v}` strip yields `1.2.3-amd64` ≠ pushed.

## Design — derive both strings, assert equal (no tag push, no Docker)

```
resolveSmokeTag(refName, eventName, arch):
    # mirrors the smoke job's resolution EXACTLY (release-smoke-jobs.md)
    if eventName == "workflow_dispatch": tag = "latest"
    else:                                tag = refName          # UN-stripped — keep the v
    return tag + "-" + arch

resolveBuildPushedTag(refName, eventName, arch):
    # models docker/metadata-action: type=semver,pattern=v{{version}}-<arch> + type=raw,value=latest-<arch>
    if eventName == "workflow_dispatch":            # branch ref → semver emits nothing
        return "latest-" + arch
    else:                                           # v* tag → pattern=v{{version}}-<arch>
        version = semverVersion(refName)            # v1.2.3 -> 1.2.3
        return "v" + version + "-" + arch           # literal v re-prepended by the pattern

TEST assertParity:
    for (refName, eventName) in cases:
        for arch in ["amd64", "arm64"]:
            smoke = resolveSmokeTag(refName, eventName, arch)
            build = resolveBuildPushedTag(refName, eventName, arch)
            assert smoke == build   # byte-identical, else RED
```

`semverVersion("v1.2.3") == "1.2.3"` and `resolveSmokeTag` keeps `v1.2.3` whole; the assertion
`v1.2.3-amd64 == v1.2.3-amd64` holds. A regression that strips the smoke side (`#v`) makes the
smoke side `1.2.3-amd64` while the build side stays `v1.2.3-amd64` → assertion RED.

## Representative cases (the coverage requirement)

| refName | eventName | arch | smoke | build | expect |
|---------|-----------|------|-------|-------|--------|
| `v1.2.3` | push | amd64 | `v1.2.3-amd64` | `v1.2.3-amd64` | EQUAL (green) |
| `v1.2.3` | push | arm64 | `v1.2.3-arm64` | `v1.2.3-arm64` | EQUAL (green) |
| `v0.8.2` | push | amd64 | `v0.8.2-amd64` | `v0.8.2-amd64` | EQUAL (green) |
| any branch | workflow_dispatch | amd64 | `latest-amd64` | `latest-amd64` | EQUAL (green) |
| any branch | workflow_dispatch | arm64 | `latest-arm64` | `latest-arm64` | EQUAL (green) |

Mutation guards (the test MUST go RED if any is re-introduced — R-09):
- smoke side strips the `v` (`${GITHUB_REF_NAME#v}`) → `1.2.3-amd64` ≠ `v1.2.3-amd64`.
- swapped suffix (`smoke-amd64` resolves `-arm64`) → suffix mismatch.
- missing/extra `v` on either side.

## Important — model the metadata-action, do not assume

`resolveBuildPushedTag` is a MODEL of `docker/metadata-action`'s pattern. The test's value comes
from the build side being independently derived from the action's documented `pattern=` /
`value=` semantics (re-prepended literal `v`, semver-extracted version) — NOT copied from the
smoke side (that would make the assertion vacuously true). If the build side is instead read
directly from `release.yml`'s `tags:` block (`type=semver,pattern=v{{version}}-<arch>`,
`type=raw,value=latest-<arch>`), even better — then a future edit to the build's pattern also
moves the assertion. Either way: the two sides must be DERIVED FROM DIFFERENT SOURCES so a
divergence is caught (R-09).

## Error Handling / framework notes

- No new framework — a small static assertion in the existing `infra-001` gate-logic test
  surface (C-13). Fully local + deterministic; no tag push, no Docker, no GHCR.
- Part of the pre-merge gate set; RED at merge on any value mismatch.

## Key Test Scenarios (this artifact IS the scenarios)

- All five representative EQUAL cases above pass (R-09 scenario 1).
- Per-arch suffix correctness — no swapped suffix (R-09 scenario 2).
- A deliberately re-introduced `${...#v}` strip on the smoke side turns the assertion RED with
  no tag push (the OCCURRED-defect regression guard — R-09 / C-13 / NFR-10).

## Open Questions

- **Build-side source (flag for tester):** preferred is to read the build's `tags:` patterns from
  `release.yml` (single source of truth) rather than re-encode them in the test; if that coupling
  is impractical in the chosen harness, the independently-derived model above is acceptable as
  long as it is NOT copied from the smoke resolution. Flagged, not a blocker.
