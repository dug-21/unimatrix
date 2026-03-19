# Alignment Report: nan-007

> Reviewed: 2026-03-19
> Artifacts reviewed:
>   - product/features/nan-007/architecture/ARCHITECTURE.md
>   - product/features/nan-007/specification/SPECIFICATION.md
>   - product/features/nan-007/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | WARN | One security requirement in the vision (`--anonymize`) was explicitly removed from scope; source docs are consistent with each other but diverge from the vision text |
| Milestone Fit | PASS | W1-3 placement is correct; feature explicitly gates W1-4, W2-4, and W3-1 as specified in the vision |
| Scope Gaps | PASS | All six SCOPE.md deliverables are fully addressed in all three source documents |
| Scope Additions | PASS | No material additions beyond what SCOPE.md requests |
| Architecture Consistency | PASS | Architecture resolves all four SCOPE.md open questions; ADRs are coherent and internally consistent |
| Risk Completeness | PASS | RISK-TEST-STRATEGY.md covers all 9 SCOPE-RISK-ASSESSMENT risks plus 9 additional architectural risks; every critical and high risk has test scenarios |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `--anonymize` flag removed | In the product vision's W1-3 description (line 477: "`--anonymize` flag replaces `agent_id` and `session_id` with seeded consistent pseudonyms") but explicitly removed from SCOPE.md Resolved Decisions. Rationale given: "`agent_id` is role-like metadata, not personal identification data." Consistent across all three source documents. Requires human approval — vision text treats anonymization as a security requirement. |
| Simplification | `eval run` snapshot-path guard | Vision says "must refuse to accept the active daemon's DB file path" (security req, High). SCOPE.md AC-02 applies this only to `snapshot`, not `eval run`. Architecture and spec follow SCOPE.md. The gap is minor: `eval run` uses `?mode=ro` enforcement instead, but an explicit path check (matching the one on `snapshot`) is absent. |
| Addition | `AnalyticsMode::Suppressed` enum | Not named in SCOPE.md but introduced by SR-07 recommendation in SCOPE-RISK-ASSESSMENT.md. Architecture (ADR-002) formalises it. Consistent with the SR-07 directive; not a scope addition requiring separate approval. |
| Addition | `EvalError` structured error type | Not named in SCOPE.md. Architecture and spec both define it. Entirely in service of scope requirements (FR-18, FR-23, SR-08, SR-09). Not a scope addition requiring approval. |

---

## Variances Requiring Approval

### VARIANCE-01 — `--anonymize` flag removed from scope (WARN level)

1. **What**: The product vision's W1-3 section explicitly describes `--anonymize` as part of `unimatrix snapshot` ("The `--anonymize` flag replaces `agent_id` and `session_id` with seeded consistent pseudonyms, preserving co-access patterns while enabling snapshot fixtures to be committed to the repository"). It is listed as a `[High]` security requirement in the W1-3 security block: "`unimatrix snapshot` must apply `--anonymize` before any snapshot is committed to a repository. The non-anonymized snapshot contains real `agent_id` and `session_id` values from production sessions."

2. **Why it matters**: The vision treats anonymization as a security requirement needed to enable snapshot fixtures to be committed to the repo. Its absence means team members sharing snapshots in the repository expose production session metadata. All three source documents (architecture, specification, RISK-TEST-STRATEGY) are consistent in removing this from scope, but none of them explicitly acknowledge the gap against the vision security requirement — they only note that `agent_id` is "role-like metadata, not personal identification data." That characterisation is asserted rather than formally substantiated.

3. **Recommendation**: Human to decide one of: (a) accept the removal with an explicit note that snapshots must never be committed to the repository (and add this as a constraint in the spec / CLI warning), or (b) add anonymize back as a follow-on task. The source documents already include a partial mitigation: NFR-07 requires CLI `--help` text to warn that the snapshot contains all database content. This partially addresses the risk but does not prevent accidental commits. Classification: **WARN** — the design is internally consistent, but the divergence from the vision security requirement must be acknowledged by the human.

---

### VARIANCE-02 — `eval run` missing snapshot-path protection guard (WARN level)

1. **What**: The product vision's W1-3 security requirements state (High severity): "`eval run` must refuse to accept the active daemon's DB file path. Open snapshot DB with `?mode=ro`." The `?mode=ro` enforcement is fully implemented (FR-24, C-02, architecture ADR-001). However, the explicit path-equality guard (`canonicalize` both paths, reject if same inode) that is required for `snapshot` (AC-02, NFR-06) is not applied to `eval run`. There is no FR or AC in the specification that requires `eval run` to reject the active daemon's DB path explicitly.

2. **Why it matters**: `?mode=ro` at the SQLite layer prevents writes, so the live database will not be corrupted. The gap is that `eval run` against the live daemon DB could still affect the daemon's performance (read lock contention, WAL bloat) and, more importantly, would produce misleading eval results (the live DB is in flux, unlike a frozen snapshot). The vision explicitly says "refuse to accept the active daemon's DB file path" for `eval run`.

3. **Recommendation**: Add an FR (or add to the existing canonicalize constraint in NFR-06) requiring `eval run --db` to check the supplied path against the active daemon DB path and refuse if they match. This is a low-effort addition that closes the gap. Classification: **WARN** — the `?mode=ro` enforcement prevents data corruption, making this a correctness/UX concern rather than a security failure.

---

## Detailed Findings

### Vision Alignment

The source documents are well-aligned with the product vision's W1-3 description. The six deliverables map one-to-one to the vision's six items. The vision's stated gate conditions are explicitly reflected:

