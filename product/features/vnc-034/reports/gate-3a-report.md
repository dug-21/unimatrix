# Gate 3a Report: vnc-034

> Gate: 3a (Design Review — Component Design vs. Source Documents)
> Date: 2026-06-11
> Scope: Wave 1 ONLY (single-project HTTPS serving #726 + pure-JS remote client #725 + C1/C2 connection-contract build-first sub-deliverable). Wave 2 (#727 routing) deliberately deferred per ADR-006 + Component Map — absence of project-router/project-registry artifacts is NOT a gap.
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | All 8 Wave-1 components map to ARCHITECTURE §2; the five locked signatures match §7 verbatim; seam matches ADR-003; route grammar matches ADR-005. |
| 2. Specification coverage | PASS | Every Wave-1 FR (A1–A11, B1–B9, X1–X5) + NFRs realized in pseudocode; no scope additions; Wave-2 FRs correctly absent. |
| 3. Risk coverage | PASS | All 13 risks (R-01..R-13) + rotation mapped to concrete test scenarios with AC-IDs; R-01 (Critical) gets all 4 scenarios; R-03 parse-edge correctly held in Wave 1. |
| 4. Interface consistency | PASS | Shared types in OVERVIEW match per-component usage; C1 wire form + 4 KB length-first guard ordering + C2 sha256-leaf-DER parity all coherent across Rust/JS; StoreResolver seam held at the trait. |
| 5. Knowledge stewardship | PASS (1 WARN) | Both design-agent reports carry a `## Knowledge Stewardship` block with `Queried:` entries. Test-plan agent has explicit `Stored: nothing novel -- {reason}`. Pseudocode agent omits the explicit `Stored:` line (WARN, non-blocking). |

Checks passed: 5 / 5 (1 warning). No FAIL.

---

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS

**Evidence**:
- Component decomposition matches ARCHITECTURE §2 exactly. The 8 Wave-1 pseudocode files (cert-provisioner, fingerprint-computer, public-url, bundle-codec, slug-router, default-resolver, remote-client, container-posture) are precisely the Wave-1 rows of the Component Map; the two Wave-2 rows (ProjectRouter, ProjectRegistry) are correctly absent.
- The five locked signatures match the IMPLEMENTATION-BRIEF "Function Signatures" block and ARCHITECTURE §7 verbatim:
  - `StoreResolver::resolve_store(&ProjectKey) -> Result<Arc<Store>, RouteError>` (slug-router.md, default-resolver.md)
  - `run_client_bundle(project_dir: Option<PathBuf>) -> Result<(), ServerError>` (bundle-codec.md)
  - `derive_public_url(env: &Env) -> PublicUrl` (public-url.md)
  - `fingerprint_leaf_der(der: &[u8]) -> String` (fingerprint-computer.md)
  - `load_or_generate_cert(data_dir, sans) -> Result<(CertPem, KeyPem), ServerError>` (cert-provisioner.md)
- Seam design matches ADR-003: one `StoreResolver` trait, `DefaultResolver` Wave-1 impl returning the one store for `Default` and `UnknownProject` for any `Slug`; `SlugRouter` thin layer parse→resolve→dispatch; per-slug hot-path explicitly modeled as living INSIDE the seam method (Wave 2), not a new edge (SR-07).
- Route grammar matches ADR-005: `/v1/tools/...` → `ProjectKey::Default`; `/v1/{slug}/tools/...` → `ProjectKey::Slug` (parses but inert under `DefaultResolver`). `/health`, `/observe` untouched.
- Technology choices honor ADRs/NFR-02: no new crates (rcgen 0.13, tokio-rustls 0.26, rustls-pemfile 2, rand 0.9 reused); cert promotion of the test-only helper; `UNIMATRIX_HTTP_ENABLED` env per ADR-007; injectable `Env` accessor honoring Rust-2024 `forbid(unsafe_code)` (no `set_var`).
- Integration points reuse the existing surface from ARCHITECTURE §6/§7: `build_tls_acceptor`, `load_or_generate_token`, `rustls_pemfile`, `PathRouter`/`ProjectRouter`/`McpAdapter`, C-10 sync subcommand block, `load_config` env-read pattern.

### Check 2 — Specification coverage
**Status**: PASS

**Evidence**: Every Wave-1 functional requirement has corresponding pseudocode:
- Group A (A1–A11): cert-provisioner (A2/A3/A4/A9), fingerprint-computer (A6), public-url (A7), bundle-codec (A5/A5b — stdout-blob-only + stderr token-redacted echo), container-posture (A1/A8/A10), cert-rotation runbook noted as a Stage-3b doc deliverable (A11) with its code-side pairing in remote-client `checkServerIdentity`.
- Group B (B1–B9): remote-client covers pure-JS zero-dep ingestion (B1), cert pin (B2), 1:1-at-transport (B3), attach≠register (B4), per-OS incl. Windows-HTTPS-only (B5), multi-LLM N:1 shared path (B6), skills-copy + no-CLAUDE.md-append + pointer (B7), <250 KB copy-install (B8), strict-schema + 4 KB length-first guard (B9).
- Cross-wave seam (X1–X5): slug-router + default-resolver cover single-funnel (X1), transport-derived identity / no payload carrier (X2), sole-write-capability `Arc<Store>` (X3), local-UDS parity reduction (X4), Wave-1-store-through-the-seam (X5).
- NFRs addressed: no-unsafe/no-unwrap/500-line (cross-cutting constraints in OVERVIEW + each file), secrets-as-0600-files (cert-provisioner, container-posture), token confidentiality (bundle-codec redaction), local-repo parity (default-resolver regression), TLS-seam preservation (cert-provisioner notes `TlsConfig` seam intact).

**No scope additions**: pseudocode implements no unrequested features. The NOT-in-Scope exclusions (no `/metrics`, no plaintext mode, no slug-listing surface, no Wave-2 routing logic) are respected — slug-router explicitly models the slug resolver without implementing it.

### Check 3 — Risk coverage
**Status**: PASS

**Evidence**: Test-plan OVERVIEW §2 maps all 13 risks to component plans + AC-IDs; each component file carries concrete, named test scenarios.
- **R-01 (Critical)** — slug-router + default-resolver cover all 4 RISK-TEST scenarios: single-funnel source assertion (`test_single_funnel_source_assertion`), resolver-swap with no call-site change (`test_resolver_swap_requires_no_callsite_change`), `Slug` under `DefaultResolver` → `UnknownProject` not panic/not default store, per-slug hot-path inside the seam method (structural assertion).
- **R-02 (High)** — fingerprint-computer is the single Rust oracle; bundle-codec + remote-client consume the committed `c1c2-parity/{fingerprint,bundle}-golden.json` corpus; JS golden never hand-written (SR-02). Covers cross-stack parity, served-cert==bundle-fp (AC-W1-S4), mismatch reject, casing/PEM-reject.
- **R-03 (High)** — slug-router tests `ProjectSlug::TryFrom` traversal corpus (`../`, `%2f`, `%2e`, absolute, uppercase, empty, >63) rejected pre-filesystem. Correctly held as Wave-1 work (the parse-edge guard) even though the full routing-escape proof AC-W2-R6 is Wave 2.
- **R-04 (High)** — default-resolver carries the local-install regression test IN the Wave-1 set (`test_local_install_resolves_path_hash_store_through_seam`, AC-W1-X2/NFR-10), not deferred.
- **R-05 (High)** — bundle-codec + remote-client: guard ordering proven by `test_bundle_length_cap_before_decode` (asserts the length-cap error variant, not a decode/parse error); strict-schema reject corpus.
- **R-06 (High)** — source/type assertions: no payload field names a project; client has no second-project field.
- **R-07/R-08/R-09/R-10/R-11/R-12/R-13** — each mapped (cert-provisioner idempotence + production params + fail-loud; public-url single-derivation + host∈SAN; seam preservation; hard invariants; additive addressing).
- Edge cases from RISK-TEST §Edge Cases are explicitly assigned to component files and declared not-optional (lesson #3386 applied).

### Check 4 — Interface consistency
**Status**: PASS

**Evidence**:
- Shared types declared in OVERVIEW.md "Shared types" table (`ProjectKey`/`ProjectSlug`/`StoreResolver`/`RouteError`, `PublicUrl`, `CertPem`/`KeyPem`, Bundle JSON, `Env`) match their per-component definitions and usages — no contradictions across files.
- **C1 wire form**: `unimatrix-bundle:<base64url-nopad(canonical-json)>`, fixed order `v, base_url, token, fp`, identical in bundle-codec.md (Rust encoder/decoder) and remote-client.md (JS decoder). Constants identical on both sides: `MAX_RAW_LEN=4096`, `TOKEN_RE=^[0-9a-f]{64}$`, `FP_RE=^sha256:[0-9a-f]{64}$`.
- **4 KB length-check-before-decode guard ordering**: both files implement GUARD 1 (raw-string byte-length cap) BEFORE GUARD 3 (base64url-decode) and GUARD 4 (JSON-parse), with GUARD 5 strict-schema as the load-bearing guard. Both explicitly require an over-cap non-base64url string to reject on length, not on a decode error (AC-W1-C10).
- **C2 sha256-leaf-DER parity-fixture rule**: fingerprint-computer.md is the sole Rust oracle; remote-client `computeFingerprint` mirrors it; the committed corpus (test-plan OVERVIEW §3) is the single source of truth, JS golden never hand-written. DER-not-PEM, leaf-not-chain, lowercase-hex held consistently.
- **StoreResolver seam**: the Wave 1↔2 boundary is held exactly at the trait. slug-router.md and default-resolver.md both model the Wave-2 swap as `Arc<dyn StoreResolver> = ProjectRouter{..}` at the same `SlugRouter::new` call site with no interface re-cut; the slug-resolver LOGIC is explicitly NOT implemented (modeled only). Boundary correctly held.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS (1 WARN)

**Evidence**:
- `vnc-034-agent-2-testplan-report.md` has a complete `## Knowledge Stewardship` block: `Queried:` (context_briefing/search/get — ADRs, lessons #3386, #4792) and explicit `Stored: nothing novel to store -- {reason}` with a substantive reason (patterns already captured in #4766/#3386).
- `vnc-034-agent-1-pseudocode-report.md` has a `## Knowledge Stewardship` block with `Queried:` entries (context_search for patterns + decisions) and a "Deviations from established patterns: none" note, but **omits an explicit `Stored:` / "nothing novel to store -- {reason}" line**.

**WARN (non-blocking)**: Pseudocode agent's stewardship block is present and shows querying evidence, but lacks the explicit `Stored:`/"nothing novel" declaration. Per the Gate 3a rubric a missing block is a REWORKABLE FAIL; a present block without a reason after "nothing novel" is a WARN. The block is present with query evidence and a deviations-none note that functionally conveys nothing-novel, so this is WARN, not FAIL.

---

## Wave-1 ↔ Wave-2 boundary verification (spawn-prompt focus)

| Required hold | Verified |
|---------------|----------|
| Seam modeled at `StoreResolver` trait | YES — slug-router.md + default-resolver.md define the trait, the `SlugRouter` call site, and the Wave-2 swap point. |
| slug-resolver NOT implemented | YES — slug-router.md states "models the seam only; does NOT implement the slug resolver"; `ProjectKey::Slug(_)` → `UnknownProject` in Wave 1. |
| `ProjectSlug` allowlist parse edge IS Wave-1 | YES — the `TryFrom` guard is built and tested now (the seam needs it before Wave 2 can route). |
| Wave-2 artifacts deliberately absent | YES — project-router.md / project-registry.md correctly not produced (ADR-006). |

## Warnings (non-blocking)

1. **WARN — pseudocode agent stewardship block lacks explicit `Stored:` line.** Block is present with `Queried:` evidence and "Deviations: none"; recommend future design agents include the literal `Stored: nothing novel to store -- {reason}` line for rubric cleanliness. Does not block Stage 3b.
2. **Downstream wiring confirmations (already flagged by the pseudocode agent, not design gaps):** parity-corpus exact path, `allowed_hosts`/rmcp host-check field wiring, `base64url` codec availability vs manual, `http.enabled` listener-gate read. All are Stage-3b implementer confirmations, not interface drift.

## Rework Required

None. Gate 3a PASSES. Proceed to Stage 3b (implementation).
