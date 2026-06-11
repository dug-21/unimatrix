# vnc-034 Agent 5 — Server Integration Report

Wave-1 Sub-wave 3 listener wiring + `client-bundle` subcommand dispatch.

## Files modified / created

- `crates/unimatrix-server/src/main.rs` — (1) added `Command::ClientBundle` variant + sync dispatch arm (C-10, pre-tokio); (2) reworked the `config.http.enabled` listener block to derive the public URL, provision the cert, build the TLS acceptor from the provisioned PEM, and acquire the served store THROUGH the `DefaultResolver` funnel.
- `crates/unimatrix-server/src/http_provision.rs` — **NEW** binary-crate wiring helper (`provision_tls`): `derive_public_url` -> `load_or_generate_cert(&sans)` -> `TlsConfig` at the resolved PEM paths -> `build_tls_acceptor`. Keeps `main.rs` glue thin (82 lines, well under the cap).
- `crates/unimatrix-server/src/http/mod.rs` — re-exported the C3/C4 wiring surface for the binary crate: `derive_public_url`, `PublicUrl`, `Env`, and `SlugRouter`, `DefaultResolver`, `ProjectKey`, `ProjectSlug`, `RouteError`, `StoreResolver`. (These were `pub(crate)`/router-internal and unreachable from `main.rs`; `http/mod.rs` is not in the forbidden set.)

NOT touched (per constraints): `config.rs`, `tls.rs`, `router.rs`, `router/seam.rs`, `router/default_resolver.rs`, `public_url.rs`, `client_bundle.rs`. No new crates, no `unsafe`, no `.unwrap()`/`.expect()` in non-test code, errors via `ServerError`.

## Task 1 — `client-bundle` subcommand: DONE

`Command::ClientBundle` added to the enum and dispatched in the C-10 sync block right after `Health`, returning `run_client_bundle(cli.project_dir).map_err(Into::into)` (the arm's error type is `Box<dyn Error>`; `ServerError: Error`, so `Into::into` bridges it). No tokio, no tracing init — matches the `health`/`version` sync pattern.

## Task 2 — listener wiring: cert/TLS DONE; per-request seam insertion BLOCKED

Done and live:
- `provision_tls(&paths.data_dir)` derives the single `PublicUrl` (C3), first-boot provisions `tls/{cert,key}.pem` with those SANs (SR-01, key 0600 via the existing provisioner), and builds the acceptor from the provisioned files (cloud HTTPS posture: an absent acceptor is surfaced as a loud `ServerError::Config`, not a silent `None`). The same files feed `client-bundle`'s C2 leaf-DER fingerprint.
- The served `Arc<Store>` threaded into MCP (`observe_ctx.store`/`entry_store`) is obtained ONLY via `DefaultResolver::resolve_store(&ProjectKey::Default)` — the no-bypass guarantee (FR-X5 / A4) holds at the store-acquisition boundary.
- `start_http_listener` now receives `Some(tls_acceptor)` from the seam-provisioned cert.

### BLOCKER — per-request `PathRouter -> SlugRouter -> ProjectRouter` insertion

`PathRouter::new(project_router: ProjectRouter<ReqBody>, ...)` is **concrete over `ProjectRouter`**, and its MCP fall-through arm calls the **private** `ProjectRouter::route_mcp`. Inserting `SlugRouter` as the per-request MCP edge (so every request runs `parse_project_key -> resolve_store -> dispatch`) therefore requires generalizing `PathRouter` to accept any `route_mcp`-capable layer — a one-line change in `router.rs`, which is an explicitly forbidden file for this agent (owned by the slug-router agent). I confirmed this is the *only* compile error when `SlugRouter` is passed to `PathRouter::new` (`E0308 expected ProjectRouter, found SlugRouter`); the lib otherwise compiles clean with the seam fully constructed.

Resolution chosen (strict constraint adherence): the seam is constructed and the served store is funnel-sourced at boot, but the `SlugRouter` is not yet the per-request edge. The blocker is documented inline at the wiring site. Follow-up (router.rs-scoped, ~1 line): relax `PathRouter` to hold the MCP layer generically (e.g. an `McpEdge` trait or `PathRouter<ReqBody, M>` with `M: route_mcp`), then pass `SlugRouter::new(Arc::new(DefaultResolver::new(store)), project_router)` as that edge. The seam authors anticipated this (slug-router report: "next-wave agent should DELETE these allows when wiring the listener") — the `#[allow(dead_code)]` on `router/seam.rs` + `router/default_resolver.rs` can be dropped in the same router.rs follow-up (left in place here since those files are out of scope).

## Validation

- `cargo check -p unimatrix-server` — **PASS** (0 errors; the 25 lib warnings are pre-existing in eval/test modules, none reference `main.rs`/`http_provision.rs`/`http/mod.rs`).
- `cargo build -p unimatrix-server --lib` — **PASS**.
- `cargo clippy -p unimatrix-server --lib` — no findings on the touched files.
- `cargo fmt -p unimatrix-server` — applied.
- **KNOWN CONTAINER LIMIT (not a failure):** the full `bin "unimatrix"` link cannot complete here — `ld` is OOM-killed (signal 9) on the 113-object link. The binary type-checks via `cargo check` (which does not link). Per the spawn instructions, `cargo run` / `cargo build` (bin) / `cargo test` on the binary target were not attempted; the link-OOM is the environment limitation, not a wiring defect.

## Issues / blockers

1. **Per-request SlugRouter insertion** — blocked by the forbidden router.rs edit (see above). No-bypass is preserved at the store-acquisition boundary; the per-request layer is a documented one-line router.rs follow-up.
2. `client_bundle::run_client_bundle` was already implemented + exported (`pub mod client_bundle`); task 1 was pure dispatch wiring, as expected.

## Knowledge Stewardship
- Queried: `context_search` (pattern: "sync pre-tokio subcommand dispatch listener wiring") -> #2651 (nested clap pre-tokio), #4577 (sync CLI subcommand pattern); `context_search` (decision, topic vnc-034) -> #4952/#4949/#4950 (ADR-006/005/003). Applied the C-10 sync-dispatch placement (before any tokio init) and the ADR-003/005 single-funnel store acquisition.
- Stored: entry #4962 "Wave-1 SlugRouter cannot be wired per-request from main.rs while PathRouter is concrete over ProjectRouter" (lesson-learned).
