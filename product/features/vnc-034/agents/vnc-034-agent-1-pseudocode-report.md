# Agent Report — vnc-034-agent-1-pseudocode (Wave 1)

## Task
Produce per-component pseudocode for vnc-034 **Wave 1 only** (single-project HTTPS serving + pure-JS remote client + the C1/C2 connection-contract build-first sub-deliverable). Wave 2 explicitly out of scope.

## Deliverables (all under product/features/vnc-034/pseudocode/)
- OVERVIEW.md — component interaction, data flow, shared types, locked contracts, C1/C2 build-first ordering rule, Wave-2 seam points, runbook doc-deliverable note.
- cert-provisioner.md — `load_or_generate_cert` (idempotence, 0600 key, production SANs/validity, fail-loud).
- fingerprint-computer.md — `fingerprint_leaf_der` (C2 oracle, leaf-DER extraction, parity corpus generation).
- public-url.md — `derive_public_url` (C3 single derivation, three consumers, placeholder, injectable Env).
- bundle-codec.md — `run_client_bundle` Rust encoder + the shared decode contract (4 KB length-first guard, strict-schema load-bearing reject, stdout/stderr token-redacted split).
- slug-router.md — `StoreResolver`/`ProjectKey`/`ProjectSlug` seam + `SlugRouter` layer + route grammar + allowlist parse edge (Wave-1 minimal; slug resolver modeled, NOT implemented).
- default-resolver.md — Wave-1 `StoreResolver` impl; local/cloud parity equivalence.
- remote-client.md — `init --remote` JS bundle decoder mirror + `checkServerIdentity` cert pin + slug append + size gate + 1:1 transport enforcement.
- container-posture.md — `UNIMATRIX_HTTP_ENABLED` config env-read + Dockerfile/compose posture (TLS-only port, nan-014 hardening preserved).

NOT produced (correctly out of scope): pseudocode/project-router.md, pseudocode/project-registry.md (Wave 2); no pseudocode for the cert-rotation runbook (doc deliverable, FR-A11 — noted in OVERVIEW as Stage-3b doc).

## Components covered
CertProvisioner, FingerprintComputer, PublicUrl, BundleCodec, SlugRouter+seam, DefaultResolver, RemoteClient, Container posture. (8 of 8 Wave-1 rows in the brief Component Map.)

## Locked contracts honored exactly
- `StoreResolver::resolve_store(&ProjectKey) -> Result<Arc<Store>, RouteError>`; `run_client_bundle(project_dir) -> Result<(), ServerError>`; `derive_public_url(env) -> PublicUrl`; `fingerprint_leaf_der(der) -> "sha256:"+hex`; `load_or_generate_cert(data_dir, sans)`.
- C1 wire form `unimatrix-bundle:<base64url-nopad(canonical-json)>`, fixed field order `v,base_url,token,fp`.
- Guard ordering: 4 KB RAW-string byte-length cap BEFORE base64url-decode and BEFORE JSON-parse; strict-schema reject is the load-bearing guard. Encoded identically on Rust (bundle-codec.md) and JS (remote-client.md).
- C2: `sha256:<lowercase hex>` over served leaf DER (not PEM, not chain); Rust oracle + parity corpus, JS golden never hand-written.

## Grounding
Read all three source docs + all 7 ADRs + the existing code surface: tls.rs (test helper to promote, build_tls_acceptor, rustls_pemfile parse), token.rs (idempotence/O_EXCL/0600 pattern reused for cert), router.rs (PathRouter/ProjectRouter/McpAdapter insertion point), main.rs (C-10 sync subcommand block + listener wiring lines), config.rs (load_config env-read pattern + Rust-2024 pure-function approach for testable env), init.js + transport-http.js + init-remote.test.js (extend initRemote; thread checkServerIdentity into mod.request options). No interface names invented — every signature traces to ARCHITECTURE §7 or the codebase.

## Constraints reflected in pseudocode
No new crates; no `unsafe`; no `.unwrap()` in non-test; max 500 lines/file (each component is one focused file); secrets as 0600 files never in DB; token redacted from stdout/stderr/logs except the opaque blob; distroless no-shell first-boot in the Rust binary; injectable Env to avoid unsafe `set_var`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern) — surfaced general service/param patterns (#316, #281, #261), none directly cert/bundle-specific; (decision, topic vnc-034) — surfaced the vnc-034 ADRs as stored entries (#4952 ADR-006, #4949 ADR-005, #4954 ADR-001), consistent with the ADR files read. No conflicting prior patterns found.
- Deviations from established patterns: none. Cert idempotence deliberately mirrors the existing `load_or_generate_token` generate-first + O_CREAT|O_EXCL + 0600 pattern; the env-read mirrors the existing `resolve_env_config_path` pure-function approach.

## Open questions / gaps for downstream
1. **Parity corpus file location** — fingerprint-computer.md and remote-client.md reference a committed cross-stack fingerprint fixture; its exact path is a Stage-3a test-plan decision (suggested `packages/unimatrix/test/fixtures/`). Flagged, not invented.
2. **`allowed_hosts` host-posture wiring** — public-url.md specifies the derivation (`host` real vs `<EDIT-ME>` permissive-with-warning); the precise wiring into the existing rmcp `allowed_hosts`/`allowed_origins` host-check is a listener-wiring detail the server implementer confirms against current `HttpConfig` semantics (router.rs notes rmcp defaults allowed_hosts to localhost — CVE-2026-42559 fix). Not contradicted; needs implementer confirmation of the exact field threaded.
3. **`http.enabled` listener gate vs `UNIMATRIX_HTTP_ENABLED`** — container-posture.md applies the override inside `load_config`; confirm the listener gate in main.rs reads the merged `config.http.enabled` (it does today) so the env flip is honored at the gate. Low risk; called out for the wiring agent.
4. **`base64url` crate vs manual** — pseudocode assumes a base64url-nopad codec is reachable from already-present deps (the `base64` crate is commonly present); if not in the tree, encode/decode is a small manual implementation — no NEW crate either way (NFR-02). Implementer to confirm which is available.

None of these block Stage 3b; they are wiring confirmations, not design gaps.
