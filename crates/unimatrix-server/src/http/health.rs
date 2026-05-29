//! Unauthenticated `/health` JSON handler.
//!
//! Returns server version and schema version as JSON for Docker HEALTHCHECK,
//! load balancer probes, and external monitors. Distinct from the CLI `health`
//! subcommand (UDS probe). No MCP framing, no authentication required.

use http::{Response, StatusCode};

use unimatrix_store::migration::CURRENT_SCHEMA_VERSION;

/// Constructs the health check response.
///
/// Returns HTTP 200 with JSON body:
/// ```json
/// {"version": "<semver>", "schema_version": <int>}
/// ```
///
/// All data is compile-time constants — no I/O, no database access, no async.
pub(crate) fn health_response() -> Response<String> {
    let version = env!("CARGO_PKG_VERSION");
    let schema_version = CURRENT_SCHEMA_VERSION;

    let body = format!(r#"{{"version":"{version}","schema_version":{schema_version}}}"#);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(body)
        .expect("static health response builder cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-HH-01: Response is 200 with valid JSON containing version and schema_version.
    #[test]
    fn test_health_returns_200_with_json_body() {
        let resp = health_response();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/json"
        );

        let body: serde_json::Value =
            serde_json::from_str(resp.body()).expect("body must be valid JSON");
        assert!(body.get("version").unwrap().is_string());
        assert!(body.get("schema_version").unwrap().is_u64());
    }

    /// T-HH-02: Version in response matches the crate version.
    #[test]
    fn test_health_version_matches_crate_version() {
        let resp = health_response();
        let body: serde_json::Value = serde_json::from_str(resp.body()).unwrap();

        assert_eq!(body["version"].as_str().unwrap(), env!("CARGO_PKG_VERSION"),);
    }

    /// Schema version in response matches the store migration constant.
    #[test]
    fn test_health_schema_version_matches_store_constant() {
        let resp = health_response();
        let body: serde_json::Value = serde_json::from_str(resp.body()).unwrap();

        assert_eq!(
            body["schema_version"].as_u64().unwrap(),
            CURRENT_SCHEMA_VERSION,
        );
    }
}
