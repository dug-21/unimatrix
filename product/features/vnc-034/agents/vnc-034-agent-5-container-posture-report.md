# Agent Report — vnc-034-agent-5-container-posture

**Component:** Container posture (Wave 1) — Dockerfile/compose network posture, `UNIMATRIX_HTTP_ENABLED` env override (ADR-007), cert-rotation runbook (FR-A11).

## Files modified / created
- `crates/unimatrix-server/src/infra/config.rs` — added `HTTP_ENABLED_VAR` const, pure `resolve_http_enabled_override(Option<&str>) -> Option<bool>` helper, `resolve_http_enabled_override_for_test` wrapper, Step 3c override application in `load_config` (after merge, before validate), and 7 unit tests.
- `Dockerfile` — `EXPOSE 8443` (TLS port only, no plaintext); refreshed comments documenting serving posture + ADR-007 env gating + bind-mount UID-65532 requirement; all nan-014 hardening preserved.
- `docker-compose.yml` (the repo's compose file; brief said `compose.yaml`) — added `environment:` with `UNIMATRIX_HTTP_ENABLED: "true"` + `UNIMATRIX_PUBLIC_URL`; `ports: ["8443:8443"]` TLS-only; `/shared:/shared:ro` hard-set (nan-014); bind-mount UID-65532 docs; header reference to the runbook.
- `docs/cert-rotation.md` *(new)* — required operator runbook (rotate cert -> re-run `client-bundle` -> re-`init` clients), with the diagnosable stale-fingerprint mismatch example.

## Tests / validation
- `cargo build -p unimatrix-server --lib` — clean (0 errors).
- `cargo test -p unimatrix-server --lib http_enabled` — **8 passed, 0 failed** (7 new + 1 pre-existing unrelated TOML-parse test `test_http_enabled_explicit_true`).
- `cargo test -p unimatrix-server --lib config` — **498 passed, 0 failed** (no regressions).
- `cargo clippy -p unimatrix-server --lib` — no warnings on the added code.
- `cargo fmt -p unimatrix-server` — applied (only reformatted my additions).
- `docker compose -f docker-compose.yml config` — validates: both env vars present, only `8443/tcp` published (ingress), `/shared` `read_only: true`.
- Full binary link NOT attempted (known `ld` OOM signal-9 in this container) — used `--lib` + `cargo check` per instructions.

## Design decisions / notes
- `resolve_http_enabled_override` is pure and total: `true`/`1` -> `Some(true)`, `false`/`0` -> `Some(false)` (case-insensitive, trimmed); unset/empty/garbage -> `None` (no override, debug log, never panics — R-11). Mirrors `resolve_env_config_path`'s test-without-`set_var` pattern (Rust 2024 `#![forbid(unsafe_code)]`).
- Override applied as Step 3c, AFTER the file/env-config merge, so env wins over file + global + compiled default. Compiled default `http.enabled=false` unchanged (ADR-007) — local UDS install unaffected.
- The image's Dockerfile does NOT set `UNIMATRIX_HTTP_ENABLED` — kept clean so the bare image honors the false default; the env lives only in `compose.yaml` per ADR-007.
- `/shared:ro` is hard-set per the brief/pseudocode's "PRESERVE :ro hardening" instruction. Updated the volume security-note comment to flag that first model-populate needs a one-time writable mount (or pre-seed) since `:ro` blocks the first download. Flagging this as a minor operator-ergonomics tradeoff the brief mandated.
- Runbook placed at `docs/cert-rotation.md` (alongside the existing operator doc `docs/client-setup.md`) and referenced from the compose header — satisfies the test plan's "concrete path referenced from compose docs."
- Did NOT touch main.rs/tls.rs/router.rs/public_url.rs/client_bundle.rs (other agents own them). Diff shows sibling-agent changes to those files in the shared checkout — not mine.

## Issues / blockers
- None blocking. The repo compose file is `docker-compose.yml`, not `compose.yaml` as the brief named it — used the real file.
- `config.rs` exceeds the 500-line guideline (pre-existing, ~11k lines); my additions are localized. Splitting it is out of this component's scope.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` was unavailable in this run (deferred MCP tools required schema fetch; proceeded per non-blocking fallback rule using the ADR files + pseudocode in `product/features/vnc-034/`).
- Stored: nothing novel to store — the env-override pattern is a direct parallel of the existing `resolve_env_config_path` convention already in the codebase; ADR-007 already documents the decision. No new gotcha surfaced.
