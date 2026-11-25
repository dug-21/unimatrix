# vnc-050-agent-3-risk report
Output: product/features/vnc-050/RISK-TEST-STRATEGY.md — 19 risks (7 Critical, 7 High, 4 Medium, 1 Low); 12 behavioral outcomes mapped to entry-point scenarios; all SR-01..SR-19 traced.

## Knowledge Stewardship
- Queried: context_search for trust-boundary/auth/TLS risk lessons. Found #4970 (prove at the real boundary), #4965 (gate the token write on verify), #4416 (rejection test plus success companion), #4321 (allowlist at a trust boundary is fix-before-merge). These informed R-01, R-02 and R-08.
- Stored: nothing. The relevant patterns already exist as #4970 and #4416.

## Rework 1 (HUMAN-DECISIONS 2-5)
- RISK-TEST-STRATEGY.md revised: R-02 rewritten as capability ceiling (delivery step 0 = prove BearerPrincipal at tool, McpAdapter fallback; empty ceiling on missing principal; claimed-agent_id table test); R-04 ceiling-derived learning gate; R-05 tombstone/no label reuse; R-15 `http.legacy_token_enabled`/`UNIMATRIX_LEGACY_TOKEN_ENABLED`; R-17 observe 403 + warn-once (Critical); R-18 FR-20 conflict list; R-13 downgraded to High (audit forgery only); new R-20 residual agent_id gap (accepted); new R-21 backward-compat matrix.
- OQ-7: old client = published `@dug-21/unimatrix@0.11.3` npm tarball (pinned, integrity hash); fallback `git archive v0.11.3`.

## Knowledge Stewardship
- Queried: prior evidence #4970, #4965, #4416, #4321, #4453 retained.
- Stored: nothing novel to store -- revisions are feature-specific.
