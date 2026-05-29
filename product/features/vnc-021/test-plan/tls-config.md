# Test Plan: TLS Configuration (`src/http/tls.rs`)

Covers: C5 — rustls ServerConfig construction from PEM files
Risks: R-06 (TLS startup validation failure)

## Unit Tests

All tests target `build_tls_acceptor(config: &TlsConfig)`.

### T-TLS-01: test_valid_cert_and_key_returns_tls_acceptor
- **Risk**: R-06
- **Arrange**: Generate self-signed cert+key PEM files (via `rcgen` in dev-deps or pre-generated fixtures). Create TlsConfig with valid paths and `enabled = true`.
- **Act**: Call `build_tls_acceptor(&config)`
- **Assert**: Returns `Ok(TlsAcceptor)`.

### T-TLS-02: test_missing_cert_path_returns_error
- **Risk**: R-06
- **Arrange**: TlsConfig with `enabled = true`, `cert_path = None`, `key_path = Some(valid_path)`.
- **Act**: Call `build_tls_acceptor(&config)`
- **Assert**: Returns `Err` with descriptive error message mentioning missing certificate path.

### T-TLS-03: test_missing_key_path_returns_error
- **Risk**: R-06
- **Arrange**: TlsConfig with `enabled = true`, `cert_path = Some(valid_path)`, `key_path = None`.
- **Act**: Call `build_tls_acceptor(&config)`
- **Assert**: Returns `Err` with descriptive error mentioning missing key path.

### T-TLS-04: test_invalid_pem_cert_returns_error
- **Risk**: R-06
- **Arrange**: Write `"not a valid PEM"` to a temp file. TlsConfig with `enabled = true` and cert_path pointing to the invalid file.
- **Act**: Call `build_tls_acceptor(&config)`
- **Assert**: Returns `Err` with descriptive error mentioning invalid PEM or certificate.

### T-TLS-05: test_invalid_pem_key_returns_error
- **Risk**: R-06
- **Arrange**: Valid cert PEM, but key file contains `"not a valid PEM"`. TlsConfig with `enabled = true`.
- **Act**: Call `build_tls_acceptor(&config)`
- **Assert**: Returns `Err` with descriptive error mentioning invalid key.

### T-TLS-06: test_mismatched_cert_key_returns_error
- **Risk**: R-06
- **Arrange**: Generate two separate self-signed certs. Use cert from pair A and key from pair B.
- **Act**: Call `build_tls_acceptor(&config)`
- **Assert**: Returns `Err` (rustls rejects mismatched cert/key pair).

### T-TLS-07: test_nonexistent_cert_file_returns_error
- **Risk**: R-06
- **Arrange**: TlsConfig with `cert_path = Some("/nonexistent/cert.pem")`.
- **Act**: Call `build_tls_acceptor(&config)`
- **Assert**: Returns `Err` with IO error or descriptive message.

## Required Edge-Case Tests

### T-TLS-08: test_tls_disabled_returns_none
- **Arrange**: TlsConfig with `enabled = false`.
- **Act**: Caller checks `tls.enabled` before calling `build_tls_acceptor`. When disabled, no acceptor is built.
- **Assert**: The function is not called (or if called, behavior is defined). The listener binds plain HTTP.

### T-TLS-09: test_cert_with_ip_san
- **Arrange**: Generate self-signed cert with IP Subject Alternative Name (e.g., 127.0.0.1) instead of DNS name.
- **Act**: Call `build_tls_acceptor(&config)` with this cert.
- **Assert**: Returns `Ok(TlsAcceptor)`. rustls accepts IP SANs. (Personal cloud users may use IP addresses.)

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-09 | T-TLS-01 (unit); full TLS handshake tested in lifecycle-integration |
| AC-10 | T-TLS-08 (unit); plain HTTP connection tested in lifecycle-integration |
| AC-11 | T-TLS-02, T-TLS-03, T-TLS-04, T-TLS-05, T-TLS-06, T-TLS-07 |