- SCOPE.md (line 14): "W1-4 (NLI cross-encoder) and W2-4 (GGUF) are both blocked on eval results as explicit gate conditions per the product vision."
- SPECIFICATION.md (FR-15, FR-16, FR-22): These requirements directly implement the vision's stated measurement contract.
- ARCHITECTURE.md (Integration Points table): D1–D4 gate W1-4/W2-4; D5–D6 gate W1-5/W3-1 — matches vision exactly.

The two WARNs (above) are the only deviations. Both are in the security block of the W1-3 vision section. Both are acknowledged by the source documents but not fully closed.

### Milestone Fit

nan-007 is correctly placed at W1-3. The vision's Wave 1 ordering is:
- W1-1: Typed Relationship Graph (COMPLETE)
- W1-2: Rayon Thread Pool (COMPLETE, crt-022)
- W1-3: Evaluation Harness (this feature)
- W1-4: NLI Re-ranking (gated on W1-3)

The architecture correctly positions the harness as infrastructure that unblocks W1-4 and W2-4. The feature does not implement any W1-4/W2-4 logic (`[inference]` stubs in profile TOML are gated stubs only). No future-milestone capability is shipped early.

### Architecture Review

The architecture is well-structured and internally consistent. Key findings:

- ADR-001 through ADR-005 each address a specific SCOPE.md open question. All four open questions are resolved before implementation.
- The module tree (`eval/mod.rs`, `profile.rs`, `scenarios.rs`, `runner.rs`, `report.rs`) correctly follows the single-binary principle (ADR-004). No new workspace crate.
- The `AnalyticsMode::Suppressed` design (ADR-002) directly addresses the critical SR-07 risk. The architecture is explicit that `?mode=ro` is a secondary layer, not the primary guard.
- The `test-support` feature gate approach (ADR-003) correctly resolves the SCOPE.md Assumption about `kendall_tau()` accessibility from production binary code.
- Open Question 3 (hook socket path) is resolved: `ProjectPaths.socket_path` is the hook IPC socket, `mcp_socket_path` is the MCP socket. No `ProjectPaths` struct change required. This closes SR-05.
- The SR-03 analysis (vector index per profile, not shared) is documented in the architecture with a memory limit caveat in CLI help text. NFR-03 sets the measurable threshold. The design decision is explicit.

One minor note: the architecture documents `AnalyticsMode::Suppressed` in ADR-002 and the component breakdown, while the specification's domain model uses `AnalyticsMode::Disabled` in the `EvalServiceLayer` struct definition (SPECIFICATION.md line 528). This naming inconsistency between architecture and specification could cause implementer confusion. It is not a functional variance — both documents clearly describe the same suppression behaviour — but the implementer will encounter two names for the same variant.

### Specification Review

The specification is complete. All six deliverables have corresponding functional requirements, non-functional requirements, acceptance criteria, and domain models.

Notable strengths:
- The Group 1 / Group 2 acceptance criteria split (SR-04 recommendation) is implemented: D1–D4 (offline) and D5–D6 (live daemon) are explicitly separated, with Group 2 clearly gated on the `daemon_server` fixture.
- FR-29 explicitly prohibits CI gate logic in `eval report` (SR-06 constraint), addressing the scope boundary risk.
- FR-18 and FR-23 provide structured error requirements that close SR-08 and SR-09.
- The ubiquitous language section (SPECIFICATION.md) aligns precisely with the architecture's domain model naming.

The `AnalyticsMode::Suppressed` vs `AnalyticsMode::Disabled` naming inconsistency noted above is the only intra-document variance. The spec's domain model uses `Disabled`; the architecture's ADR and code snippets use `Suppressed`. One of these should be chosen before implementation begins.

Open questions in the spec (OQ-1 through OQ-5) are all answered in the architecture. The spec's FR-41 ("hook socket path convention shall be confirmed by the architect") is satisfied by architecture Open Question Answer 3.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is thorough. Findings:

- All nine SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-09) are traceable in the Scope Risk Traceability table at the end of the document. Each has a corresponding architecture risk (R-01 through R-16).
- The critical risk (R-01, analytics suppression) has four dedicated test scenarios including SHA-256 snapshot integrity verification, a unit test on construction, a no-op assertion, and a WAL check.
- Security risks are addressed with dedicated test scenarios: R-06 (path canonicalization bypass) has three test cases including the symlink case, the relative path case, and the `canonicalize` failure case. This is appropriately thorough for a security-class test.
- R-12 (zero-regression OR semantics) and R-16 (array length mismatch) are directly informed by prior gate failure patterns (entries #1203, #1204, #2577) found in Unimatrix knowledge — the risk strategy queried Unimatrix before documenting these risks.
- Edge cases section covers: empty snapshot, single-entry scenarios, Kendall tau undefined for single-element list, profile TOML with only `[profile]` section, snapshot against live WAL-mode DB, missing `--out` parent directory, Unicode in query text, profile name collision, `--k 0` or negative K. This is a comprehensive set.

The one gap in risk coverage: neither the risk strategy nor the specification adds an FR or test scenario for the `eval run` snapshot-path guard described in VARIANCE-02. R-01 and AC-05 cover read-only enforcement via SHA-256 but do not include a test for the case where the caller accidentally passes the live daemon DB path to `eval run --db`.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found two entries (#2298, #2063). Entry #2298 (config key semantic divergence between TOML and vision example) is not applicable to nan-007. Entry #2063 (single-file vs split-file vision language, milestone discipline) is not applicable. No prior nan-007-domain patterns found.
- Stored: nothing novel to store — the `--anonymize` scope removal pattern (vision lists a security feature; scope removes it with a rationale assertion rather than formal risk acceptance) is feature-specific. If this pattern recurs across multiple features, it would warrant a stored pattern entry. Will revisit at retro.
