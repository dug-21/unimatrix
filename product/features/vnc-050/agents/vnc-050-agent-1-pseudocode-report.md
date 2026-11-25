# vnc-050-agent-1-pseudocode Report

## Files
- pseudocode/OVERVIEW.md (components, shared types, data flow, waves, backward-compat matrix FR-34..39)
- pseudocode/token-table.md (C1), scoped-validator.md (C2), slug-guard.md (C3), principal.md (C4), cap-ceiling.md (C4b, incl. R-02 step 0), legacy-token-config.md (C5), observe-gate.md (C6), observe-403-client.md (C6c), client-bundle-cli.md (C7), bundle-decoder.md (C8), remote-resolve.md (C9), bridge-transports.md (C10), docs-smoke.md (C11)

## Wave Ordering
- W0 (gate): C4 types, C1 read path, C2 validator + extension insert, `ceiling_from` + cfg(test) probe, R-02 step 0 real-HTTP test. If it fails, apply the ADR-003 fallback in `McpAdapter::handle` and re-run.
- W1: rest of C4b, C3, C5, C6, C1 writers.
- W2: C7 (Rust); C8 and C9 (JS, can run in parallel with W1).
- W3: C10, C6c.
- W4: C11 and the compat harness.

## Open Questions / Gaps (flagged in files)
1. `BearerValidator` cannot return the principal. Resolution: add a default trait method `validate_principal`, and a builder `with_legacy_enabled` (needed for the disabled-legacy 401 body).
2. main.rs needs about 3 boot-wiring lines (`TokenTable::open`, the validator, `StaticTokenAuthLayer::with_validator`) beyond the variant swap.
3. `AccessAudit` trait is undefined in the architecture. Defined in slug-guard.md (`ResolverAccessAudit` over `StoreResolver`).
4. Observe latch: ADR-004 says "per process", but the hook runs one process per event, and RTS R-17.2/AC-18 need one warning across N events. Proposed: a per-credential marker file in stateDir. Needs architect acknowledgement.
5. Hook exit code: FR-20/23 say "exit non-zero", which conflicts with the hook exit-0 contract (C-05) and FR-24a. Proposed: the bridge exits 1; the hook exits 0 and prints one stderr line naming the variable.
6. The bridge argv `projectHash` is made optional when `UNIMATRIX_BUNDLE` or `_MCP_URL` is set (needed for FR-21).
7. `UNIMATRIX_LEGACY_TOKEN_ENABLED`: exact case-sensitive match. `TRUE` is rejected. New `ConfigError::InvalidLegacyTokenOverride`.
8. The `token:` reserved-prefix guard is placed in `build_context_with_external_identity`. UDS external-input sites need a grep check (R-13).
9. `list_for` returns a `Vec` and cannot report corruption. Added `list_for_checked`.
10. The bridge sends no SSE GET today. Headers cover every request it actually sends.
11. Resolver tuples gain `tier`, and the observe tuple gains `timeouts`, to keep the config.js source labels and the stored timeouts.
12. `init.js` MCP registration may key on a truthy pin. A CA bundle must register the bridge.
13. Unpinned HTTPS in transport-http now defers the body until `secureConnect` plus the `authorized` check. Behavior change, strictly safer.

## Gate 3a Rework (iteration 1)
- P1: OVERVIEW.md §Test-Only Seams defines all seams as `#[cfg(test)]`, zero production change: PROBE (cap_ceiling.rs), DISPATCH_SPY (http/router/handlers.rs), RESOLVE_SPY (http/router/project_resolver.rs), StripPrincipalLayer (http/auth/test_layers.rs), UsageService enqueued/in_flight counters + `flush()` (services/usage.rs), `lookup_ct_counted` scan counter (http/token_table.rs), AFTER_TMP_CREATE hook (http/token_table.rs, found in test-plan/token-table.md). Cross-referenced in observe-gate, slug-guard, scoped-validator, token-table, cap-ceiling.
- P2: cap-ceiling.md lists Admin-gated tools: `context_quarantine`, `context_enroll`.
- P3: remote-resolve.md: header values limited to HTAB + 0x20-0x7E; all else `bad_headers` (Node sends latin1 silently, throws on >0xFF).
- Observe latch marker design unchanged (pending human decision). Note: usage recording is spawn-based, not a channel; "enqueue spy" = `enqueued` counter.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing. Found the vnc-050 ADRs (#5814, #5815, #5818, #5817), #4665 (constant-time token compare), #4702 (extension propagation risk), #189 (fs2 flock), #5609 (Write gate as the trust seam).
- Deviations from established patterns: C5 parser rejects unrecognized values instead of the `http.enabled` ignore-on-garbage behavior (per ADR-001). The observe latch persists across processes (open question 4).
