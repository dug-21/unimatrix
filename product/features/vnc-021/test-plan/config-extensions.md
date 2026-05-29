# Test Plan: Config Extensions (`src/infra/config.rs` modifications)

Covers: C7 — HttpConfig and TlsConfig parsing with defaults
Risks: R-14 (config defaults incorrect)

## Unit Tests

All tests parse TOML strings into `UnimatrixConfig` and assert HttpConfig/TlsConfig field values.

### T-CE-01: test_empty_config_http_defaults
- **Risk**: R-14
- **Arrange**: TOML string with no `[http]` section
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `http.enabled == false`, `http.content_port == 8443`, `http.bind_address == "0.0.0.0"`, `http.max_concurrent_sessions == 32`, `http.max_request_body_bytes == 1_048_576`, `http.connection_timeout_secs == 30`

### T-CE-02: test_empty_config_tls_defaults
- **Risk**: R-14
- **Arrange**: TOML string with no `[tls]` section
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `tls.enabled == false`, `tls.cert_path == None`, `tls.key_path == None`

### T-CE-03: test_tls_enabled_true_when_both_paths_present
- **Risk**: R-14
- **Arrange**: TOML with `[tls]` section containing `cert_path = "/tmp/cert.pem"` and `key_path = "/tmp/key.pem"` but no explicit `enabled`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `tls.enabled == true` (auto-detected from both paths present)

### T-CE-04: test_tls_enabled_false_when_only_cert_path
- **Risk**: R-14
- **Arrange**: TOML with `[tls]` section containing only `cert_path = "/tmp/cert.pem"` (no key_path)
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `tls.enabled == false` (key_path missing prevents auto-enable)

### T-CE-05: test_http_enabled_explicit_true
- **Arrange**: TOML with `[http]` section containing `enabled = true`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `http.enabled == true`

### T-CE-06: test_http_custom_port
- **Arrange**: TOML with `[http]` containing `content_port = 9443`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `http.content_port == 9443`

### T-CE-07: test_http_port_zero_allowed
- **Arrange**: TOML with `[http]` containing `content_port = 0`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `http.content_port == 0` (OS-assigned port for testing)

### T-CE-08: test_tls_explicit_enabled_false_overrides_auto_detect
- **Arrange**: TOML with `[tls]` containing `enabled = false`, `cert_path = "/tmp/cert.pem"`, `key_path = "/tmp/key.pem"`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `tls.enabled == false` (explicit setting overrides auto-detect)

### T-CE-09: test_http_custom_bind_address
- **Arrange**: TOML with `[http]` containing `bind_address = "127.0.0.1"`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `http.bind_address == "127.0.0.1"`

### T-CE-10: test_http_custom_max_connections
- **Arrange**: TOML with `[http]` containing `max_concurrent_sessions = 64`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: `http.max_concurrent_sessions == 64`

## Required Edge-Case Tests

### T-CE-11: test_existing_config_sections_unchanged
- **Arrange**: TOML with existing sections (`[knowledge]`, `[confidence]`, etc.) plus new `[http]` section
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: All existing config values are unchanged; new HTTP values are correct

### T-CE-12: test_unknown_http_fields_ignored_or_error
- **Arrange**: TOML with `[http]` containing an unknown field `foo = "bar"`
- **Act**: Parse into `UnimatrixConfig`
- **Assert**: Either parse succeeds (unknown fields ignored via `deny_unknown_fields` absent) or fails with clear error (if `deny_unknown_fields` is set). Behavior must match existing config sections.

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-20 | T-CE-01, T-CE-02, T-CE-03, T-CE-04, T-CE-05, T-CE-08 |
