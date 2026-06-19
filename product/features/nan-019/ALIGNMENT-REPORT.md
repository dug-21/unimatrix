# Alignment Report: nan-019

> Reviewed: 2026-06-19
> Artifacts reviewed:
>   - product/features/nan-019/architecture/ARCHITECTURE.md
>   - product/features/nan-019/specification/SPECIFICATION.md
>   - product/features/nan-019/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Strategic goal: #4946 "Individual developer-friendly deployment" (`personal-cloud`)
> Capabilities advanced/guarded: N5 (#5163), N3 (#5161)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Quality-guard for the `personal-cloud` goal; flips N5 from PARTIAL toward maintained, guards N3. No new product behavior — appropriate for an NFR-maintenance feature. |
| Milestone Fit | PASS | Targets the current shipped artifact only; no future-milestone capabilities pulled in. Explicitly orthogonal to crt-056/#787 (C5). |
| Scope Gaps | PASS | All seven ACs and five SCOPE goals are carried into the source docs. No SCOPE item left unaddressed. |
| Scope Additions | PASS | No additions beyond SCOPE. The two genuine refinements (tag resolution per trigger; WAL-robust grew-signal) are mechanics inside DECIDED OQs, not new scope. |
| Architecture Consistency | PASS | ARCHITECTURE, SPECIFICATION, and RISK-TEST-STRATEGY agree on job topology, exit-code/run-marker contract, manifest gating, and ADR-004 independence. |
| Risk Completeness | PASS | Load-bearing false-green class (SR-01) is the documented spine; WAL-monotonicity and arm64 cold-boot risks are surfaced. Full SR→R traceability present. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE AC-01..AC-07 maps to FR/NFR/AC in the spec and to an architecture component/edge. |
| Addition | (none) | No capability appears in source docs that SCOPE does not request. |
| Simplification | (none material) | No SCOPE item was dropped or downscoped. |
| Refinement (in-scope) | Tag resolution per trigger surface (`:<version>-<arch>` on push, `:latest-<arch>` on dispatch) | Mechanics under DECIDED OQ-5; architecture integration surface, not new scope. |
| Refinement (in-scope) | WAL-inclusive grew-signal (vs. naive main-DB-file delta) | AC-05 mechanics under OQ-C; risk strategy R-04 grounds it in ADR #329 (autocheckpoint). Tightens AC-05, does not expand it. |

## Variances Requiring Approval

None. No VARIANCE or FAIL findings. All six checks PASS.

## Detailed Findings

### Vision Alignment
SCOPE frames nan-019 as an NFR-maintenance feature advancing N5 and guarding N3 — both `capability` entries that `Advances → #4946` ("Individual developer-friendly deployment"). This is the correct lens: the feature adds **no** new product behavior (SPECIFICATION line 3; SCOPE Non-Goals line 29: "No new deploy behavior or container changes"), it makes an existing quality property — "the shipped artifact is always deployable as released" — *stay* true by wiring an already-built smoke into the release pipeline.

Mapping to architectural principles: the gate's whole purpose is to defend Principle-style guarantees the product already claims — specifically that a write for slug A can only ever land in A's store (N3, the integrity basis of the isolation model in goal #4946). The #783 symptom (write mis-routed to the hash store) is exactly the contamination the `personal-cloud` goal calls "catastrophic and unrollbackable." AC-05's grew/hash-unchanged assertion pins the gate to that literal symptom. This is vision-defending, not vision-expanding — the strongest alignment posture for an NFR feature.

The feature does **not** touch the self-learning, proactive-delivery, or domain-agnostic goals, and correctly does not claim to. Marking those N/A is proportional: this is release-pipeline infrastructure for one goal's deployability NFR, not a core retrieval feature.

### Milestone Fit
The feature is scoped strictly to the artifact that ships today (the GHCR multi-arch manifest built by the existing `build-container-*` jobs). It introduces no future-milestone capability:
- It is explicitly **not** the functional per-slug analytics work (crt-056/#787 = C5) — SCOPE Non-Goals line 30, SPECIFICATION line 132, both stating no dependency either direction.
- It wires the **existing** `infra-001` smoke (cumulative test infra), not a new or rewritten smoke (SCOPE line 31; NFR-08).
- Both-arch coverage is not over-build: arm64 is already built by `build-container-arm64`, so smoking it is an incremental job on an existing artifact, and the multi-arch manifest is what operators actually pull — leaving arm64's first-run path unvalidated would be a *gap* in N5, not discipline. The HARD RULE (NFR-06) forbids a silent "amd64-only = N5-proven" outcome, which keeps the milestone claim honest.

### Architecture Review
The architecture is a YAML job-topology change plus a shell exit-code/run-marker contract — correctly noting there are no crate boundaries to draw. Five ADRs map 1:1 onto the SCOPE DECIDED OQs (ADR-001→OQ-1, ADR-002→OQ-2, ADR-003→SR-01/AC-03, ADR-004→OQ-5, ADR-005→OQ-4/AC-05). The ADR-004 independence constraint (#4572) is honored structurally: the gate edge lands only on `create-container-manifest needs: [smoke-amd64, smoke-arm64]`, with no edge crossing into `build-linux-*`/`package-npm`/`create-release`. The pinned run-marker capture pattern (ARCHITECTURE lines 136-150) preserves the exit code through `set +e; RC=$?; set -e` and anchors the marker grep — directly addressing the load-bearing false-green class. The OQ-1 human preference (gate the manifest, keep binary/npm uncoupled) is honored exactly.

### Specification Review
Every SCOPE AC is carried forward: AC-01→FR-01/FR-02/FR-04, AC-02→FR-06, AC-03→FR-07/FR-08, AC-04→NFR-03, AC-05→FR-05, AC-06→FR-03, AC-07→AC-07 (post-tag). The verify-by-name / skip-is-failure / positive-run-marker triad — the feature's reason to exist — is specified as load-bearing (NFR-01, C-01) with measurable criteria (forced `exit 3` and forced early-`exit 0` both produce red). The pushed-bytes-not-rebuild constraint (C-03, NFR-05) and no-silent-retry constraint (C-04, FR-09) are both present and trace to DECIDED OQ-2/OQ-6. CI-dependent ACs are correctly phrased "configured + verified locally; GH execution confirmed post-tag" per lesson #4796 — no AC is asserted as executed fact before it runs on a hosted runner.

### Risk Strategy Review
The risk register makes the false-green class (SR-01 → R-01/R-02/R-03) the documented spine, with a required pre-merge truth table {0,1,3,early-0,unexpected} × {marker present/absent}. Two non-obvious risks are surfaced that strengthen alignment: R-04 grounds the AC-05 grew-signal in ADR #329 (WAL autocheckpoint means the main DB file is **not** monotone on a single small write) and requires a WAL-inclusive signal — preventing a flaky-but-un-retryable gate (OQ-6 forbids retry); R-05/R-06 cover arm64 cold-boot deadline and the ADR-004 needs-graph invariant. Full SR-01..SR-09 → R traceability is present (RISK-TEST-STRATEGY lines 211-221). Security review correctly notes the read-only busybox sidecar and that no new secret beyond `GITHUB_TOKEN` is introduced (matches NFR-04).

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns — surfaced #2298 (config-key semantic divergence from vision example), #3337 (architecture diagram diverges from spec), #3426 (formatter section-order regression). None applied: nan-019 is an NFR-maintenance/CI-topology feature with no config-vision-example surface and no spec-vs-diagram string divergence; the three source docs are mutually consistent.
- Stored: nothing novel to store — the recurring patterns this feature would teach (verify-by-name self-skip is #5180; exit-code-swallow false-green is #4873; WAL non-monotonicity is #329) are already captured. nan-019's variances are feature-specific (none found), so there is no generalizable misalignment pattern to record.
