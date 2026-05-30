# Scope Risk Assessment: vnc-023

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Cargo feature flags renamed or removed in rmcp 1.7 — 6 features (`server`, `client`, `transport-io`, `macros`, `transport-streamable-http-server`, `transport-streamable-http-server-session`) assumed stable but not verified against 1.7 manifest | High | Low | Verify all 6 features exist in rmcp 1.7.0 Cargo.toml BEFORE architecture — a renamed feature invalidates the scope estimate |
| SR-02 | rmcp 1.7 MSRV exceeds workspace MSRV 1.89 — rmcp 1.4+ requires Rust 1.92, but 1.5-1.7 may raise further; workspace MSRV bump ripples into CI and downstream consumers | Med | Low | Check rmcp 1.7.0 `rust-version` in Cargo.toml during design; if >1.89, scope expands to include CI config and MSRV bump |
| SR-03 | `ServerHandler` trait signature changed between 0.16 and 1.7 — `initialize` return type or method signatures may have shifted to `async fn`, breaking `std::future::ready()` pattern | High | Med | Architect should verify trait definition in rmcp 1.7 source; if changed, the fix is mechanical but must be scoped |
| SR-04 | `http` crate version mismatch — rmcp 1.7 may bump `http` dependency beyond `http = "1"`, breaking `http::request::Parts` extraction used for extension propagation | Med | Low | Check rmcp 1.7.0 `Cargo.toml` for `http` version; must match or be bumped in lockstep |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Bundled opportunistic enhancements (Opp 11 origin validation, Opp 20 Implementation description) expand review surface beyond a pure dependency upgrade | Low | Med | Keep enhancements strictly additive — no behavioral changes to existing config defaults; test independently from migration |
| SR-06 | `allowed_origins` config interaction with `allowed_hosts` is undefined in scope — independent checks vs. alternative checks affects config documentation and validation behavior | Med | Med | Architect must clarify interaction semantics from rmcp source before designing config wiring |
| SR-07 | `schemars` version drift — rmcp proc macros depend on schemars for JSON Schema generation; version conflict could cause compilation failure across workspace | Med | Low | Verify `schemars` version compatibility in rmcp 1.7 dependency tree during design |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | Extension propagation regression — `ResolvedIdentity` inserted by `StaticTokenAuth` must survive rmcp 1.7 internal processing to reach `RequestContext.extensions`; validated for 0.16 only | High | Med | Treat as mandatory integration test (AC-07); architect should design the validation approach explicitly |
| SR-09 | UDS `IntoTransport` blanket impl for `(OwnedReadHalf, OwnedWriteHalf)` tuple may not resolve in 1.7 — transitive `transport-async-rw` enablement assumed but unverified | Med | Low | Compile-verify UDS transport path early in implementation; failure here blocks a transport mode |
| SR-10 | Behavioral default changes (`keep_alive` 5min, `allowed_hosts` localhost-only) may affect non-standard deployment topologies (reverse proxy, container networking) | Med | Med | Architect should document expected deployment topologies and validate defaults against each |

## Assumptions

1. **~90% call sites unaffected** (SCOPE.md "Proposed Approach"): Based on ass-065 analysis of 0.16→1.4 range. Confirmed by pattern #4699. Assumption is well-evidenced but 1.5-1.7 changes were cataloged as non-breaking — verify no regressions in proc-macro output.
2. **~4 hour effort estimate** (SCOPE.md "Background Research"): Assumes no feature flag renames (SR-01), no MSRV bump (SR-02), no trait signature changes (SR-03). If any materialize, add 1-2 hours each.
3. **Exact version pin `=1.7.0` continues** (SCOPE.md "Constraints"): Correct for deliberate upgrade policy but means no automatic patch fixes. Acceptable given biweekly release cadence.
4. **`serve_client` test helper unchanged** (SCOPE.md "Open Questions"): Used in integration tests. If renamed/moved, test infrastructure needs updating.

## Design Recommendations

1. **(SR-01, SR-02, SR-04, SR-07)** Run a pre-architecture verification: `cargo add rmcp@=1.7.0 --dry-run` or inspect rmcp 1.7.0 Cargo.toml for feature list, MSRV, `http` version, and `schemars` version. Do this before committing to architecture.
2. **(SR-03)** Verify `ServerHandler` trait definition in rmcp 1.7 source. If `initialize` is now `async fn`, document the migration pattern in architecture.
3. **(SR-08)** Extension propagation is the highest-integration-risk item. Architect should specify an explicit integration test that validates the full chain: `StaticTokenAuth` → rmcp internals → `RequestContext.extensions.get::<Parts>()`.
4. **(SR-06)** Clarify `allowed_origins` vs `allowed_hosts` interaction from rmcp source before designing config struct additions.
