## ADR-002: In-Test Bundle Emission With a Host/Container Runtime Split

### Context

D-1 LOCKS in-test bundle emission: the doc-test emits the connection bundle by running the
shipped CLI's `client-bundle` command rather than using a pre-staged fixture, so it covers
the whole documented attach path (a staged fixture would re-introduce the docs-vs-reality
gap this feature closes). SR-01/A1 required confirming `client-bundle`'s name/signature in
the shipped image; SR-03/A3 required confirming both runtimes the attach path uses are
present where the test invokes them.

Design-time verification found a decisive constraint: the shipped runtime image is
`gcr.io/distroless/cc-debian12:nonroot` (`Dockerfile:110`) and copies ONLY the Rust
`unimatrix` binary + `libonnxruntime.so` + `/data`,`/shared`. It ships **no `node` and no
`packages/unimatrix` JS**. But the documented attach path is two-runtime: `client-bundle`
(Rust, server-side) THEN `init --bundle` (JS, npm package, client-side). A3's assumption
that "the image ships both runtimes" is FALSE for the JS half. So "run the whole attach path
inside the container" is impossible and, more importantly, would be unsound — it would test
an environment no operator has.

### Decision

**Emit the bundle inside a throwaway container off the shipped image (Rust
`client-bundle`); consume it on the CI host (JS `init --bundle` from the repo's
`packages/unimatrix`). The host is the operator surrogate.**

Concrete flow (Gates 5–7, appended after the nan-019 Gate 4):

1. `BUNDLE=$(docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data client-bundle
   "$SLUG")` — Rust binary, in-container, reads the same data volume Gates 1–4 used.
   Verified signature: `Command::ClientBundle { slug }`, sync pre-tokio
   (`main.rs:293,437`); stdout is the opaque `unimatrix-bundle:` blob.
2. `node packages/unimatrix/bin/unimatrix.js init --bundle "$BUNDLE" --project-dir
   "$WORKDIR"` — JS, on the host (Node confirmed present). `init` decodes the blob, pins the
   leaf cert by the carried `sha256:` fingerprint, writes the out-of-tree credential, wires
   the hook client, and validates with a pinned `Ping` over fingerprint-pinned HTTPS to
   `https://localhost:PORT` — the exact posture a real client gets. **No `--slug`** — it is
   retired on the bundle path (`init.js:353`).
3. Fire one hook event through the wired hook client; assert the resulting `POST` to the
   bundle's server-composed `observe_url` (`/v1/<slug>/observe`) returns 204 and the per-slug
   store grows (reusing nan-019's `store_size` delta).

This split is faithful: an operator runs `client-bundle` on the server and `init` on their
own machine. Putting `init` "in the container" would require injecting `node` + the npm
package into a distroless image no shipped artifact contains — testing fiction, not reality.

Every step that can fail hard-fails via `fail()` (exit 1) with a step-naming message
(ADR-001), incl. `client-bundle` rc≠0 (names the command for the SR-02 rename case), empty/
malformed blob, `node` absent, `init` rc≠0, non-204 observe, store-no-grow.

**Node MUST be EXPLICITLY provisioned on the smoke runner via a pinned `setup-node` step —
not relied on as incidental presence.** The doc-test makes `node`-absence a hard-fail (exit 1,
same class as Docker-absent — ADR-001). nan-019's `smoke-amd64`/`smoke-arm64` jobs require
only Docker; they currently carry NO `setup-node` step (verified: `release.yml:406–446` —
`checkout` + GHCR login + `run_smoke_gate` only) and depend on whatever `node` the
`ubuntu-22.04` runner image happens to ship. If the host JS leg's hard-fail is left riding on
that incidental presence, an unrelated runner-image change (GitHub dropping or moving `node`)
would silently arm a release-BLOCKER on a surface nan-020 never declared a dependency on. That
is exactly the "don't depend on unpinned infra" failure #793 is fixing for `busybox` (pin it).
The host JS leg's hard-fail must be **INTENTIONAL, not latent**. Therefore:

- Add a named, version-pinned `actions/setup-node` step to BOTH smoke jobs in `release.yml`,
  immediately after `actions/checkout@v4` and before the `run_smoke_gate` step:

  ```yaml
        - name: Provision pinned Node for the documented init --bundle leg (nan-020)
          uses: actions/setup-node@v4
          with:
            node-version: '24'
  ```

  Pin `24` to match the `package-npm` job's `setup-node@v4` `node-version: '24'`
  (`release.yml:215–218`) — the same node major that publishes and is therefore the operator's
  install surface. Delivery names this step deliberately; it is the provisioning half of the
  contract whose enforcement half is ADR-001's `node`-absent hard-fail.
- The `node`-absent hard-fail (ADR-001) is then a genuine SAFETY NET for an out-of-band
  provisioning regression (someone deletes/renames the step), never the primary acquisition
  mechanism. The `command -v node` preflight stays — provisioning + assertion are
  complementary, not redundant.

### Consequences

- Easier: the doc-test exercises the real operator topology, so a break in either the Rust
  emit surface OR the JS consume surface is caught — and is correct signal (SR-02: a CLI
  rename breaks the test, message names the command). D-1's "cover the command operators
  actually run" intent is fully met.
- Harder: the doc-test depends on `node` + the repo's `packages/unimatrix` on the smoke
  host, and on `--project-dir`/HOME hermeticity across CI runs (ADR-005, was OQ-C). The host
  must be a Docker-AND-node lane; both absences hard-fail (never skip). The `node` dependency
  is now made EXPLICIT in `release.yml` via a pinned `setup-node@v4` step rather than left
  incidental, so the host JS leg's hard-fail is intentional and a runner-image change cannot
  silently arm a release-blocker (the #793 "pin the infra you depend on" discipline).
- Correction to record: SR-03/A3 "image ships both runtimes" is FALSE; only the Rust binary
  is in the image. This ADR resolves SR-03 by relocating the JS half to the host rather than
  by adding JS to the image. (Cross-ref ADR-001 for exit handling; ADR-003 for why this one
  chain is the tested set; ADR-005 for the hermeticity proof obligation on the host consume
  step.)
