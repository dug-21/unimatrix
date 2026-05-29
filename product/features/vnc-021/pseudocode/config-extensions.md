# config-extensions (C7) -- `src/infra/config.rs` modifications

## Purpose

Extend `UnimatrixConfig` with `[http]` and `[tls]` TOML sections. These control HTTP listener activation, bind address/port, connection limits, and TLS certificate paths. The config follows existing two-level hierarchy (global + per-project, replace semantics).

## New Structs

### `HttpConfig`

```
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
struct HttpConfig:
    enabled: bool               // default: false
    content_port: u16           // default: 8443
    bind_address: String        // default: "0.0.0.0"
    max_concurrent_sessions: usize  // default: 32
    max_request_body_bytes: usize   // default: 1_048_576 (1 MB)
    connection_timeout_secs: u64    // default: 30

impl Default for HttpConfig:
    fn default() -> Self:
        HttpConfig {
            enabled: false,
            content_port: 8443,
            bind_address: "0.0.0.0".to_string(),
            max_concurrent_sessions: 32,
            max_request_body_bytes: 1_048_576,
            connection_timeout_secs: 30,
        }
```

### `TlsConfig`

```
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
struct TlsConfig:
    enabled: Option<bool>       // None = auto-detect from cert/key presence
    cert_path: Option<PathBuf>
    key_path: Option<PathBuf>

impl Default for TlsConfig:
    fn default() -> Self:
        TlsConfig {
            enabled: None,
            cert_path: None,
            key_path: None,
        }
```

### `TlsConfig::is_enabled()` method

```
impl TlsConfig:
    /// Resolve effective TLS enabled state.
    /// - explicit `enabled = true/false` takes precedence
    /// - absent `enabled`: true when BOTH cert_path and key_path are present
    fn is_enabled(&self) -> bool:
        match self.enabled:
            Some(v) => v,
            None => self.cert_path.is_some() && self.key_path.is_some(),
```

## UnimatrixConfig Modification

Add two new fields to the existing `UnimatrixConfig` struct:

```
struct UnimatrixConfig:
    // ... existing fields ...
    #[serde(default)]
    http: HttpConfig,
    #[serde(default)]
    tls: TlsConfig,
```

Both fields use `#[serde(default)]` so absent TOML sections produce compiled defaults. This means HTTP is disabled by default and TLS auto-detects.

## Validation

### `validate_http_config(config: &HttpConfig) -> Result<(), ServerError>`

Called during config loading, after TOML parse.

```
fn validate_http_config(config: &HttpConfig) -> Result<(), ServerError>:
    // Port 0 is valid (OS-assigned, for testing)
    // No port validation needed beyond u16 range (handled by serde)

    // Validate bind address is parseable
    if config.bind_address.parse::<IpAddr>().is_err():
        return Err(ServerError::Config(
            format!("invalid bind_address: {}", config.bind_address)
        ))

    // Validate max_concurrent_sessions > 0
    if config.max_concurrent_sessions == 0:
        return Err(ServerError::Config("max_concurrent_sessions must be > 0"))

    // Validate max_request_body_bytes > 0
    if config.max_request_body_bytes == 0:
        return Err(ServerError::Config("max_request_body_bytes must be > 0"))

    // Validate connection_timeout_secs > 0
    if config.connection_timeout_secs == 0:
        return Err(ServerError::Config("connection_timeout_secs must be > 0"))

    return Ok(())
```

### `validate_tls_config(config: &TlsConfig) -> Result<(), ServerError>`

```
fn validate_tls_config(config: &TlsConfig) -> Result<(), ServerError>:
    if config.is_enabled():
        // When TLS is enabled, BOTH cert and key must be present (FR-05)
        match (&config.cert_path, &config.key_path):
            (Some(_), Some(_)) => Ok(()),
            (None, _) => Err(ServerError::Config(
                "tls.enabled = true requires tls.cert_path"
            )),
            (_, None) => Err(ServerError::Config(
                "tls.enabled = true requires tls.key_path"
            )),
    else:
        // TLS disabled -- cert/key presence is irrelevant
        Ok(())
```

## Integration with Existing Config Loading

The existing `load_config` / `merge_configs` functions in `config.rs` already handle two-level hierarchy with replace semantics. The new `[http]` and `[tls]` sections are struct-level fields on `UnimatrixConfig`, so serde's `#[serde(default)]` handles absent sections. The per-project config replaces entire sections (not field-by-field merge within HttpConfig) -- this matches the existing replace semantics.

Add validation calls after config loading in the existing validation path:

```
// In the existing config validation function:
validate_http_config(&config.http)?;
validate_tls_config(&config.tls)?;
```

## Error Handling

| Error Case | Error Type | Caller Action |
|-----------|-----------|--------------|
| Invalid bind_address | `ServerError::Config` | Startup failure |
| max_concurrent_sessions = 0 | `ServerError::Config` | Startup failure |
| TLS enabled without cert_path | `ServerError::Config` | Startup failure |
| TLS enabled without key_path | `ServerError::Config` | Startup failure |
| Malformed TOML in [http]/[tls] | serde parse error | Startup failure (existing path) |

## Key Test Scenarios

1. **Default config**: No `[http]` section. Verify `enabled = false`, `content_port = 8443`, `bind_address = "0.0.0.0"`.
2. **Default TLS**: No `[tls]` section. Verify `is_enabled() = false`.
3. **TLS auto-detect**: `cert_path` and `key_path` both present, no `enabled` field. Verify `is_enabled() = true`.
4. **TLS auto-detect partial**: Only `cert_path` present. Verify `is_enabled() = false` (R-14).
5. **TLS explicit false**: `enabled = false` with cert+key present. Verify `is_enabled() = false`.
6. **TLS explicit true no cert**: `enabled = true`, no cert_path. Verify validation error (FR-05).
7. **Port 0**: Verify accepted (testing convenience, FR-25).
8. **Invalid bind_address**: `bind_address = "not-an-ip"`. Verify validation error.
9. **Per-project override**: Global `http.enabled = false`, project `http.enabled = true`. Verify project wins.
