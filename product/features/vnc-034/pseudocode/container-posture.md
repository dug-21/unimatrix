# Container posture — Dockerfile / compose.yaml / config env-read

> `Dockerfile`, `compose.yaml` (nan-014), and `crates/unimatrix-server/src/infra/config.rs`. Realizes ADR-007 (`UNIMATRIX_HTTP_ENABLED`), FR-A1/A8/A9, NFR-12, R-12. This component is configuration + a single config env-read; the only Rust logic is the `UNIMATRIX_HTTP_ENABLED` override.

## Purpose

Flip the container into HTTPS-serving posture WITHOUT changing the global binary default (`http.enabled=false` stays clean for the local UDS install). The mechanism is the env var `UNIMATRIX_HTTP_ENABLED=true`, container-scoped, set alongside `UNIMATRIX_PUBLIC_URL` in `compose.yaml` — surface-consistent (ADR-007). The published port is TLS-only; nan-014 hardening is preserved.

## Config env-read (Rust — the only code logic, config.rs `load_config`)

Parallels the existing `UNIMATRIX_CONFIG` / `UNIMATRIX_PUBLIC_URL` env reads in `load_config` (config.rs:2806+). Read `UNIMATRIX_HTTP_ENABLED` as an env OVERRIDE of `HttpConfig.enabled`, applied AFTER the merge of file configs (highest priority, like the env-config step), so the boolean flips serving on without a config file.

```
const HTTP_ENABLED_VAR = "UNIMATRIX_HTTP_ENABLED";

// pure helper (testable without std::env::set_var, mirroring resolve_env_config_path):
fn resolve_http_enabled_override(env_value: Option<&str>) -> Option<bool>:
    match env_value.map(|s| s.trim().to_ascii_lowercase()):
        Some("true")  | Some("1")  => Some(true)
        Some("false") | Some("0")  => Some(false)
        Some("")  | None           => None           // unset/empty -> no override
        Some(_other)               => None           // unrecognized -> no override (log debug); never panic

// in load_config, after Step 3b merge, before validate:
http_override = resolve_http_enabled_override(std::env::var(HTTP_ENABLED_VAR).ok().as_deref())
if let Some(enabled) = http_override:
    merged.http.enabled = enabled                    // env wins over file + global + compiled default
```
Global compiled default for `http.enabled` stays `false` (ADR-007) — only the env flips it. TLS auto-detects the first-boot-provisioned cert (cert-provisioner.md); no separate TLS-enable env is added (TLS turns on with the provisioned cert + the existing `TlsConfig` seam, NFR-08).

## Dockerfile changes (nan-014 base — PRESERVE hardening)

```
# EXPOSE the TLS port ONLY — no plaintext port published (FR-A8, AC-W1-S2, R-12).
EXPOSE 8443
# PRESERVE (do not remove): non-root USER 65532, distroless base (no shell), ORT pinning,
#   --project-dir /data, HOME=/data (ADR-005). (NFR-12, AC-W1-S7).
# No shell entrypoint provisioning — first-boot cert/token gen is in the Rust binary
#   (cert-provisioner.md / token.rs), since distroless has no shell (FR-A9, constraint #4).
# Refresh comments to document the serving posture + bind-mount UID 65532 requirement.
```
Do NOT add a plaintext port, a shell entrypoint, or bake the token/cert into any image layer (NFR-06 — token never imaged; AC-W1-S5).

## compose.yaml changes

```
services:
  unimatrix:
    environment:
      UNIMATRIX_HTTP_ENABLED: "true"                 # ADR-007 — flips serving posture (container-scoped)
      UNIMATRIX_PUBLIC_URL:  "https://cloud.example:8443"   # C3 — base-url + allowed_hosts + cert SAN
    ports:
      - "8443:8443"                                  # TLS port ONLY (no plaintext mapping)
    volumes:
      - ./data:/data                                 # bind-mount; MUST be writable by UID 65532 (documented)
      - ./shared:/shared:ro                          # PRESERVE :ro hardening (nan-014)
    # PRESERVE: read_only rootfs / cap_drop / no-new-privileges from nan-014 if present.
```
Document in comments: `/data` must be writable by UID 65532 or first boot fails loud-and-actionable (FR-A9 / AC-W1-S8); set `UNIMATRIX_PUBLIC_URL` before running `client-bundle` or the bundle base-url is the `<EDIT-ME>` placeholder (C3 / FR-A5b).

## Data flow

- **Input:** env vars `UNIMATRIX_HTTP_ENABLED`, `UNIMATRIX_PUBLIC_URL` (compose).
- **Output:** `HttpConfig.enabled = true` at runtime (via config override); TLS listener bound `0.0.0.0:8443` with the provisioned cert; only port 8443 published.
- **Interacts with:** cert-provisioner.md (first-boot cert), public-url.md (`derive_public_url` reads `UNIMATRIX_PUBLIC_URL`), slug-router.md (listener wiring).

## Error handling

- `resolve_http_enabled_override` is total — unrecognized values yield `None` (no override, debug log), never panic.
- Unwritable `/data` is handled by cert-provisioner.md / first-boot (loud-and-actionable, no panic — R-11).
- No `.unwrap()` in the config read path.

## Key test scenarios (hints for tester)

- `UNIMATRIX_HTTP_ENABLED=true` -> `HttpConfig.enabled == true` even with global default false; `=false`/unset -> stays false (ADR-007).
- `resolve_http_enabled_override` parses `true`/`1`/`false`/`0`/unset/garbage correctly; pure (no `set_var`).
- Global binary default `http.enabled` remains `false` (local UDS unaffected — no local behavior change).
- Only TLS port published; plaintext connect fails (AC-W1-S2, R-12).
- nan-014 hardening intact: UID 65532, distroless, ORT pinning, `/shared :ro` (AC-W1-S7, NFR-12).
- Token never baked into an image layer (grep image layers) (AC-W1-S5, NFR-06).
- Sibling-container HTTPS request by service name succeeds with no operator config (AC-W1-S1).
```
