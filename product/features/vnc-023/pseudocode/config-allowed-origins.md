# Component: config-allowed-origins

## Purpose

Add `allowed_origins: Vec<String>` field to `HttpConfig` in `crates/unimatrix-server/src/infra/config.rs`. This enables operators to configure Origin header validation for CSRF defense-in-depth (ADR-002). Default is empty vec (no origin restriction -- backward compatible).

## Current Code (lines 1669-1696)

```rust
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    pub enabled: bool,
    pub content_port: u16,
    pub bind_address: String,
    pub max_concurrent_sessions: usize,
    pub max_request_body_bytes: usize,
    pub connection_timeout_secs: u64,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            enabled: false,
            content_port: 8443,
            bind_address: "0.0.0.0".to_string(),
            max_concurrent_sessions: 32,
            max_request_body_bytes: 1_048_576,
            connection_timeout_secs: 30,
        }
    }
}
```

## New/Modified Functions

### HttpConfig struct -- add field

```
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    pub enabled: bool,
    pub content_port: u16,
    pub bind_address: String,
    pub max_concurrent_sessions: usize,
    pub max_request_body_bytes: usize,
    pub connection_timeout_secs: u64,

    /// Allowed Origin headers for CSRF defense-in-depth.
    /// Empty vec = no origin restriction (backward-compatible default).
    /// Independent of allowed_hosts (Host header / DNS rebinding defense).
    /// Both checks apply independently when configured.
    pub allowed_origins: Vec<String>,
}
```

### Default impl -- add field default

```
impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            enabled: false,
            content_port: 8443,
            bind_address: "0.0.0.0".to_string(),
            max_concurrent_sessions: 32,
            max_request_body_bytes: 1_048_576,
            connection_timeout_secs: 30,
            allowed_origins: Vec::new(),
        }
    }
}
```

## Data Flow

- Input: `config.toml` `[http]` section, optionally containing `allowed_origins = [...]`
- Output: `HttpConfig.allowed_origins: Vec<String>`
- Consumed by: `main.rs` which passes it to `ProjectRouter::new()`

## Error Handling

- `#[serde(default)]` on the struct ensures missing field deserializes to `Vec::new()` (backward compatible)
- Invalid TOML types for `allowed_origins` (e.g., integer instead of array) produce a serde deserialization error at startup -- this is correct behavior, no special handling needed

## Key Test Scenarios

1. **Backward compatibility** (R-09, NFR-02): TOML string without `allowed_origins` key deserializes into HttpConfig with `allowed_origins == vec![]`
2. **Present field** (R-04): TOML string with `allowed_origins = ["https://example.com"]` deserializes correctly
3. **Empty array** (R-04): TOML string with `allowed_origins = []` deserializes to empty vec
4. **PartialEq** still derives correctly (existing tests may compare HttpConfig instances)
