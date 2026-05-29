# health-handler (C6) -- `src/http/health.rs`

## Purpose

Unauthenticated HTTP health endpoint returning server version and schema version as JSON. Used by Docker HEALTHCHECK, load balancer probes, and external monitors. Distinct from CLI `health` subcommand (UDS probe).

## Functions

### `health_response() -> Response<Body>`

Constructs the health check response. No async needed -- all data is compile-time or startup-time constants.

```
fn health_response() -> Response<Body>:
    // Version from Cargo.toml via env! macro
    let version = env!("CARGO_PKG_VERSION")

    // Schema version: the current migration version (compile-time constant)
    // This is the same value used in the CLI health subcommand.
    // Import from unimatrix_store or define as a constant matching the current schema.
    let schema_version = unimatrix_store::SCHEMA_VERSION  // integer, e.g. 27

    let body = format!(
        r#"{{"version":"{version}","schema_version":{schema_version}}}"#
    )

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()  // static builder -- cannot fail
```

**Note on schema_version source**: The implementation agent must find the actual constant or function that provides the current schema migration version. Likely candidates:
- `unimatrix_store::SCHEMA_VERSION` (if defined as a constant)
- `SqlxStore::schema_version()` (if it's a method requiring a store reference)

If it requires a store reference (runtime query), the health handler must accept an `Arc<SqlxStore>` or the schema version must be resolved at startup and stored as a static. The architecture says "schema_version: <int>" which implies a known compile-time value, but verify.

**Fallback**: If schema_version requires runtime access and adding a store reference is undesirable for the health handler, use the migration version constant from the store crate. This is the more likely path given that the health handler should be minimal and not require database access.

## Error Handling

This function cannot fail. The response builder uses only static/compile-time values. No I/O, no database access, no allocation that can fail.

## Key Test Scenarios

1. **Response format**: Call `health_response()`. Verify status 200, content-type `application/json`, body matches `{"version":"<semver>","schema_version":<int>}`.
2. **Version matches crate**: Verify version in response matches `env!("CARGO_PKG_VERSION")`.
3. **Schema version is integer**: Verify schema_version is a bare integer (not quoted string) in JSON.
4. **No auth required**: Verified at the auth middleware layer (static-token-auth bypasses `/health`). Health handler itself has no auth logic.
