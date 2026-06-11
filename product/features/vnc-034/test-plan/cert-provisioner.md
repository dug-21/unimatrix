# Test Plan — CertProvisioner (`load_or_generate_cert`)

> `crates/unimatrix-server/src/http/tls.rs`. Promotes the test-only `generate_self_signed` to production `load_or_generate_cert(data_dir, sans) -> Result<(cert, key), ServerError>`. **Lead risks: R-07 (idempotence), R-08 (production params), R-11 (fail-loud).**

## AC-IDs covered
AC-W1-S3 (idempotent + override), AC-W1-S8 (fail-loud), AC-W1-S9 (SAN from C3), partial AC-CT-C6 (`TlsConfig` seam intact).

---

## Unit tests (Rust)

### R-08 — production cert params (not test-helper defaults)
- `test_cert_san_set_includes_public_url_host_and_local_set` — given `sans = derive_public_url(...).sans`, assert generated cert SAN set == `["localhost","127.0.0.1","0.0.0.0", <public-url-host>]` (FR-A4). Assert no narrower/hardcoded test SAN remains.
- `test_cert_validity_is_production_period` — assert `not_after - not_before` equals the defined production validity window (a concrete constant, e.g. the chosen N days), NOT the rcgen default; assert `not_before <= now <= not_after` at generation.
- `test_cert_key_written_mode_0600` — after generation, `fs::metadata(key_path).permissions().mode() & 0o777 == 0o600`. (Unix-gated.)
- `test_cert_is_leaf_self_signed_no_chain` — generated artifact is a single self-signed leaf (the DER `fingerprint_leaf_der` will hash). No CA chain emitted.

### R-07 — idempotence (load, not regenerate)
- `test_load_or_generate_cert_first_call_generates` — empty `data_dir/tls/` → creates `cert.pem` + `key.pem`.
- `test_load_or_generate_cert_second_call_loads_byte_identical` — call twice on the same `data_dir`; assert returned cert DER + key bytes are **byte-identical** across calls and the files were not rewritten (mtime/bytes unchanged). This is the unit-level proof under AC-W1-S3.
- `test_operator_override_cert_honored` (FR-A3) — pre-place an operator cert+key in `data_dir/tls/`; call `load_or_generate_cert`; assert the operator artifact is returned unchanged, NOT overwritten or regenerated.
- `test_partial_write_first_boot_not_corrupt` (edge: `RISK-TEST-STRATEGY §Edge Cases`) — simulate cert present but key missing (or vice-versa) → assert a deterministic, non-corrupting outcome (regenerate-both or error loud), never a half-state that silently serves a mismatched pair.

### R-11 — fail-loud provisioning
- `test_unwritable_data_dir_returns_actionable_error` — `data_dir` read-only → returns `Err(ServerError::…)` whose message names the path and the UID-65532 writability requirement; assert **no panic, no `.unwrap()`** path taken (NFR-03, FR-A9). The error is recoverable into a non-zero exit by the caller.
- `test_unreadable_key_file_errors_loud` (edge) — key present but unreadable (mode `0000`) → loud actionable error, not a panic.

### R-10 / AC-CT-C6 — seam preservation (source/structural)
- `test_tls_config_enabled_seam_present` — `TlsConfig { enabled, cert_path, key_path }` + `is_enabled()` still exist and are reachable; `build_tls_acceptor` consumes the provisioned cert without TLS being hardcoded (the enterprise-proxy seam survives, NFR-08). Structural assertion, may be a doc-comment + compile reference.

---

## Integration tests (container — see OVERVIEW §4.2)
- **boot-twice idempotence (AC-W1-S3):** real container boot → capture `{cert,key,token}` bytes → restart → assert byte-identical; then mount override cert `:ro` → assert used, not overwritten.
- **unwritable `/data` (AC-W1-S8):** mount `/data` unwritable by UID 65532 → container exits non-zero with the actionable error; no panic in logs.
- **served-cert linkage (feeds AC-W1-S4):** the cert this component provisions is the exact leaf served on `:8443` — asserted in `bundle-codec`/`fingerprint-computer` end-to-end test.

## Edge cases (assigned here, not optional)
- Cert exactly at validity boundary (`not_after == now`) — define and assert behavior (still loads; rotation is the operator's job).
- Key file present but unreadable.
- First boot racing two container starts on one volume → no duplicate/corrupt credential (idempotent generate or one-writer-wins, never two distinct certs).

## Assertions are concrete
Not "cert is valid" — assert `permissions().mode() & 0o777 == 0o600`, assert the exact SAN vector, assert byte-equality of DER across restarts.
