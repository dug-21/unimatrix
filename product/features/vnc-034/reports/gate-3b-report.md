# Gate 3b Report: vnc-034 (Wave 1)

> Gate: 3b (Code Review)
> Date: 2026-06-11
> Scope: Wave-1 ONLY (#726 server serving + #725 pure-JS client + C1/C2 build-first contract). Wave 2 (#727) intentionally deferred — absence not flagged.
> Branch: feature/vnc-034 (HEAD ee189354)
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Five locked signatures implemented as specified; C1 guard ordering, C2 single-oracle, C4 funnel all match pseudocode |
| 2. Architecture compliance | PASS | C4 seam is the genuine per-request funnel (PathRouter→SlugRouter→resolve_store→dispatch); split-defect from ee189354 closed; ADR-001/002/003/005/007 honored |
| 3. Interface implementation | PASS | StoreResolver / ProjectKey / ProjectSlug / run_client_bundle / derive_public_url / fingerprint_leaf_der / load_or_generate_cert all match the locked surface |
| 4. Test case alignment | PASS | R-01/R-02/R-03/R-05 + token redaction + cross-stack parity all exercised; oracle-regen tests correctly `#[ignore]`d |
| 5. Code quality | PASS (1 WARN) | Compiles clean; no stubs/TODO/unwrap in non-test code; router.rs 562 lines (pre-existing, +39 from seam wiring) — WARN only |
| 6. Security | PASS | Slug allowlist pre-filesystem; bundle length-cap-before-decode; token never in stdout/stderr/logs; no hardcoded secrets; key 0600 |
| 7. Knowledge stewardship | PASS | All 12 impl-agent reports carry `## Knowledge Stewardship` with Queried + Stored entries |

Rust: `cargo check -p unimatrix-server` clean; lib tests 3946 passed / 0 failed; integration targets fingerprint_parity (18p), cert_provisioner (9p), bundle_codec (12p) all green. JS: 840 pass / 0 fail / 1 skip.

## Detailed Findings

### 1. Pseudocode fidelity — PASS
- **C1 bundle codec** (`client_bundle.rs`): `decode_bundle` enforces the LOCKED guard ordering — GUARD 1 length cap on the RAW string (`raw.len() > MAX_RAW_LEN`, 4096) BEFORE base64url-decode/JSON-parse, then scheme → base64url → JSON → strict schema (exactly 4 keys, `v==1`, `base_url` https, token 64-hex, `fp ^sha256:[0-9a-f]{64}$`). The JS mirror (`bundle.js`) implements the same ordering; `Buffer.byteLength(raw) > MAX_RAW_LEN` runs first and rejects an over-cap non-base64url string on LENGTH (AC-W1-C10).
- **C2 fingerprint** (`tls.rs::fingerprint_leaf_der`): `"sha256:" + lowercase_hex(sha256(der))`, DER-in (not PEM), leaf-only. `leaf_der_from_pem` extracts the served leaf for the bundle so `fp` pins the served cert (AC-W1-S4). The JS `computeFingerprint` mirrors it byte-for-byte. Single oracle: the committed corpus is emitted by `#[ignore]`d Rust regen tests, never hand-written (SR-02).
- **CertProvisioner** (`cert_provisioner.rs`): generate-first + `O_CREAT|O_EXCL` key claim (0600) so two racing first boots converge on one credential; load-verbatim on reboot (R-07); production SANs/validity (825d) not rcgen test defaults (R-08).
- **PublicUrl** (`public_url.rs`): single `derive_public_url` feeding base_url + host + sans; host∈sans by construction; no socket auto-detect; placeholder + WARN when unset.

### 2. Architecture compliance — PASS (split-defect verified closed)
The C4 seam is confirmed to be the **per-request funnel**, NOT a boot-time one-shot bypass:
- `PathRouter` holds a `SlugRouter<ReqBody>`; the MCP fall-through arm (`router.rs:278-284`) clones it and calls `router.route_mcp(request).await` per request.
- `SlugRouter::route_mcp` (`seam.rs:255-300`) runs `parse_project_key(path)` → `self.resolver.resolve_store(&key)` → `project_router.route_mcp`. `UnknownProject`/`InvalidSlug` short-circuit to JSON errors — never a panic, never a path join, never the default store on `UnknownProject`.
- `PathRouter::new(resolver, project_router, observe_ctx)` builds the `SlugRouter` internally; `main.rs:939` wires it. The resolver arg alone is the Wave-1↔Wave-2 swap point.
- The `#[allow(dead_code)]`/`#[allow(unused_imports)]` placeholders on the seam are gone (`router.rs:316` comment confirms removal); `SlugRouter`/`StoreResolver` have a real production caller.
- `/observe` acquires its store handle through the funnel once at boot (`main.rs:901`, `resolve_store(&ProjectKey::Default)`), consistent with ADR-005 (observe is not on the MCP seam).
- ADR-007: `UNIMATRIX_HTTP_ENABLED` env override implemented as a pure, total, tested function (`config.rs:2957`); global default `http.enabled=false` preserved.

### 3. Interface implementation — PASS
All five locked signatures present and matching: `StoreResolver::resolve_store(&ProjectKey)->Result<Arc<Store>,RouteError>`, `run_client_bundle(Option<PathBuf>)->Result<(),ServerError>`, `derive_public_url(&Env)->PublicUrl`, `fingerprint_leaf_der(&[u8])->String`, `load_or_generate_cert(&Path,&[String])->Result<(CertPem,KeyPem),ServerError>`. `ProjectKey`/`ProjectSlug`/`RouteError` re-exported as the public seam surface. `ProjectSlug::TryFrom` enforces `^[a-z0-9][a-z0-9-]{0,62}$` at the parse edge.

### 4. Test case alignment — PASS
- **R-01 (Critical)**: `test_slug_key_under_default_resolver_returns_unknown_project` (no fallback), `test_resolver_swap_requires_no_callsite_change` (Wave1↔Wave2 boundary IS the trait), real `DefaultResolver` tests (same Arc each call, slug inert), and AGENT-6 per-request wiring proof — `CountingResolver` verifies `resolve_store` is consulted per request with the transport-derived key, plus a structural proof that PathRouter's MCP edge IS a SlugRouter.
- **R-02**: JS `computeFingerprint(der)` asserted equal to corpus `fp` for every `der_hex` entry; `decode_bundle(wire)` `deepStrictEqual` corpus fields; mismatch rejection names expected vs presented fp.
- **R-03**: `test_projectslug_rejects_traversal_corpus` (`../`, `%2f`, `%2e`, absolute, leading-hyphen, uppercase, over-length, empty); `test_route_grammar_rejects_traversal_slug_before_resolution` proves rejection at the allowlist edge before resolve_store/path join.
- **R-05**: bundle malformed/oversized/extra-field/bad-scheme/bad-base64/bad-json corpus rejected in both stacks; round-trip parity.
- **Token redaction (AC-W1-S5b/NFR-06)**: `test_client_bundle_token_absent_from_stdout_and_stderr` (token in NEITHER, recoverable only from inside the blob); `render_output` makes the stdout=blob-only / stderr=base_url+fp contract unit-testable.

### 5. Code quality — PASS (1 WARN)
- `cargo check -p unimatrix-server` compiles clean (25 pre-existing crate warnings, none introduced by vnc-034).
- No `todo!()`/`unimplemented!()`/`TODO`/`FIXME` in vnc-034 source. All `panic!` occurrences are in `#[cfg(test)]` modules. No `.unwrap()`/`.expect()` in non-test code (grep hits are doc-comment text).
- File sizes: all vnc-034 new files < 500 lines (seam.rs 301, default_resolver.rs 74, client_bundle.rs 459, cert_provisioner.rs 437, public_url.rs 455, tls.rs 450, http_provision.rs 82).
- **WARN — router.rs at 562 lines.** Over the 500 guideline. Pre-existing condition (523 on main; vnc-034 added +39 for the SlugRouter-into-PathRouter wiring). The seam logic was extracted into `router/seam.rs` + `router/default_resolver.rs` to minimize the addition. Not introduced by this feature in spirit; flagged for a future split, does not block.

### 6. Security — PASS
- Slug allowlist enforced at the parse edge BEFORE any filesystem use; `../`, encoded separators, absolute paths, and percent-encodings are unrepresentable (cannot pass the charset) — escape is structural-impossible, not runtime-rejected.
- Bundle parser: 4 KB raw-string length cap runs BEFORE decode/parse (DoS pre-filter); strict 4-key schema is the load-bearing guard; no token in any error message.
- Token redaction verified (check 4). stdout = opaque blob only.
- No hardcoded secrets: token/cert/key are files on the data volume; key mode 0600 set at creation via `O_CREAT|O_EXCL` (no chmod window) and re-asserted on load.
- `read_token_hex` validates 64-hex shape; deserialization (`decode_bundle`, `validate_schema`) cannot panic on malformed input (parses to `serde_json::Value`, returns typed errors).

### 7. Knowledge stewardship — PASS
All 12 implementation-agent reports under `agents/` contain a `## Knowledge Stewardship` block. Active-storage agents (agents 3–6) carry `Queried:` + `Stored:` entries; the read-only pseudocode agent (agent-1) carries `Queried:` only — correct per role.

## Minor observations (non-blocking, for cleanup)
1. `public_url.rs:19` retains a module-level `#![allow(dead_code)]` whose justifying comment ("until that wiring lands") is now stale — `derive_public_url`/`Env`/`PublicUrl` are consumed by `client_bundle.rs` and `http_provision.rs`. No dead_code warning for this module surfaces elsewhere; the allow is likely now masking nothing. Recommend removing the allow + stale comment in a follow-up (not a gate blocker — no functional impact).
2. router.rs split (the 562-line WARN above) is the natural companion cleanup.

## Environment constraints honored
- Did NOT build/run/test the `bin "unimatrix"` target (ld OOM in container). Validated via `cargo check -p unimatrix-server` + lib tests + the three per-integration-test targets, per spawn instructions.
- `http::token::tests::test_concurrent_creation_no_corruption` pre-existing flake — not run under full-suite parallelism here; token.rs untouched by vnc-034.
