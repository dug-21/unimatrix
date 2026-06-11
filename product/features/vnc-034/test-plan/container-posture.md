# Test Plan — Container Posture (Dockerfile / compose / env)

> `Dockerfile`, `compose.yaml` (nan-014 base). `EXPOSE 8443` / `ports` (TLS only), `UNIMATRIX_HTTP_ENABLED=true` + `UNIMATRIX_PUBLIC_URL` env (ADR-007, C3), bind-mount UID-65532 docs; nan-014 hardening preserved. **Lead risks: R-11 (fail-loud), R-12 (hard invariants).**

## AC-IDs covered
AC-W1-S1 (sibling-container HTTPS), AC-W1-S2 (no plaintext port), AC-W1-S6 (only `/health` unauth, no `/metrics`), AC-W1-S7 (nan-014 hardening preserved), AC-W1-S8 (fail-loud `/data`), and the env-override path for `UNIMATRIX_HTTP_ENABLED`.

---

## Config / static tests

### Env-driven HTTP enable (ADR-007)
- `test_http_enabled_env_override` (Rust, `config.rs`) — `UNIMATRIX_HTTP_ENABLED=true` overrides `HttpConfig.enabled`; global binary default stays `false` (local-UDS unaffected). Parallels the existing `UNIMATRIX_PUBLIC_URL` read in `load_config`.
- `test_http_enabled_default_false_without_env` — env unset → `http.enabled` remains `false` (no accidental HTTP exposure for the local install).
- `test_compose_sets_http_enabled_and_public_url` (file-check) — `compose.yaml` sets both `UNIMATRIX_HTTP_ENABLED=true` and `UNIMATRIX_PUBLIC_URL` together.

### Published-port invariant (R-12, AC-W1-S2)
- `test_only_tls_port_published` (shell) — `docker compose config` / runtime port inspection: assert ONLY the TLS port (`8443`) is published; no plaintext port. Plaintext connect to it fails.

### nan-014 hardening preserved (AC-W1-S7)
- `test_image_uid_65532` (shell) — runtime user is UID 65532 (non-root).
- `test_image_distroless_no_shell` — base is distroless; no shell present (first-boot provisioning is in the Rust binary).
- `test_ort_pinning_preserved` — ONNX Runtime pin unchanged.
- `test_shared_mount_readonly` — `/shared` mounted `:ro`.
- `test_expose_8443` (file-check) — Dockerfile `EXPOSE 8443`.

---

## Integration tests (docker compose — see OVERVIEW §4.2)

### AC-W1-S1 — sibling-container HTTPS reachability (NEW integration test)
- `test_sibling_container_https_health` — compose up the server container with no operator config; from a sibling container, `GET https://<service-name>:8443/health` succeeds over TLS (service-name reachability, the operator's real topology).

### AC-W1-S6 — endpoint surface
- `test_only_health_unauthenticated` — unauth probe over HTTPS: only `GET /health` answers; all tool endpoints require bearer; assert `/metrics` is absent (no new metrics endpoint — deferred to #732).

### AC-W1-S8 — fail-loud `/data` (also in cert-provisioner)
- `test_unwritable_data_volume_fails_loud` — bind-mount `/data` not writable by UID 65532 → container exits non-zero with an actionable error naming the path + UID requirement; no panic, no `.unwrap()` in logs.

## Edge cases (assigned here)
- Bind-mounted `/data` writable check at first boot (documented UID-65532 requirement).
- First boot racing two container starts on one shared volume → no corrupt/duplicate credentials (shared with cert-provisioner R-07).

## Concrete assertions
Port test asserts the published set equals exactly `{8443/tcp}` (or the configured TLS port) — not "8443 is present", because the invariant is that NO plaintext port leaks.
