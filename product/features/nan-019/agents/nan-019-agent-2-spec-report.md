# Agent Report — nan-019-agent-2-spec

**Role:** Specification writer
**Produced:** `product/features/nan-019/specification/SPECIFICATION.md`

## Summary

Translated locked SCOPE.md + SCOPE-RISK-ASSESSMENT.md into a structured specification for wiring `docker-http-posture-smoke.sh` into `release.yml` as a standing verify-by-name release gate. All seven SCOPE acceptance criteria (AC-01..AC-07) carried through with explicit verification methods, and SR-01..SR-09 mapped into NFRs/constraints.

## Key Requirements

- **9 FRs**: per-arch smoke jobs (FR-01); trigger surface tag `v*` + `workflow_dispatch`, NOT `pull_request` (FR-02); pushed-bytes-via-`IMAGE=` ordered after per-arch push (FR-03); inherited smoke behavior (FR-04); AC-05 grew-assertion via busybox sidecar (FR-05); manifest gating `needs:[smoke-amd64,smoke-arm64]` (FR-06); exit-code keying 0/1/3 with skip-is-failure (FR-07); positive run-marker `ALL GATES PASSED` (FR-08); no silent retry (FR-09).
- **9 NFRs**: verify-by-name (load-bearing), skip-is-failure, ADR-004 independence, no new secrets, no duplicate build, arch-coverage-must-be-named, cold-boot timeout tolerance, cumulative test infra, briefly-public intermediates accepted.
- **AC-07 phrased post-tag-only**: "configured + verified locally; GH execution confirmed post-tag" — flagged as not provable by local Linux validation.

## Key Decisions / Interpretations

- Treated the verify-by-name guard (exit-code keying AND run-marker) as the spine — both checks required for green, per pattern #5180 and SR-01.
- Made AC-05 a required FR (not fast-follow), per DECIDED OQ-4.
- Recorded the briefly-public per-arch intermediates as accepted/documented (NFR-09) so it is not re-litigated downstream as a defect (SR-09).
- Left exact `needs` mechanics, exit-code-capture placement, arm64 boot timeout, and AC-05 measurement detail as architect/tester open questions (OQ-1, OQ-A, OQ-B, OQ-C).

## Open Questions for Architect/Tester

- OQ-A: where to place exit-code capture + run-marker grep without `set -e`/pipefail/`if:` swallowing the code.
- OQ-B: is the existing 90s boot deadline enough for cold arm64 ONNX first-boot on `ubuntu-22.04-arm`?
- OQ-C: precise pre/post measurement for the per-slug-grew / hash-not-receive assertion via busybox.

## Correction Pass (BLOCKING design defect — tag-resolution + AC-06 + dispatch-gating)

Verified against `.github/workflows/release.yml` ground truth:
- Build pushes `type=semver,pattern=v{{version}}-<arch>` (lines 348, 383) ⇒ pushed tag KEEPS the `v`: `:v1.2.3-<arch>`.
- Manifest consumer resolves `VERSION="${GITHUB_REF_NAME}"` (line 410), consumes `:${VERSION}-<arch>` un-stripped (lines 421-423).
- Dispatch (branch ref): only `type=raw,value=latest-<arch>` pushed ⇒ `:latest-<arch>`.

### Sections changed
- Domain model **Per-arch tag** (v kept; un-stripped; dispatch=latest).
- **FR-03** (resolve un-stripped, never `${GITHUB_REF_NAME#v}`).
- **FR-10 (new)** dispatch manifest gating; **FR-11 (new)** pre-merge tag-parity assertion.
- **AC-01** (resolved tag forms), **AC-06** (strengthened — below), **AC-08 (new)** dispatch gating.
- **NFR-09** (corrected forms), **NFR-10 (new)** parity pre-merge-provable, **NFR-11 (new)** dispatch gating.
- **C-03** (corrected), **C-13/C-14 (new)**.
- Dependencies entry (corrected tag forms + consumer-match), User Workflows dispatch entry.

### Corrected AC-06 verification method
Config (local): `IMAGE=...:<resolved-tag>-<arch>` with push-path tag UN-stripped (`VERSION="${GITHUB_REF_NAME}"` ⇒ `:${VERSION}-<arch>`, never `${GITHUB_REF_NAME#v}`), dispatch-path `:latest-<arch>`; no production `docker build`; correct `needs`. Pre-merge parity (local, FR-11): static assertion in the existing gate-logic test surface proves the smoke's resolved tag is byte-identical to the metadata-action push tag for both surfaces — `v1.2.3` ⇒ `:v1.2.3-<arch>`, branch ⇒ `:latest-<arch>` — derived from one expression or a tiny equality test, RED at merge on mismatch, no tag push required. Execution (post-tag): log shows `using prebuilt image: ghcr.io/...:v<version>-<arch>`.

### New pre-merge parity verification (FR-11 / NFR-10 / C-13)
Bounded static assertion in the existing local gate-logic test surface; not a new framework; provable pre-merge without pushing a tag. A re-introduced `${...#v}` strip turns it RED at merge.

### Dispatch-gating requirement (FR-10 / NFR-11 / AC-08 / C-14)
`create-container-manifest` carries `if: github.event_name != 'workflow_dispatch'`. On dispatch only `:latest-<arch>` is pushed and `${GITHUB_REF_NAME}` is a branch, so the manifest job would go falsely red assembling a never-pushed per-arch tag; skipped on dispatch, the only meaningful signal is the `smoke-*` statuses. On `v*` push the manifest job runs, smoke-gated (FR-06).

### Flag for architect (BLOCKING coordination)
Stored nan-019 ADR-004 (#5184) records the WRONG stripped contract (`TAG=${GITHUB_REF_NAME#v}`). Ground truth = un-stripped. The architect correcting ADRs in parallel must `context_correct` #5184 to the un-stripped push-path form so ARCHITECTURE/ADRs and SPECIFICATION agree. Spec is already aligned.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` -- surfaced N5 capability #5163 (PARTIAL; #788 flips it), verify-by-name/skip-is-failure/run-marker pattern #5180 (exact gate pattern, tagged nan-019), #783 container-ENV-posture lesson #5130, real-build-not-static-review lesson #4582. Retrieved #5180 + #5163 in full.
- Correction pass: re-queried briefing; surfaced nan-019 ADR-004 #5184 (workflow_dispatch + per-arch tag-resolution), retrieved in full. Found it records the WRONG stripped contract — flagged for architect `context_correct`. Read-only tier; no storage (spec decisions are feature-specific).
