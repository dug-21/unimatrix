//! #783: assert the production Dockerfile bakes the cloud HTTP serving posture
//! into the RUNTIME stage.
//!
//! The cloud image must boot HTTP-serving so registered `[[projects]]` slugs are
//! routable. The enabling env (`UNIMATRIX_HTTP_ENABLED=true`, vnc-034 ADR-007)
//! previously lived only in `docker-compose.yml`, which the release does not ship
//! — a clean `docker run` of the GHCR image booted with the binary default
//! `http.enabled=false` and misrouted writes to the path-hash store (#783).
//!
//! This is a cheap static guard against the env being accidentally removed from
//! the image. The load-bearing runtime verification is the docker-build + boot
//! smoke (see `product/test/infra-001/scripts/docker-http-posture-smoke.sh`).

use std::path::PathBuf;

/// Repo-root `Dockerfile` resolved from this crate's manifest dir.
fn dockerfile_contents() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("Dockerfile");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read Dockerfile at {}: {e}", path.display()))
}

/// Split the Dockerfile at the runtime stage boundary so an assertion can scope
/// to the runtime stage only (NOT the builder stage).
fn runtime_stage(contents: &str) -> &str {
    let idx = contents
        .find("AS runtime")
        .expect("Dockerfile must declare an `AS runtime` stage");
    &contents[idx..]
}

#[test]
fn test_dockerfile_runtime_stage_enables_http() {
    let contents = dockerfile_contents();
    let runtime = runtime_stage(&contents);
    assert!(
        runtime.contains("UNIMATRIX_HTTP_ENABLED=true"),
        "runtime stage must bake `UNIMATRIX_HTTP_ENABLED=true` (#783, vnc-034 ADR-007); \
         a clean `docker run` must boot HTTP-serving so registered slugs are routable"
    );
}

#[test]
fn test_dockerfile_builder_stage_does_not_enable_http() {
    // The env is a container serving-posture concern; baking it into the builder
    // stage would be meaningless and could mask a missing runtime-stage entry.
    let contents = dockerfile_contents();
    let runtime_idx = contents
        .find("AS runtime")
        .expect("Dockerfile must declare an `AS runtime` stage");
    let pre_runtime = &contents[..runtime_idx];
    assert!(
        !pre_runtime.contains("UNIMATRIX_HTTP_ENABLED"),
        "UNIMATRIX_HTTP_ENABLED must live in the runtime stage only, not the builder"
    );
}
