# vnc-032 — Container HTTPS Serving Wiring + Network Security Posture

## Problem Statement

W2-2 (#658, shipped) added an HTTPS streamable-HTTP transport plus static bearer-token auth to the `unimatrix` server binary. But the nan-014 container packaging (#629) predates that merge and was never updated: `Dockerfile:137` runs `serve --foreground` (UDS-only, no network listener), `Dockerfile:134` and `docker-compose.yml:25` still carry "until W2-2" placeholder comments, and there is no `EXPOSE` / `ports:`. Result: `docker compose up` yields a **UDS-only instance with no reachable network endpoint** — a remote MCP client cannot connect.

This feature turns the service on: it wires the container to serve a reachable, TLS-terminated HTTPS endpoint, and it commits the OSS network security posture as the service is enabled (settled in the uni-zero session 2026-06-10, GH #726) rather than leaving it to omission. It is the server-side half of the personal-cloud option; #725 owns the client-side `init --remote`.

Affected: any operator deploying the personal cloud, and any MCP client (Claude Code, Codex CLI, Gemini CLI) needing to reach it over HTTPS.

## Goals

1. Container runs an HTTPS listener bound `0.0.0.0` inside the container, reachable on a published TLS port, driven by config/env — not UDS `--foreground`.
2. First boot auto-generates BOTH credentials with zero operator input: a 32-byte bearer token (already exists) AND a self-signed cert+key (MUST be built), both persisted to the data volume. Operator may override either with provided values.
3. A server-side `client-bundle` command emits `{base-url, token, cert-fingerprint}` as one blob for the client to ingest via `init --remote` (#725).
4. Client trust model is cert-fingerprint pinning: the bundle carries the SHA-256 fingerprint; the server can compute and expose it.
5. Dockerfile and docker-compose.yml reflect the new posture: `EXPOSE` the TLS port, wired `ports:`, refreshed comments, documented data-volume bind-mount UID handling.
6. Existing nan-014 hardening is preserved: non-root user (UID 65532), distroless runtime, `/shared` model volume `:ro` after download, ORT SHA-256 pinning.
7. Forward-compat (ass-060, do NOT pre-build #727): store access for the request path funnels through a single resolution point returning the one store today, and the route shape leaves room for an additive `/{slug}` segment.

## Non-Goals

- **No plaintext-to-client mode.** No `tls.enabled=false` path exposed in the OSS deployment posture; the published port is always TLS.
- **No proxy-terminated / K8s TLS termination in OSS.** This is enterprise-only, additive on the existing `TlsConfig` seam. Leave the seam (the binary already supports `tls` disabled internally); do not pre-build proxy support.
- **No `ProjectRouter` / slug router (#727).** Only the funnel point + route shape; multi-project routing is designed later against this validated single-project cloud.
- **No client-side work.** `init --remote`, bundle ingestion, and client-side pinning are #725.
- **No CA-trust / SAN-based hostname validation.** Fingerprint pinning is the OSS trust model; CA+SAN is the enterprise/proxy path.
- **No rate limiting, no secret-rotation tooling, no separate-auth-domain `/metrics`, no adversarial security testing (#628).** Deferred / enterprise.
- **No new Prometheus `/metrics` endpoint.** None exists today; this feature must not add an unauthenticated one. (The W2-2 knowledge entry mentions metrics aspirationally; the code has none.)

## Background Research

Grounded in the actual code (not the W2-2 prose). EXISTS vs MUST-BUILD:

### W2-2 server surface — what already EXISTS
- **HTTP listener wiring is already present** in `crates/unimatrix-server/src/main.rs:840-900`, gated on `config.http.enabled`. It loads/generates the token, builds the TLS acceptor, assembles the tower stack (`StaticTokenAuthLayer` → `PathRouter` → `ProjectRouter` → rmcp `StreamableHttpService`), and starts the listener. Reused by both `tokio_main_daemon` (used by `serve --foreground`) and stdio paths.
- **Bearer token auto-generation EXISTS**: `http/token.rs` `load_or_generate_token(data_dir)` — generate-first-then-load, 32 raw bytes → 64 hex chars, file `{data_dir}/token`, mode `0600`, atomic `create_new`. Auto-creates on first call.
- **TLS acceptor from PEM EXISTS**: `http/tls.rs` `build_tls_acceptor(&TlsConfig)`. `TlsConfig { enabled: Option<bool>, cert_path: Option<PathBuf>, key_path: Option<PathBuf> }` (`infra/config.rs`). `enabled=None` auto-detects from cert/key presence; loads PEM via rustls/ring; validates cert/key match; returns `None` (plain HTTP) when disabled.
- **Bind host is configurable, default `0.0.0.0`**: `HttpConfig.bind_address` default `"0.0.0.0"`, `content_port` default `8443`, `enabled` default `false`, plus `max_concurrent_sessions` (pre-TLS semaphore, =32), `max_request_body_bytes`, `connection_timeout_secs`, `allowed_origins`. `start_http_listener` binds `{bind_address}:{content_port}` (`http/listener.rs:53`).
- **Bearer auth middleware EXISTS** with constant-time compare: `http/auth.rs` `StaticTokenAuthLayer` uses `subtle::ConstantTimeEq`. `/health` (GET) is the sole auth-bypass path (exact match).
- **`rcgen = "0.13"` is already a dependency** of unimatrix-server — but used only in `#[cfg(test)]` in `http/tls.rs` (`generate_simple_self_signed`, and a richer IP-SAN form at line 327). Production `build_tls_acceptor` does NOT generate.

### What this feature MUST BUILD
- **First-boot self-signed cert generation.** Production `build_tls_acceptor` is load-only and errors if `cert_path`/`key_path` are absent. Add a load-or-generate path (mirror `load_or_generate_token`): if no operator cert provided, generate a self-signed cert+key with rcgen, persist to the data volume (e.g. `{data_dir}/tls/cert.pem` + `key.pem`, key mode 0600), and feed it into the acceptor. The IP-SAN test helper at `tls.rs:327` is a usable template (fingerprint-pinning means SAN content is not load-bearing for trust, but a sane SAN is still good practice).
- **Cert fingerprint exposure.** No fingerprint/SHA-256 code exists near TLS. Add SHA-256-over-DER fingerprint computation, surfaced for the bundle command.
- **`client-bundle` command.** Does not exist anywhere. New `unimatrix` subcommand (sync path, like `health`/`version`) that reads the persisted token + cert, computes the fingerprint, derives the base-url, and prints `{base-url, token, cert-fingerprint}` as one blob. Base-url derivation (port + host placeholder) is an open question (see below).
- **Container `serve` mode that enables HTTP.** Today `enabled` defaults `false` and no `[http]`/`[tls]` sections are in `DEFAULT_CONFIG_TOML`. The container must turn HTTP on — via env override, a first-boot config seam, or a dedicated container serve path. Decide the cleanest mechanism that keeps "operator invents nothing".
- **Dockerfile/compose changes**: `EXPOSE 8443`, compose `ports: ["8443:8443"]`, refreshed comments (`Dockerfile:134`, `docker-compose.yml:25`), data-volume bind-mount UID/GID documentation.
- **Forward-compat funnel point.** `ProjectRouter`/`McpAdapter` (`http/router.rs`) clones the server into the rmcp closure; there is no global store but also no explicit `resolve_store(request)` function, and `POST /observe` reaches the store via a separate `ObserveContext` built in main.rs. Consolidate to one resolution seam returning today's single store.

### Container artifacts (current state)
- `Dockerfile`: distroless `cc-debian12:nonroot`, UID **65532**, `/data` + `/shared` volumes chmod 0700 owned 65532, `HOME=/data`, `HEALTHCHECK` runs `unimatrix --project-dir /data health` (UDS connect, internal — keep), `CMD ["--project-dir","/data","serve","--foreground"]`, **no EXPOSE**.
- `docker-compose.yml`: `unimatrix-data:/data`, `unimatrix-shared:/shared`, **no ports**, stale "until W2-2" comment at line 25.

### Data-volume / path resolution (UID/bind-mount trap)
- `ensure_data_directory` (`crates/unimatrix-engine/src/project.rs:146`): `data_dir = {base}/{project_hash}` where `base = HOME/.unimatrix` and `project_hash = SHA-256(canonical project_root)[..16]`. In-container: `project_root=/data`, so `data_dir = /data/.unimatrix/{hash}`. `data_dir` is created `0700`. **Token and cert therefore persist under `/data/.unimatrix/{hash}/`** — already on the data volume.
- Bind-mount trap: a host bind-mounted `/data` is owned by the host UID, but the container runs as 65532 and creates `0700` dirs. If the host path isn't writable by 65532, startup fails. Must document the container UID (65532) and/or handle directory creation/chown so a host-mounted `/data` is writable.

### Knowledge grounding
- Product vision #4934 (`goal:personal-cloud`): in-container TLS only, auto-generate token+cert first boot, connection bundle, fingerprint pinning (rotation = re-bundle), proxy/K8s = enterprise additive seam. ass-060 = seven OSS invariants / enterprise-compat contract.
- W2-2 #4555 (deprecated, in-flight snapshot): rmcp streamable-HTTP, 32-byte token, ConstantTimeEq, rustls 0.23, `tls.enabled=false` for proxy. Mentions content 8443 / admin 8444 and Prometheus metrics — **admin port and /metrics are NOT in the current code**; treat the entry as aspirational where code disagrees.
- W2-3 #4556: `StaticTokenAuth` implements the `BearerValidator` enterprise seam; token is the sole authz credential; agent_id/slug are attribution/scoping metadata, not security boundaries.
- vnc-023 #4701: `allowed_origins` is an additive `HttpConfig` field threaded through `ProjectRouter::new` → `McpAdapter::new` to rmcp (CSRF defense). Already wired.
- nan-014 #4570: ORT SHA-256 supply-chain gate — preserve.

## Proposed Approach

1. **Cert provisioning**: add a `load_or_generate_cert` analogue beside `load_or_generate_token`; promote the test-only rcgen helper into a production path. `TlsConfig` stays the seam (`enabled`/`cert_path`/`key_path`); OSS default = auto-generated cert under `{data_dir}/tls/`; operator override = provided paths. This keeps the proxy-terminated/enterprise path as a config-seam variation with no re-architecture.
2. **Enable HTTP in the container** via the least-surprising mechanism that needs no operator action (env-driven `[http] enabled=true` + TLS auto-detect, or container-default config). Decide in spec.
3. **`client-bundle` subcommand**: sync, reads token + cert from `{data_dir}`, computes SHA-256 DER fingerprint, prints one blob.
4. **Container**: `EXPOSE 8443`; compose `ports: ["8443:8443"]`; keep `HEALTHCHECK` on internal UDS; document UID 65532 and host bind-mount writability; refresh stale comments.
5. **Forward-compat**: carve a single `resolve_store(request) -> Arc<Store>` seam in `ProjectRouter` returning the one store; keep route shape so `/v1/{slug}/tools/...` can be added with `/v1/tools/...` as default alias. Design note, not a router.

Rationale: the transport, token, auth, bind, and TLS-from-PEM are already shipped — the smallest correct delta is cert auto-generation + fingerprint + bundle + container wiring + the funnel seam, with the security posture committed as defaults rather than new mechanism.

## Acceptance Criteria

- AC-01: `docker compose up` (no operator config) yields a container serving HTTPS on the published TLS port, reachable from another container on the same user-defined network by service name. No plaintext port is published.
- AC-02: On first boot with no operator-provided credentials, the server auto-generates a 32-byte bearer token AND a self-signed cert+key, both persisted to the data volume; on restart they are loaded, not regenerated.
- AC-03: An operator may override the auto-generated token and/or cert+key with provided values (config/env/volume), and the server uses them instead of generating.
- AC-04: A server-side `client-bundle` command outputs a single blob containing base-url, bearer token, and cert SHA-256 fingerprint.
- AC-05: The cert fingerprint emitted by `client-bundle` equals the SHA-256 of the served leaf certificate's DER, so a pinning client validates the exact cert.
- AC-06: The bearer token is never written to logs, never baked into the image, never committed in compose, and is not stored in any database.
- AC-07: No unauthenticated endpoint is exposed on the published port other than what W2-2 already bypasses (`GET /health`); the container `HEALTHCHECK` uses the internal UDS, not the published port. No new unauthenticated `/metrics` is added.
- AC-08: The container preserves nan-014 hardening: runs as non-root UID 65532, distroless runtime, ORT SHA-256 pinning, `/shared` mountable `:ro` after model download.
- AC-09: The Dockerfile `EXPOSE`s the TLS port and docker-compose.yml publishes it; the stale "until W2-2" comments (`Dockerfile:134`, `docker-compose.yml:25`) are removed/refreshed.
- AC-10: A host bind-mounted data volume is writable by the container — the container UID (65532) and any required chown/permission step are documented or handled so startup succeeds on a host path.
- AC-11: HTTP request store access is funneled through a single resolution point returning today's single store; the route shape admits an additive `/{slug}` segment later without rewiring (verified by design review, not a working router).
- AC-12: A documented re-enrollment path exists: rotating the cert requires re-running `client-bundle` + (client-side) `init --remote` to re-pin.

## Constraints

- **Dependencies already present**: `rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9` in unimatrix-server — no new crates needed for cert gen/fingerprint.
- **No `unsafe`, no `.unwrap()` in non-test code, max 500 lines/file** (rust-workspace rules). The test-only rcgen helper must be refactored cleanly when promoted to production.
- **No secrets in any DB** (architecture principle); token/cert live as files on the data volume with 0600 key perms.
- **Distroless runtime has no shell** — any first-boot provisioning must be done by the Rust binary itself (entrypoint is the binary), not a shell entrypoint script.
- **`data_dir` is `/data/.unimatrix/{hash}`, not `/data`** — cert/token persistence and any docs must use the resolved path, and the project hash depends on `--project-dir /data`.
- **TLS must stay terminable internally** (the `tls.enabled` seam) for the enterprise proxy path — do not hardcode TLS so deeply that the seam disappears.
- **Sync vs async dispatch (C-10)**: `client-bundle`, like `health`/`version`, should be a pre-tokio sync subcommand.
- **rmcp `allowed_origins` / `allowed_hosts`** are independent CSRF/DNS-rebind layers already wired; published-port exposure should set sane defaults or document them.

## Open Questions

1. **Base-url derivation for `client-bundle`**: the server cannot know its externally reachable host/port (NAT, published-port remap, docker service name vs host). Should the bundle emit a placeholder host the operator edits, accept a `--base-url`/host flag, or read an env hint (e.g. `UNIMATRIX_PUBLIC_URL`)? The issue says "presents base-url" but doesn't resolve how the server learns it.
2. **HTTP-enable mechanism in the container**: env var flipping `http.enabled` + TLS auto-detect, a container-specific default config written on first boot, or a new serve flag? The cleanest "operator invents nothing" path needs a decision (and must not change non-container behavior where `enabled` defaults false).
3. **Cert persistence location/format**: confirm `{data_dir}/tls/{cert,key}.pem` vs flat `{data_dir}/cert.pem` + `key.pem`; confirm key file mode 0600 and that operator-provided certs can be mounted read-only.
4. **Bind-mount UID handling**: document-only (operator chowns to 65532) vs. binary-handled (attempt dir creation/chown, tolerate EPERM)? Distroless + non-root limits chown options at runtime.
5. **`allowed_origins`/`allowed_hosts` defaults on a published port**: leave empty (permissive) or ship a documented restrictive default? Affects DNS-rebinding/CSRF posture for a now-exposed endpoint.
6. **Fingerprint format in the bundle**: raw lowercase hex, colon-separated, or `sha256:`-prefixed? Must match whatever #725's client pinning expects — coordinate the wire format with #725.
7. **Cert SAN content**: fingerprint pinning makes SAN non-load-bearing for trust, but rustls/clients may still require a SAN to complete the handshake; confirm a default SAN (e.g. `localhost` + `0.0.0.0` IP, or the service name) that doesn't break pinning clients.

## Tracking

GitHub Issue: #726 (`feat(vnc-032): container HTTPS serving wiring + network security posture (W2-2 follow-through)`).
