# Alignment Report: vnc-050

> Reviewed: 2026-11-25
> Artifacts reviewed:
>   - product/features/vnc-050/architecture/ARCHITECTURE.md
>   - product/features/vnc-050/specification/SPECIFICATION.md
>   - product/features/vnc-050/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Also: SCOPE.md, SCOPE-RISK-ASSESSMENT.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances `personal-cloud`, and `integrity` through the read-token learning-poisoning guard. Principle 3 (capability checks at the service layer) is honored. |
| Milestone Fit | PASS | Stays on the v:2 bundle and the existing seams. JWT/OIDC, a token UI and multi-slug minting are deferred. |
| Scope Gaps | VARIANCE | Read-bundle observe behavior is undecided (Spec OQ-3 / R-17). |
| Scope Additions | WARN | `--trust pin\|ca` flag, `UNIMATRIX_LEGACY_TOKEN` env var, per-request `http_access` audit row, header size/count limits |
| Architecture Consistency | VARIANCE | Spec and architecture contradict each other on 3 rules (revoke semantics, bundle+env conflict, legacy-disable mechanism). |
| Risk Completeness | PASS | Every SR-01..SR-19 is traced. Boundary-level tests cover the critical risks. |

Counts: PASS 3, WARN 1, VARIANCE 2, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | Read-token observe handling | SCOPE OQ-6 denies SessionWrite to read tokens, but no document defines what the hook client does when a read bundle's observe is denied (skip, or send and swallow the 403). Spec OQ-3 is open, and RTS R-17 is blocked on it. |
| Gap (minor) | vnc-039 ADR-005 supersession | SCOPE says this feature relaxes ADR-005 (#5119). The spec delegates the supersession to the architect, but ARCHITECTURE.md lists ADR-001..007 and none of them names the correction to #5119. |
| Addition | `--trust pin\|ca` on `client-bundle` | Resolves Spec OQ-2. It is an extension of the existing command, so SR-05 is not violated. Scope OQ-7 says trust "follows the server cert", which reads as automatic. The flag makes it an explicit operator choice. |
| Addition | `UNIMATRIX_LEGACY_TOKEN=disabled` env var | Scope OQ-5 asked for an "operator config option". The architecture uses an env var to avoid touching `config.rs`, which is already over cap. |
| Addition | `http_access` audit row on every HTTP request | Goes beyond AC-11 ("every bearer call"), which only asks for a label. Volume on long SSE sessions is open (Arch OQ-3). |
| Addition | Header limits (8 KiB, 32 entries) | Appear only in RTS R-08.2, not in the spec FRs. Harmless hardening, but the spec should own them. |
| Simplification | One slug per mint | The schema allows `allowed_slugs[]` (AC-07). The CLI binds one slug per mint. Rationale: SCOPE goal 5 says "bound to allowed_slugs", and the CLI is per-slug. Acceptable. |
| Simplification | AC-13 live and AC-14 operator-run | A scripted local-CA equivalent runs in CI. SCOPE Constraints acknowledge the operator dependency. Acceptable. |

## Variances Requiring Approval

1. **Read-bundle observe behavior undefined (Spec OQ-3 / RTS R-17)**
   - What: A read token gets 403 on observe, but nothing defines the client response. Options: (a) the bundle declares its mode and the observe path skips sending, or (b) the client sends, swallows the 403 and warns once on stderr.
   - Why it matters: The scope entry point says "observe events land" for `UNIMATRIX_BUNDLE`. If the 403 fails the job, read-only CI breaks. If the 403 is dropped silently, telemetry loss is invisible (SR-15). Either outcome undermines AC-13.
   - Recommendation: Human decides before synthesis. Suggested answer: (b). Send, take the 403, log one stderr warning and never fail the job. This needs no change to the bundle format (keeps v:2 keys unchanged, ADR-005).

2. **Spec and architecture contradict each other (RTS R-18)**
   - What:
     - (a) FR-05/FR-06 say revoke "marks revoked" and `--list` shows "revoked state". The architecture's `tokens.json` schema has no revoked field, and revoke rewrites the file, which reads as a delete.
     - (b) FR-20/FR-24 make only bundle + env *pair* a conflict, and allow a bundle fingerprint and env fingerprint that are equal. ADR-006 makes a bundle plus *any* env var a conflict.
     - (c) FR-17 says "operator config option". The architecture uses an env var.
   - Why it matters: User intent is authoritative, but delivery cannot test two different rules. The gate will fail on whichever rule it doesn't implement.
   - Recommendation: Accept the architecture's rules: (a) delete-on-revoke, with `--list` dropping the revoked column, (b) the stricter any-env conflict, and (c) the env var, which counts as operator config. The synthesizer amends FR-05, FR-06, FR-17, FR-20 and FR-24 to match, and adds one test per rule.

## Detailed Findings

### Vision Alignment
- `personal-cloud`: The feature closes the gap where non-interactive clients could not attach (SCOPE Problem Statement). It uses one secret (`UNIMATRIX_BUNDLE`) and CA TLS, which fits "one container, one bearer token, one command".
- `integrity`: FR-15 and ARCH C6 gate usage, co-access and query_log writes on the read identity ("SLN1 poisoning guard"). The guard is placed before enqueue (RTS Integration Risks).
- Principle 3: Read-mode denials go through the existing `require_cap` and the C5 overlay, with no parallel checks (FR-14, #4453).
- Principle 8: Only SHA-256 hashes are stored, in `tokens.json` under the data dir and not in either database (NFR-04).
- `trusted-identity` (emerging): `token:<label>` is a non-self-assertable principal, and the reserved namespace is enforced on every transport (C5, R-13). This is a partial advance and does not conflict with the goal.
- `platform`: The `BearerValidator` seam is reused, not bypassed (C2). PASS.

### Milestone Fit
Out-of-scope items are explicitly excluded: OAuth, JWT, expiry, a token UI and a plain-HTTP listener (Spec "NOT in Scope"). The architecture's Modularity section confirms there is no mid-delivery refactor of over-cap files. PASS.

### Architecture Review
- The human constraint (SR-05) holds: `--headless` is a flag on `client-bundle`, there is one decoder (C8 only accepts `fp:null`), and there is no new command or API.
- Live revocation (SR-03) is resolved through a per-request stat stamp and a shared `Arc<TokenTable>` used by both C2 and C5.
- Arch OQ-4 is an unresolved feasibility risk: rmcp may not propagate `Parts.extensions`. If it doesn't, scoped identity falls back to a self-asserted id. The architecture names a fallback (re-insert in `McpAdapter`), and RTS R-02.3 requires an early proof. WARN, to be tracked into pseudocode.
- See Variance 2 for the inconsistencies with the spec.

### Specification Review
- FR-01..FR-33 cover AC-01..AC-16, including the added AC-15 and AC-16.
- The FR-07 flag matrix and the FR-29 header validation match the scope's fail-loud intent.
- FR-03 and FR-04 leave the choices to the architect. The architecture resolved them (accept `init --bundle` storing `fingerprint:null`, and `--trust`), but the spec was not updated to match.

### Risk Strategy Review
- There are 19 risks. The 7 critical risks are tested at the real socket and HTTP boundary (#4970). Each critical risk has a success companion (#4416).
- The RTS states that seam-level tests do not discharge the behavioral-outcome rows, which is correct.
- R-17 and R-18 are blocked on the variances above.
- The scope-risk traceability table covers every SR item.

## Knowledge Stewardship
- Queried: none run in this review, so no prior vision patterns were applied.
- Stored: nothing. The variances are specific to this feature. Spec-vs-ADR drift after the architect resolves the spec's open questions may recur, but one occurrence is not enough to store as a pattern.
