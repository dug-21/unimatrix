# Test Plan — First-Boot Token Redaction (`token.rs`)

> Component: `crates/unimatrix-server/src/http/token.rs:101` · Surface: first-boot/token-surface tests + `tests/client_bundle_e2e.rs` · Risks: R-14 (High) · AC-11 (#735 CI-1, NFR-06)

## Scope
On the HTTP/cloud first-boot surface the bearer token is NOT printed to stdout or written via `tracing`; the `token.rs:101` print is redacted/gated. The `v:2` bundle is the SOLE token channel. Redaction is deployment-context-gated so the local STDIO/UDS token affordance is unaffected (R-14 ∩ R-13).

## Unit Test Expectations

### First-boot token-surface (R-14 sc.1 — the concrete verification)
- `test_first_boot_stdout_no_token_substring` — on the HTTP/cloud first-boot path, capture stdout AND `tracing` output; assert the token's hex substring appears NOWHERE in either. (Use a known synthetic 64-hex token; assert-absent.)
- `test_token_print_site_redacted` — assert the `token.rs:101` emission is gated/redacted (does not reach an unconditional stdout/`tracing` print on the cloud surface).

### Sole-channel (R-14 sc.2)
- `test_bundle_carries_token` — assert the emitted `v:2` bundle DOES carry the token (the token IS delivered — just only via the bundle). Pairs with bundle-codec-rust.md `{..., token, fp}`.
- `test_no_parallel_token_print_path` — assert no parallel "also print it" path survives beside the bundle channel.

### Local-surface non-regression (R-14 sc.3 — reconcile with ADR-006 / R-13)
- `test_redaction_gated_by_deployment_context` — assert the redaction is scoped to the HTTP/cloud first-boot context; if `token.rs:101` is shared with the local STDIO/UDS path, it is GATED by deployment context, NOT unconditionally removed (a naive removal would regress the local token affordance — cross-check local-binding-guard.md / AC-10).
- `test_local_token_affordance_unchanged` — assert the local path's token handling is functionally unchanged.

## Security (credential-exposure surface)
- Untrusted observer = anyone with access to aggregated/persisted container logs. Damage = bearer-token capture → full access to all served projects. The mitigation under test: the token never reaches stdout or `tracing` on the cloud surface; the bundle is the sole delivery channel.

## Integration (infra-001)
- Gap #6: `test_first_boot_token_not_in_logs` — capture first-boot stdout/stderr from the spawned binary; assert no token substring; assert the bundle carries it (R-14/AC-11).

## Coverage Requirement
The first-boot token never reaches stdout/logs on the cloud surface; the bundle is the sole channel; the redaction is deployment-context-gated so local token handling is functionally unchanged (no AC-10 regression).
