# nan-019 — Specification: Standing Release Gate for the Docker HTTP-Posture Smoke

> **NFR-maintenance feature.** Advances capability **N5** (#5163 — "the shipped artifact is always deployable as released") and guards **N3** (writes are integrity-protected — never mis-routed across projects). This feature adds NO new runtime behavior to the shipped artifact; it wires the existing `product/test/infra-001/scripts/docker-http-posture-smoke.sh` (built in #786) into `.github/workflows/release.yml` as a standing, verify-by-name gate that blocks the multi-arch manifest on failure. Source: `SCOPE.md` (locked) + `SCOPE-RISK-ASSESSMENT.md`.

---

## Objective

Every release of the GHCR multi-arch container image must be exercised end-to-end against the **actual pushed bytes** on **both** architectures (amd64 + arm64) before the multi-arch manifest operators pull is released. The gate runs the existing `docker-http-posture-smoke.sh` — build-reuse → boot-HTTP-on → register slug → restart → per-slug HTTPS write → assert-landed — and is wired so that a smoke failure, a self-skip, or a silent early-exit-0 all block the manifest loudly. This closes the shipped-but-broken-on-first-run class (#774, #783) for both arches and kills the false-green class (#4796/#4970) by verifying the smoke provably ran to completion.

---

## Domain Models / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Smoke** | The existing script `product/test/infra-001/scripts/docker-http-posture-smoke.sh`. It builds (or reuses via `IMAGE=`) the production image, boots it with no HTTP override, registers a slug, restarts, POSTs a `SessionRegister` over the cert-pinned per-slug HTTPS route, and asserts the write landed in the per-slug store. Exit contract: **0 = ran + passed, 1 = ran + failed, 3 = self-skipped (Docker absent)**. |
| **Gate** | The CI wiring in `release.yml` that runs the smoke and decides pass/fail. "Verify-by-name" gate: a green result must *provably* mean "the smoke ran to its end and its behavioral assertions passed," never "the job exited 0." |
| **Run-marker** | The smoke's terminal success line `ALL GATES PASSED` (script line 142). Its presence in captured smoke output is positive proof the smoke reached its end. A green job MUST assert this marker was emitted; its absence — even with exit 0 — is a gate failure. |
| **Self-skip** | The smoke's `exit 3` path (script lines 47–51) when Docker is unavailable in the lane. Under the gate, a self-skip is a **hard failure**, never a deferred/green step. |
| **Per-arch tag** | The GHCR-pushed intermediate image for one architecture, produced by `build-container-x64` / `build-container-arm64`. The pushed tag string KEEPS the `v` prefix on a `v*` tag push: `build-container-*` pushes via `docker/metadata-action` `type=semver,pattern=v{{version}}-<arch>`, so for ref `v1.2.3` the pushed tag is `ghcr.io/<owner>/unimatrix:v1.2.3-amd64` (and `:v1.2.3-arm64`) — the literal `v` is re-prepended by the pattern and is NOT stripped. The consuming side (the existing manifest job) resolves the same un-stripped form: `VERSION="${GITHUB_REF_NAME}"` ⇒ `:${VERSION}-<arch>` = `:v1.2.3-<arch>`. On `workflow_dispatch` (branch ref → `type=semver` emits no tag) only the `type=raw,value=latest-<arch>` tag is pushed, so the resolved tag is `:latest-<arch>`. These pushed bytes are exactly what the smoke must test (not a rebuild). |
| **Manifest** | The multi-arch OCI index `ghcr.io/<owner>/unimatrix:<tag>` (and `:latest`) assembled by `create-container-manifest`. This is the tag operators actually pull; gating it ensures no released artifact is ever un-smoked. |
| **Per-slug store** | `/data/.unimatrix/<slug>/unimatrix.db` — the integrity-correct destination for a write routed through the registered slug (N3). |
| **Hash store** | The path-hash data dir (sibling of per-slug dirs; holds the bearer token + TLS cert). The #783 symptom was a write mis-routed here instead of the per-slug store. |
| **False-green** | A job that reports success while its behavioral assertions never executed (self-skip) or exited 0 early (before the run-marker). The single load-bearing failure mode this feature exists to prevent (lessons #4796/#4970; pattern #5180). |
| **Pushed bytes** | The exact per-arch image artifact that was pushed to GHCR during this release run. Testing pushed bytes (via `IMAGE=` + GHCR login) — not a rebuild on the smoke runner — is mandatory, because "the shipped image misbehaves on first run" can only be proven against what ships. |

---

## Functional Requirements

Each requirement is testable; verification methods appear in the Acceptance Criteria section.

- **FR-01 — Per-arch smoke jobs.** Add two jobs to the container branch of `.github/workflows/release.yml`: `smoke-amd64` (runs on `ubuntu-22.04`) and `smoke-arm64` (runs on `ubuntu-22.04-arm`). Each invokes `bash product/test/infra-001/scripts/docker-http-posture-smoke.sh` against the pushed per-arch image. (AC-01)

- **FR-02 — Trigger surface.** The release workflow runs the smoke jobs on (a) `push` of a tag matching `v*`, and (b) `workflow_dispatch` (human pre-release dry-run). The smoke jobs MUST NOT run on `pull_request`. The `workflow_dispatch` trigger must be added to the workflow `on:` block (currently only `push.tags: ['v*']`). (AC-01)

- **FR-03 — Test the pushed bytes, not a rebuild.** Each smoke job authenticates to GHCR (`docker login`, reusing `GITHUB_TOKEN` — no new secret) and runs the smoke with `IMAGE=ghcr.io/<owner>/unimatrix:<resolved-tag>-<arch>`, pointing at the exact per-arch tag pushed earlier in the same run. The resolved tag string MUST be byte-identical to what `build-container-*` pushed. On a `v*` push the pushed tag keeps the `v` (metadata-action `pattern=v{{version}}-<arch>`), so the smoke resolves the version UN-stripped — `VERSION="${GITHUB_REF_NAME}"` ⇒ `:${VERSION}-<arch>` = `:v1.2.3-<arch>` — never `${GITHUB_REF_NAME#v}` (stripping the `v` yields `:1.2.3-<arch>`, which was never pushed and 404s). On `workflow_dispatch` the resolved tag is `:latest-<arch>` (the only tag the build pushes on a branch ref). No `docker build` of the production image runs in the smoke job. `smoke-amd64` orders after the amd64 push (`needs: [build-container-x64]`); `smoke-arm64` orders after the arm64 push (`needs: [build-container-arm64]`). (AC-06)

- **FR-04 — Per-arch smoke behavior (inherited from the existing smoke).** Each smoke job, against its per-arch image, performs in order: (i) assert image ENV carries `UNIMATRIX_HTTP_ENABLED=true`; (ii) clean `docker run` with no HTTP override and confirm the daemon logs `HTTP transport active` (failing fast on the `set [http] enabled` HTTP-off hint); (iii) register slug `arch-research`; (iv) restart and confirm the listener re-activates; (v) POST a `SessionRegister` over the cert-pinned per-slug route `https://localhost:18443/v1/<slug>/observe` and require HTTP `204`; (vi) assert the per-slug store `/data/.unimatrix/<slug>/unimatrix.db` exists. This behavior is provided by the existing script and is NOT re-implemented in YAML. (AC-01)

- **FR-05 — AC-05 grew-assertion (in-scope smoke hardening).** Extend `docker-http-posture-smoke.sh` so that, after the per-slug write, it asserts BOTH: (a) the per-slug store `/data/.unimatrix/<slug>/unimatrix.db` **grew** as a result of the write (measurably larger than its pre-write size), AND (b) the hash store did **not** receive the write (its observation/db footprint is unchanged by the write). The assertion pins the data-landing check to the literal #783 symptom ("slug dir empty, hash dir populated"). It MUST use the existing read-only `busybox` sidecar pattern (`vol()`), never `docker exec` into the distroless runtime image. It is a single bounded assertion pair — no new test scenarios, no new scripts. (AC-05)

- **FR-06 — Manifest gating.** `create-container-manifest` gains `needs: [smoke-amd64, smoke-arm64]`. The builds stay in the dependency graph transitively — each smoke `needs:` its own-arch build (`smoke-amd64 needs: [build-container-x64]`, `smoke-arm64 needs: [build-container-arm64]`) — so re-listing the builds on the manifest is redundant and omitted. The multi-arch manifest is assembled and pushed ONLY if both per-arch smokes pass. A failed smoke on either arch leaves the manifest tag unreleased. (AC-02, AC-04)

- **FR-10 — Dispatch-gating of the manifest job.** `create-container-manifest` is gated OFF on `workflow_dispatch` via `if: github.event_name != 'workflow_dispatch'`. On a dispatch (dry-run) run the build pushes only `:latest-<arch>` and `${GITHUB_REF_NAME}` is a branch, so the existing manifest job — which assembles `:${GITHUB_REF_NAME}` from `:${GITHUB_REF_NAME}-<arch>` — would try to assemble a per-arch tag that was never pushed and go red on a false signal. With the manifest job skipped on dispatch, the ONLY meaningful pass/fail signal on a dispatch run is the `smoke-amd64`/`smoke-arm64` job statuses (each smoking the just-pushed `:latest-<arch>` bytes). On a `v*` push the manifest job runs normally and stays gated by both smokes (FR-06). (AC-01, AC-08)

- **FR-07 — Skip-is-failure keying.** The gate captures the smoke's exit code explicitly and treats ONLY `0` as success. Exit `3` (self-skip / Docker absent) and exit `1` (ran + failed) BOTH fail the job, with `exit 3` emitting a clear diagnostic ("smoke SKIPPED — Docker-capable lane is mis-provisioned; this is a hard failure, not a deferred step"). The capture must not let `set -e`/pipefail or a YAML `if:` swallow the code into a green result. (AC-03)

- **FR-08 — Positive run-marker assertion.** Beyond exit-code keying, the gate captures the smoke's combined stdout/stderr and asserts the terminal run-marker `ALL GATES PASSED` is present. A run that exits 0 but did not emit the marker (a future early-exit-0) fails the job. A green job requires BOTH exit 0 AND the run-marker present. (AC-03)

- **FR-09 — No silent retry.** Neither smoke job wraps the smoke in `|| retry`, a retry loop, or `continue-on-error: true` for transient Docker/network flake. A flaky deployability gate fails and is treated as a signal, not papered over. (AC-02, AC-03)

- **FR-11 — Pre-merge tag-parity assertion.** A bounded, pre-merge-provable check asserts that the smoke job's resolved per-arch tag string is byte-identical to the tag `build-container-*` pushes — push: `v<version>-<arch>`; dispatch: `latest-<arch>` — so a tag-string VALUE mismatch (e.g. a stray `${...#v}` strip) is caught at merge time, not at release time. The check is a small static assertion in the existing local gate-logic test surface: it derives both strings from the same expression, OR a tiny test that asserts the smoke's resolution formula equals the metadata-action `tags:` patterns (`pattern=v{{version}}-<arch>` and `value=latest-<arch>`) for representative refs (`v1.2.3` ⇒ `:v1.2.3-<arch>`; a branch ⇒ `:latest-<arch>`). It is NOT a new test framework and does NOT require pushing a tag or a post-tag CI run to prove. (AC-06)

---

## Non-Functional Requirements

- **NFR-01 — Verify-by-name (load-bearing).** A green smoke job MUST provably mean "the smoke ran to its end and passed." Required mechanism: exit-code keyed on the three distinct codes (`0`/`1`/`3`) AND run-marker (`ALL GATES PASSED`) asserted present. Job-exit-0 alone is insufficient proof. *Measurable:* a forced `exit 3` and a forced early `exit 0` (no marker) both produce a red job. (Mitigates SR-01; pattern #5180.)

- **NFR-02 — Skip-is-failure.** A self-skip (Docker absent → `exit 3`) is a hard job failure with a clear diagnostic, never green or "deferred." *Measurable:* on a lane without Docker the job is red and the diagnostic appears in the log. (SR-01.)

- **NFR-03 — ADR-004 container-lane independence (#4572).** Wiring introduces NO `needs` edge between the container/smoke branch and the binary/npm branch. `build-linux-*`, `package-npm`, and `create-release` are neither blocked by nor able to block the smoke jobs; the smoke jobs depend only on container-branch jobs. *Measurable:* a smoke failure leaves `create-release` and `package-npm` reachable/unaffected; no smoke job appears in any binary/npm job's `needs`, and vice versa. (SR-08.)

- **NFR-04 — No new secrets.** The smoke needs only GHCR read to pull a pushed image, satisfied by `docker login` with the existing `GITHUB_TOKEN`/`secrets.GITHUB_TOKEN` and the existing `packages: write` permission. The token + TLS cert for the HTTPS write are read from the running container's data volume via the busybox sidecar. No new repository/organization secret is introduced. *Measurable:* the diff adds no `secrets.*` reference beyond `GITHUB_TOKEN`. (SCOPE Non-Goals; SR — no new credentials.)

- **NFR-05 — No duplicate image build.** The smoke reuses the pushed per-arch bytes via `IMAGE=`; it does NOT run a full `docker build` of the ONNX-bearing production image. *Measurable:* neither smoke job contains a `docker build`/`buildx build` of the production image; the smoke log shows "using prebuilt image: ghcr.io/...". (SR-02, SR-03; cost constraint.)

- **NFR-06 — Arch coverage must be named — no silent cap (HARD RULE).** The gate proves N5 on BOTH arches. A silent "amd64-only = N5-proven" outcome is FORBIDDEN. The only acceptable fallback (not the chosen path) is amd64-now + arm64 as a NAMED, TRACKED fast-follow with N5 staying PARTIAL and the arm64 gap explicitly logged — never buried under a green checkmark. The decided/preferred path is both arches smoked. *Measurable:* both `smoke-amd64` and `smoke-arm64` exist and gate the manifest; any deferral is recorded as a tracked item, not silently dropped. (SR-07; N5 `done_when` #5163.)

- **NFR-07 — Cold-boot timeout tolerance.** The smoke's boot-wait/log-poll deadlines (currently 90s per the existing script) must tolerate a cold arm64 first-boot embedding-model load (#767, plausibly arch-sensitive) without masking a real hang. If the existing 90s budget proves insufficient on `ubuntu-22.04-arm`, the architect may widen it; the spec requires the timeout be generous enough not to false-fail a healthy cold arm64 boot, while still bounding a true hang. *Measurable:* `smoke-arm64` passes on a healthy arm64 cold boot in the post-tag run (AC-07). (SR-03.)

- **NFR-08 — Test infrastructure is cumulative.** The only script change is FR-05 (AC-05), extending the existing `infra-001` smoke in place via the established busybox-sidecar pattern. No parallel/duplicate smoke script is created; the smoke's behavioral logic is not re-implemented in YAML. (SR-05; project rule.)

- **NFR-09 — Briefly-public un-smoked intermediates are accepted.** The per-arch tags (`:v<version>-amd64`/`-arm64` on push; `:latest-<arch>` on dispatch) are pushed (public) before their smokes run; only the manifest is gated. This is an accepted, documented consequence — operators pull the manifest, which is never released until both smokes pass. It is NOT a defect to be re-litigated downstream. (SR-09; DECIDED OQ-1/OQ-2.)

- **NFR-10 — Tag-string parity is pre-merge-provable (load-bearing).** The smoke's resolved per-arch tag string MUST equal the build's pushed tag for both trigger surfaces, and this equality MUST be provable before merge — never deferred to the post-tag run. Required mechanism: a bounded static parity assertion (FR-11) derived from, or asserted equal to, the `docker/metadata-action` `tags:` patterns. *Measurable:* a deliberate value mismatch (e.g. re-introducing a `${...#v}` strip on the push path) turns the local gate-logic assertion RED with no tag push. This abolishes the post-tag-surprise the feature exists to prevent. (SR-01.)

- **NFR-11 — Dispatch manifest gating.** On `workflow_dispatch` the multi-arch manifest job is gated off (`if: github.event_name != 'workflow_dispatch'`); the meaningful signal on a dispatch run is the `smoke-*` job statuses only. *Measurable:* a dispatch run shows the manifest job skipped and does not fail attempting to assemble a branch-named manifest from per-arch tags that were never pushed. (SR-06; nan-019 ADR-004.)

---

## Acceptance Criteria

> Per the #4796 lesson and `local-gates-linux-only-ci-is-crossplatform` memory: the release workflow runs on tag `v*` push, so the gate's **first real execution is post-merge**. CI-dependent behavior is phrased as "configured + verified locally; GH execution confirmed post-tag" — never asserted as executed fact before it has run on the hosted runner.

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| **AC-01** | On a tag `v*` push **and** on `workflow_dispatch` (NOT on `pull_request`), Docker-capable lanes run `docker-http-posture-smoke.sh` against the shipped image on **both** arches — `smoke-amd64` (`ubuntu-22.04`) and `smoke-arm64` (`ubuntu-22.04-arm`) — each executing reuse-pushed-bytes → boot-HTTP-on → register → restart → per-slug write → assert-landed. Both jobs live on the container branch. | **Config (local):** inspect `release.yml` — `on:` includes `push.tags: ['v*']` and `workflow_dispatch` and excludes `pull_request`; jobs `smoke-amd64`/`smoke-arm64` exist on the named runners and invoke the smoke. On a `v*` push each smokes its un-stripped per-arch tag `:v<version>-<arch>`; on `workflow_dispatch` each smokes `:latest-<arch>`. **Execution (post-tag):** confirm both jobs ran on the hosted runners in the first real release run (AC-07). |
| **AC-02** | A smoke **failure** on **either** arch (smoke `exit 1`: HTTP-off boot, per-slug route not `204`, or per-slug store assertion fails) fails the job and **blocks the container release** — the multi-arch manifest is not released. | **Config (local):** `create-container-manifest` has `needs: [..., smoke-amd64, smoke-arm64]`; no `continue-on-error`/retry on smoke jobs. **Behavioral (local):** run the smoke locally against a deliberately HTTP-off image and confirm `exit 1`. **Execution (post-tag):** a red smoke leaves the manifest step skipped. |
| **AC-03** | A smoke **self-skip** (`exit 3`, Docker absent) is a **hard job failure with a clear diagnostic**, never silent green. **Additionally** the gate asserts the positive run-marker `ALL GATES PASSED` was emitted, so a future early-exit-0 cannot masquerade as success. A green job provably means the smoke ran to its end and passed. (Closes #4796/#4970.) | **Config (local):** the gate step captures the exit code and branches on `0`/`1`/`3` (3 → fail with diagnostic), and greps captured output for `ALL GATES PASSED`. **Behavioral (local):** (i) run on a Docker-less shell → `exit 3` → job logic fails; (ii) simulate an early `exit 0` with no marker → job logic fails; (iii) full pass → exit 0 AND marker present → green. |
| **AC-04** | Wiring introduces **NO** `needs` coupling between the container/smoke branch and the binary/npm branch. `build-linux-*`, `package-npm`, and `create-release` are neither blocked by nor able to block the smoke. A smoke failure blocks only the container release path (the manifest). | **Config (local):** trace the `needs` graph in `release.yml` — confirm no smoke job is referenced by any binary/npm job and no binary/npm job is referenced by a smoke job; `create-release` still `needs: package-npm` only. (ADR-004 #4572.) |
| **AC-05** | The smoke additionally asserts the per-slug store **grew** as a result of the write **and** the hash store did **not** receive it — pinning to the literal #783 symptom. One bounded change via the existing busybox sidecar; no `docker exec` into distroless. | **Code review:** the assertion pair is in `docker-http-posture-smoke.sh`, uses `vol()`/busybox read-only inspection, captures pre/post per-slug DB size and confirms growth + hash-store non-receipt. **Behavioral (local):** run the smoke end-to-end → both assertions pass; the run still emits `ALL GATES PASSED`. |
| **AC-06** | The gate tests the **actual shipped artifact** — the GHCR-**pushed** per-arch tags. The resolved tag string is byte-identical to what `build-container-*` pushed: on a `v*` push the `v` is KEPT (`:v<version>-<arch>`, resolved UN-stripped from `${GITHUB_REF_NAME}`); on dispatch it is `:latest-<arch>`. This parity is provable PRE-MERGE, so a tag-string VALUE mismatch is caught at merge time, not at release time. **Self-build is rejected.** No duplicate full image build is introduced. | **Config (local):** smoke jobs `docker login` to GHCR and pass `IMAGE=ghcr.io/<owner>/unimatrix:<resolved-tag>-<arch>` where the push-path tag is resolved UN-stripped (`VERSION="${GITHUB_REF_NAME}"` ⇒ `:${VERSION}-<arch>`, never `${GITHUB_REF_NAME#v}`) and the dispatch-path tag is `:latest-<arch>`; no production `docker build` in the smoke jobs; `smoke-amd64 needs build-container-x64`, `smoke-arm64 needs build-container-arm64`. **Pre-merge parity (local, FR-11):** a static assertion in the existing gate-logic test surface proves the smoke's resolved tag string is byte-identical to the metadata-action push tag for both surfaces — `v1.2.3` ⇒ `:v1.2.3-<arch>` (matches `pattern=v{{version}}-<arch>`) and a branch ref ⇒ `:latest-<arch>` (matches `value=latest-<arch>`) — derived from the same expression or a tiny equality test; this fails RED at merge time on any value mismatch, with no tag push or post-tag run required. **Execution (post-tag):** smoke log shows "using prebuilt image: ghcr.io/...:v<version>-<arch>". |
| **AC-07** | The first real release run post-merge is watched to completion on the hosted runners (**both arches**) and the gate is confirmed to actually run green. CI-dependent assertions are "configured + verified locally; GH execution confirmed post-tag," not asserted before execution. | **Process (post-tag):** the delivery leader watches the first `v*` release run to completion; both `smoke-amd64` and `smoke-arm64` complete green; any platform/runner surprise is treated as in-scope rework. This AC is verifiable ONLY post-tag — it cannot be proven by local Linux validation. |
| **AC-08** | On `workflow_dispatch`, the multi-arch manifest job is **gated off** so only the `smoke-*` job statuses carry signal. The manifest job (which assembles `:${GITHUB_REF_NAME}` from per-arch tags the build does not push on a branch ref) does not run on dispatch and so cannot go falsely red. On a `v*` push the manifest job runs and stays smoke-gated (AC-02). | **Config (local):** `create-container-manifest` carries `if: github.event_name != 'workflow_dispatch'`; its `needs` still includes `smoke-amd64`/`smoke-arm64` for the push path. **Behavioral (reasoned, local):** on a dispatch ref the manifest job is skipped and the run's pass/fail reduces to the two smoke jobs; on a `v*` ref the manifest job runs gated by both smokes. **Execution (post-tag/dispatch):** a `workflow_dispatch` run shows the manifest job skipped and both smokes green. |

---

## User Workflows

- **Release cutter (human).** Pushes a `v*` tag (e.g. via the `uni-release` flow). The release workflow builds + pushes both per-arch images, then runs `smoke-amd64`/`smoke-arm64` against the pushed bytes. Only if both smokes pass does `create-container-manifest` assemble and push the multi-arch tag operators pull. A smoke failure leaves the manifest unreleased and surfaces a red job.
- **Pre-release dry-run (human).** Before cutting a release — e.g. after a release-config or Dockerfile change — the human triggers the workflow via `workflow_dispatch` to exercise the gate without tagging. On dispatch the build pushes `:latest-<arch>`, the smokes run against those bytes, and the multi-arch manifest job is gated off (`if: github.event_name != 'workflow_dispatch'`) — so the dispatch run's pass/fail is the two `smoke-*` statuses alone, no release object is created, and no false-red manifest assembly is attempted.
- **Delivery leader (post-tag).** Watches the first real release run to completion on both arches (AC-07), confirming the gate runs green on the hosted runners and treating any runner/platform surprise as in-scope rework.

---

## Constraints

- **C-01 — Verify-by-name, not by green suite (cross-cutting, load-bearing).** Exit-code keyed (`0`/`1`/`3`) AND run-marker (`ALL GATES PASSED`) asserted. Getting this wrong re-creates the exact false-green class the feature exists to kill. (SR-01.)
- **C-02 — Arch coverage named — no silent caps (HARD RULE).** Both arches smoked; any deferral is a named, tracked fast-follow with N5 PARTIAL. (SR-07.)
- **C-03 — Pushed bytes only — self-build forbidden.** Smoke runs against GHCR-pushed per-arch tags via `IMAGE=`, never a rebuild on the smoke runner. The resolved tag keeps the `v` on a `v*` push (`:v<version>-<arch>`, un-stripped) and is `:latest-<arch>` on dispatch — byte-identical to the build's pushed tag. (DECIDED OQ-2; SR-02.)
- **C-13 — Tag parity is pre-merge-provable.** A bounded static parity assertion (FR-11) proves the smoke's resolved tag equals the metadata-action push tag for both surfaces, RED at merge time on mismatch — no post-tag round-trip to discover a tag-string defect. (SR-01.)
- **C-14 — Dispatch manifest gating.** `create-container-manifest` carries `if: github.event_name != 'workflow_dispatch'`; on dispatch only the `smoke-*` statuses signal. (nan-019 ADR-004; SR-06.)
- **C-04 — No silent retry.** No `|| retry`, retry loop, or `continue-on-error` on the smoke. (DECIDED OQ-6; SR-04.)
- **C-05 — Gate the manifest, not the per-arch pushes.** The block lands on `create-container-manifest` (`needs: [..., smoke-amd64, smoke-arm64]`); the released multi-arch tag is never un-smoked. (Human-preferred OQ-1; architect owns exact mechanics.)
- **C-06 — ADR-004 container independence (#4572).** No `needs` edge between the container branch and the binary/npm branch; `create-release` stays independent of container/smoke jobs. (SR-08.)
- **C-07 — No duplicate image build / runner cost.** Reuse pushed bytes (`IMAGE=` + GHCR login); do not rebuild the slow ONNX-bearing image. (SR-02/SR-03.)
- **C-08 — Docker availability is the gate's whole point.** Do not tolerate the `exit 3` skip; a Docker-less lane is a hard failure. Do not depend on hosted runners always shipping Docker. (SR-01.)
- **C-09 — Distroless runtime / busybox sidecar.** The AC-05 change uses the read-only busybox sidecar; never `docker exec` into the shell-less runtime image. (SR-05.)
- **C-10 — No new secrets.** Only GHCR read via existing `GITHUB_TOKEN`. (SCOPE Non-Goals.)
- **C-11 — Release runs on tag push → first execution post-merge.** The gate cannot be proven by local Linux validation; budget a post-tag CI round-trip. (Lesson #4796; `local-gates-linux-only-ci-is-crossplatform` memory.)
- **C-12 — Test infrastructure is cumulative.** Wire and extend the existing `infra-001` smoke (AC-05 only); no parallel script; no smoke logic re-implemented in YAML.

---

## Dependencies

- **`product/test/infra-001/scripts/docker-http-posture-smoke.sh`** (from #786) — the smoke wired by this feature; the only sanctioned change is AC-05 (FR-05). Its exit contract (0/1/3) and run-marker (`ALL GATES PASSED`) are load-bearing.
- **`.github/workflows/release.yml` container branch** (`build-container-x64`, `build-container-arm64`, `create-container-manifest`, lines 327–428) — the host for the new smoke jobs and the gating `needs` edge. The per-arch tags it pushes are the artifacts under test: on a `v*` push `:v<version>-<arch>` (metadata-action `pattern=v{{version}}-<arch>`, `v` kept), on dispatch `:latest-<arch>` (`value=latest-<arch>`). The existing manifest job consumes the same un-stripped form (`VERSION="${GITHUB_REF_NAME}"` ⇒ `:${VERSION}-<arch>`) — the smoke's tag resolution must match it exactly.
- **GHCR + `secrets.GITHUB_TOKEN`** — for `docker login` to pull pushed per-arch images (already used by the container build jobs; `packages: write` already granted). No new secret.
- **Hosted runners `ubuntu-22.04` and `ubuntu-22.04-arm`** — ship Docker preinstalled; the gate must NOT assume this is eternal (the `exit 3` guard catches a mis-provisioned lane).
- **ADR-004 (#4572)** — container-lane independence; constrains where the `needs` edge may land.
- **Pattern #5180** — the verify-by-name / skip-is-failure / run-marker pattern this gate must implement.
- **Capability N5 (#5163)** — the capability this feature flips from PARTIAL ("proven once, not maintained") toward maintained.

---

## NOT in Scope

- **No new deploy behavior or container changes.** No edits to the Dockerfile, runtime posture, served routes, or first-boot behavior. If the gate reveals a real artifact defect, fixing it is a separate feature/bugfix.
- **Not the functional per-slug analytics work (crt-056 / #787 = C5).** Orthogonal; no dependency either direction.
- **Not a new smoke or a smoke rewrite.** The only script change is the bounded AC-05 grew-assertion. No new scenarios, no new scripts, no smoke logic duplicated in YAML.
- **Not a CI-lane (`ci.yml` / `pull_request`) gate.** The release workflow is the intended home; `pull_request` is explicitly excluded (DECIDED OQ-5).
- **No change to the binary/npm release jobs.** The gate is additive on the container branch only.
- **No new secrets.** GHCR read via existing `GITHUB_TOKEN` only.
- **No silent retry / no `continue-on-error` papering over flake.**

---

## Open Questions (for the architect)

- **OQ-1 (mechanics, human preference recorded).** Exact `needs` wiring to gate the release without violating ADR-004. Human-preferred resolution: gate `create-container-manifest` (`needs: [..., smoke-amd64, smoke-arm64]`). The architect owns final mechanics but must honor: (i) binary/npm branch stays uncoupled; (ii) no released multi-arch artifact is ever un-smoked.
- **OQ-A (architect).** Where to place the exit-code capture + run-marker grep — a thin wrapper step in the job vs. teaching the smoke to emit a machine-checkable success token. Constraint: the captured exit code must not be swallowed by `set -e`/pipefail or a YAML `if:` into a green result, and the run-marker check must operate on captured combined output.
- **OQ-B (architect / tester).** Whether the existing 90s boot-wait deadline in the smoke is sufficient for a cold arm64 first-boot ONNX model load on `ubuntu-22.04-arm`, or whether NFR-07 requires widening it (without masking a true hang). To be confirmed in the post-tag run (AC-07).
- **OQ-C (architect, AC-05 mechanics).** The precise pre/post measurement for "per-slug store grew, hash store did not" via the busybox sidecar — DB file size delta vs. a more specific signal — kept to one bounded assertion pair.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced N5 capability #5163 (PARTIAL, flipped by #788), the verify-by-name/skip-is-failure/run-marker pattern #5180 (the exact gate pattern, tagged nan-019), container ENV-posture lesson #5130 (#783 root cause), and the real-build-not-static-review lesson #4582. Retrieved #5180 and #5163 in full. Container-CI-independence ADR-004 (#4572) referenced from SCOPE/risk-assessment.
- Correction pass (this edit): re-queried briefing for the tag-resolution defect; surfaced nan-019 ADR-004 (#5184, the workflow_dispatch + per-arch tag-resolution ADR). Retrieved #5184 in full. **Conflict noted and flagged for the architect:** #5184 currently records `TAG=${GITHUB_REF_NAME#v}` (v stripped) on the push path. Verified against `.github/workflows/release.yml` ground truth (lines 348/383 push `pattern=v{{version}}-<arch>` → `:v1.2.3-<arch>`; lines 410/421 consume `:${GITHUB_REF_NAME}-<arch>` un-stripped) — the stored ADR's strip is the same defect being corrected. The spec is now aligned to the UN-stripped contract; the architect (correcting ARCHITECTURE/ADRs in parallel) should `context_correct` #5184 to the un-stripped form. Read-only tier; no storage performed by this spec agent.
