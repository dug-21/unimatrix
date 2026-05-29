# Gate 3a Report: vnc-021

> Gate: 3a (Design Review)
> Date: 2026-05-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | WARN | Two signature discrepancies (tls acceptor return type, TlsConfig.enabled type) — pseudocode is more correct than source docs |
| Specification coverage | PASS | All 30 FRs, 10 NFRs, and 25 ACs traceable to pseudocode or test plans |
| Risk coverage | PASS | All 18 risks mapped to 101 test cases + 4 code review checkpoints |
| Interface consistency | WARN | build_tls_acceptor and TlsConfig.enabled differ between Architecture/IMPL-BRIEF and pseudocode; pseudocode is correct per FR-26 |
| Wave dependency ordering | PASS | 6 waves (0-5) match Critical Implementation Ordering; R-01 spike is Wave 0 prerequisite |
| R-01 spike addressed | PASS | Spike test pseudocode in path-router.md; Wave 0 must complete before Wave 2 |
| Knowledge stewardship | PASS | All 4 design-phase agents have compliant stewardship blocks |

## Detailed Findings

### 1. Architecture Alignment
**Status**: WARN
**Evidence**: All 8 components from the Component Map have both pseudocode and test plan files. Component boundaries match architecture decomposition (C1-C8). Wave ordering in pseudocode/OVERVIEW.md matches Critical Implementation Ordering from IMPLEMENTATION-BRIEF.md. All 6 ADRs (#4665-4670) are explicitly cited in pseudocode at their enforcement points:
- ADR-001: `subtle::ConstantTimeEq` in static-token-auth.md Step 3
- ADR-002: `AUTH_BYPASS_PATHS` exact-match in static-token-auth.md
- ADR-003: `McpAdapter` struct in path-router.md with fallback path
- ADR-004: Pre-TLS semaphore in http-listener.md accept_loop
- ADR-005: TLS bypass via `is_enabled()` in tls-config.md
- ADR-006: `CREDENTIAL_TYPE_STATIC_TOKEN` constant in static-token-auth.md

**Issue (WARN)**: Two function signature discrepancies between source documents and pseudocode:

1. `build_tls_acceptor` — Architecture Integration Surface table and IMPLEMENTATION-BRIEF both say `fn(config: &TlsConfig) -> Result<TlsAcceptor, ServerError>`. Pseudocode says `fn(config: &TlsConfig) -> Result<Option<TlsAcceptor>, ServerError>`. The pseudocode is correct: when TLS is disabled, the function returns `Ok(None)` rather than requiring the caller to not call it.

2. `TlsConfig.enabled` — IMPLEMENTATION-BRIEF Data Structures section shows `enabled: bool`. Pseudocode shows `enabled: Option<bool>` with an `is_enabled()` method implementing auto-detect logic per FR-26 ("Default for `tls.enabled`: `true` when both `cert_path` and `key_path` are present in config, `false` otherwise"). The `Option<bool>` is correct because it distinguishes "explicitly set" from "auto-detect from cert/key presence".

Both discrepancies favor the pseudocode. Implementation agents should follow the pseudocode signatures, not the Architecture/IMPLEMENTATION-BRIEF table.

### 2. Specification Coverage
**Status**: PASS
**Evidence**: Systematic trace of functional requirements to pseudocode:

| FR Range | Coverage |
|----------|----------|
| FR-01 to FR-07 (Transport) | http-listener.md, lifecycle-integration.md |
| FR-08 to FR-14 (Auth) | static-token-auth.md, token-manager.md |
| FR-15 to FR-17 (Audit) | static-token-auth.md (credential_type constant), lifecycle-integration.md (wiring gap flagged) |
| FR-18 to FR-21 (Path-Dispatching) | path-router.md |
| FR-22 to FR-24 (ProjectRouter) | path-router.md (ProjectRouter struct, single-project default, /observe registered) |
| FR-25 to FR-27 (Config) | config-extensions.md |
| FR-28 to FR-30 (Client Docs) | lifecycle-integration.md references docs/client-setup.md as file to create |

NFR-01 (1MB body limit) covered in path-router.md McpAdapter. NFR-03 (32 connections) covered in http-listener.md semaphore. NFR-07 (500 lines/file) noted in architecture with line estimates per module. No scope additions detected — pseudocode implements only what the specification requires.

The credential_type wiring gap (pseudocode open question #2) is correctly flagged as an implementation-time task, not a design gap. The pseudocode identifies where the constant is defined and where it must be consumed.

### 3. Risk Coverage
**Status**: PASS
**Evidence**: The test-plan/OVERVIEW.md Risk-to-Test Mapping table maps all 18 risks (R-01 through R-18) to specific test IDs across 8 component test plans. Total: 101 test cases + 4 code review checkpoints (CR-01 through CR-04 for R-02 timing side-channel).

Critical risks have the highest test density:
- R-01 (extension propagation): 3 tests (T-LI-01, T-LI-02, T-LI-03)
- R-03 (identity first activation): 4 tests (T-LI-04, T-LI-05, T-LI-06, T-LI-07)
- R-04 (connection flood): 3 tests (T-HL-05, T-HL-06, T-HL-07)
- R-07 (health bypass): 5 tests (T-HH-03 through T-HH-07)

Note: RISK-TEST-STRATEGY.md Coverage Summary says "17" risks but lists 18 (R-01 through R-18). The High priority row lists 7 risk IDs but labels the count as 6. This is a minor documentation typo in the risk strategy document — all risks have actual test coverage regardless of the count label.

### 4. Interface Consistency
**Status**: WARN
**Evidence**: Shared types in pseudocode/OVERVIEW.md (ResolvedIdentity, CallerId, UnimatrixConfig, LifecycleHandles, ServerError, UnimatrixServer) match per-component usage across all 8 pseudocode files. New types (HttpConfig, TlsConfig, BearerValidator, StaticTokenAuth, PathRouter, ProjectRouter, McpAdapter) are defined in their respective component files and referenced consistently.

Constants are defined once and referenced: CREDENTIAL_TYPE_STATIC_TOKEN in auth.rs, HEALTH_PATH in auth.rs, OBSERVE_PATH in router.rs, TOKEN_FILE_NAME/TOKEN_HEX_LEN/TOKEN_BYTE_LEN in token.rs.

Data flow between components is coherent: token_bytes from token-manager flows into StaticTokenAuth constructor; HttpConfig/TlsConfig from config-extensions flows into listener and tls-config; PathRouter composes StaticTokenAuth and health/observe handlers; lifecycle-integration wires everything through main.rs.

**Issue (WARN)**: Same two signature discrepancies noted in Check 1 above. These are between source documents (Architecture, IMPLEMENTATION-BRIEF) and pseudocode, not between pseudocode files themselves. Pseudocode internal consistency is clean.

### 5. Wave Dependency Ordering
**Status**: PASS
**Evidence**: Pseudocode/OVERVIEW.md defines 6 waves matching the Critical Implementation Ordering:
- Wave 0: R-01 spike test (in path-router) — no dependencies
- Wave 1: token-manager, config-extensions — no dependencies, foundation
- Wave 2: static-token-auth (depends on token-manager), tls-config (depends on config-extensions), health-handler (no deps)
- Wave 3: path-router — depends on spike result + auth + health
- Wave 4: http-listener — depends on tls-config + path-router + config-extensions
- Wave 5: lifecycle-integration — depends on all above

The constraint "Wave 0 MUST complete before Wave 2" is explicitly stated. This matches IMPLEMENTATION-BRIEF: "Spike: rmcp extension propagation (R-01) -- validate ... before proceeding."

### 6. R-01 Spike Test Addressed
**Status**: PASS
**Evidence**: path-router.md contains a full spike test pseudocode (`spike_rmcp_extension_propagation`) that:
1. Builds a minimal UnimatrixServer via make_server() fixture
2. Wraps it in StreamableHttpService
3. Inserts ResolvedIdentity into request extensions
4. Calls the service and verifies identity survives
5. Documents the two outcome paths: primary (extensions propagate) vs ADR-003 fallback (task-local injection)

The spike outcome determines McpAdapter behavior: if extensions propagate, the copy step is a debug assertion; if dropped, the adapter uses a task-local or side-channel.

### 7. Open Questions Assessment
**Status**: PASS (none are scope-blocking)

**Pseudocode agent open questions (5):**
1. schema_version source — implementation-time discovery, two fallback paths documented
2. credential_type wiring gap — flagged with specific audit path to trace, not a design gap
3. rmcp StreamableHttpService constructor — implementation-time API verification, well-scoped
4. hyper Body type alignment — type conversion noted in McpAdapter, standard adapter work
5. CallerId::HttpBearer construction site — specific location to update identified, compile-time enforcement

**Test plan agent open questions (4):**
1. Uppercase hex in token file — behavior definition question, not scope-blocking
2. Query params on /health — behavior definition question, test T-HH-08 explicitly asks agent to define and document
3. GET on MCP path — behavior test T-PR-12 is included, asks agent to verify and document
4. TLS test fixture strategy — implementation choice (rcgen vs pre-generated PEM), not blocking

All 9 questions are addressable during implementation without scope changes.

### 8. Knowledge Stewardship Compliance
**Status**: PASS

| Agent | Role | Queried | Stored | Status |
|-------|------|---------|--------|--------|
| architect | active-storage | briefing: 17 entries, key inputs listed | 6 ADRs (#4665-4670) | PASS |
| risk-strategist | active-storage | 4 searches with specific results | "nothing novel -- first HTTP transport feature" | PASS |
| pseudocode | read-only | briefing: ADRs, patterns #319, #4661, #4362, #4368 | (not required for read-only) | PASS |
| test-plan | read-only | briefing: ADRs, lesson #3386, pattern #729 | "nothing novel -- followed established patterns" | PASS |

## Risk Coverage Traceability Matrix

| Risk ID | Priority | Test Plan | Test IDs | Scenarios Covered |
|---------|----------|-----------|----------|-------------------|
| R-01 | Critical | lifecycle-integration | T-LI-01, T-LI-02, T-LI-03 | Extension propagation, audit proof, fallback |
| R-02 | High | static-token-auth | T-STA-05, T-STA-06, T-STA-07, CR-01 to CR-04 | Malformed hex, short token, identical responses, code review |
| R-03 | Critical | lifecycle-integration | T-LI-04, T-LI-05, T-LI-06, T-LI-07 | Identity chain, UDS comparison, capabilities, agent_id |
| R-04 | Critical | http-listener | T-HL-05, T-HL-06, T-HL-07 | Limit enforced, release on close, UDS not starved |
| R-05 | High | token-manager | T-TM-01, T-TM-02, T-TM-03, T-TM-04 | File length, permissions, raw bytes, load existing |
| R-06 | Medium | tls-config | T-TLS-01 to T-TLS-07 | Valid, missing, invalid, mismatch, nonexistent |
| R-07 | Critical | health-handler | T-HH-03 to T-HH-07 | Exact match, trailing slash, prefix, subpath, POST |
| R-08 | High | lifecycle-integration | T-LI-08, T-LI-09, T-LI-10 | In-flight completion, new conn reject, ordering |
| R-09 | High | http-listener | T-HL-08, T-HL-09, T-HL-10 | TLS failure recovery, malformed HTTP, sequential |
| R-10 | High | lifecycle-integration | T-LI-04, T-LI-05 | HTTP vs UDS credential_type proof |
| R-11 | High | path-router | T-PR-06, T-PR-07, T-PR-08 | Oversized, boundary, pre-rmcp enforcement |
| R-12 | Low | path-router | T-PR-04, T-PR-05 | Auth required before 501 |
| R-13 | Medium | path-router | T-PR-09, T-PR-10, T-PR-11 | Wrap verification, default mode, /observe registered |
| R-14 | Medium | config-extensions | T-CE-01 to T-CE-04 | All default permutations |
| R-15 | Medium | token-manager | T-TM-05 to T-TM-08 | Newline, odd length, non-hex, exact 64 |
| R-16 | Low | lifecycle-integration | T-LI-11 | Stdio mode exclusion |
| R-17 | Medium | lifecycle-integration | T-LI-12 | Rate limit not exempt |
| R-18 | High | http-listener | T-HL-11, T-HL-12, T-HL-13 | Idle timeout, partial request, active not timed out |

## Notes for Implementation Agents

1. **Follow pseudocode signatures over Architecture/IMPLEMENTATION-BRIEF** for `build_tls_acceptor` (returns `Result<Option<TlsAcceptor>>`) and `TlsConfig.enabled` (use `Option<bool>` with `is_enabled()` method).

2. **credential_type wiring** is the most significant implementation-time task not fully resolved at pseudocode level. The pseudocode correctly identifies the gap and the constant to use. The implementation agent must trace `AuditContext` -> `AuditEvent` -> `audit_log INSERT` to wire `"static_token"` for external identity paths.

3. **R-01 spike test MUST pass before Wave 2 begins.** If it fails, the ADR-003 adapter fallback path in McpAdapter must be activated.
