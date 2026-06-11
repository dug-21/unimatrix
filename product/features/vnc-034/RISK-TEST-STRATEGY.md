# Risk-Based Test Strategy: vnc-034

> Personal-cloud multi-project serving. Wave 1 = server serving (Group A) + pure-JS client (Group B); Wave 2 = multi-project routing (Group C). Six locked contracts C1–C6. Risks are design-specific, traced to SCOPE-RISK-ASSESSMENT (SR-XX) and SPECIFICATION (FR/AC/NFR).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Deferred `StoreResolver` seam (Wave-1 minimal → Wave-2 additive) breaks on the trait swap: Wave-2 `ProjectRouter` injected outside the seam, or Wave-1 store served *around* not *through* the funnel | High | High | **Critical** |
| R-02 | C2 fingerprint diverges Rust-oracle ↔ JS-client: DER vs PEM, wrong cert in chain, or hex casing — silent pin failure at connect time | High | Med | **High** |
| R-03 | Slug parser trust boundary fails to block `../`/encoded separators → filesystem escape from `/data/.unimatrix/{slug}/` | High | Med | **High** |
| R-04 | C4 cloud-slug path and local-UDS path-hash path become two code paths → local-parity (NFR-10) silently regresses; cloud built on unproven seam | High | Med | **High** |
| R-05 | C1 bundle parser accepts malformed/oversized/extra-field input → crash or DoS at the client trust boundary | High | Med | **High** |
| R-06 | 1-client:1-project enforced only by client config, not transport → misconfigured client mis-targets and corrupts another project's hash chain (unrollbackable) | High | Med | **High** |
| R-07 | First-boot cert/token auto-gen not idempotent on restart → credentials regenerated, every pinned client + emitted bundle silently invalidated | High | Med | **High** |
| R-08 | Promoted `rcgen` cert carries test-grade defaults (weak SAN, short validity, non-0600 key) into the trust root | High | Med | **High** |
| R-09 | C3 single-knob mis-derivation desyncs cert SAN from bundle base-url → client connects but host/fingerprint mismatch | Med | Med | Medium |
| R-10 | OSS↔enterprise seams (`TlsConfig`, `BearerValidator`, slug-as-scope) collapsed by a Wave-1 shortcut → enterprise re-architecture | Med | Med | Medium |
| R-11 | Distroless first-boot on unwritable `/data` panics/`.unwrap()`s instead of failing loud-and-actionable | Med | Med | Medium |
| R-12 | Invariant leaks: plaintext port published, token in a DB/log/image layer, or install > 250 KB | High | Low | Medium |
| R-13 | OQ-C addressing mis-call: Wave-1 client must re-init when Wave 2 lands (non-additive `/{slug}`) | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Deferred isolation seam swap (Critical)
**Severity** High · **Likelihood** High · **Impact** Wave 2 builds on a bypassed seam; routing added outside the funnel; every project's integrity rides on the seam that was never exercised in Wave 1.
**Test Scenarios**:
1. Source-grade assertion: the Wave-1 single store is reached **only** via `resolve_store(ProjectKey::Default)` — no call site obtains `Arc<Store>` by another route (AC-W1-X1, FR-X1/X5).
2. Swap test: replacing `DefaultResolver` with a stub `ProjectRouter` requires no change to the `SlugRouter` call site or route grammar.
3. `ProjectKey::Slug(_)` against `DefaultResolver` returns `RouteError::UnknownProject` (not a panic, not the default store).
4. Per-slug hot-path routing resides inside the seam method, not in a new edge layer.
**Coverage Requirement**: A single-funnel source assertion + a resolver-swap test must both pass; zero bypass call sites.

### R-02: Cert-fingerprint cross-stack parity (High)
**Severity** High · **Likelihood** Med · **Impact** Pinning silently fails; onboarding breaks at connect time with an opaque TLS error.
**Test Scenarios**:
1. Cross-stack parity fixture generated from the Rust oracle (never hand-written): `fingerprint_leaf_der(der)` == JS `checkServerIdentity` compute over the same DER (AC-CT-C2, FR-B2).
2. Bundle-emitted fingerprint == SHA-256 of the leaf DER actually served on the TLS port (AC-W1-S4) — proves the bundle pins the served cert, not a stale one.
3. Negative: mismatched/changed cert rejected by the client (AC-W1-C2).
4. Casing/format: assert `sha256:` prefix + lowercase hex on both stacks; reject uppercase or PEM-derived input.
**Coverage Requirement**: Parity fixture from a single Rust oracle; server==client byte-equality + served-cert equality + reject-on-mismatch.

### R-03: Slug allowlist trust boundary (High — fix-before-merge)
**Severity** High · **Likelihood** Med · **Impact** Path traversal escapes the per-slug data dir; reads/writes another project's volume.
**Test Scenarios**:
1. `ProjectSlug::TryFrom` rejects `../`, encoded `%2f`/`%2e`, absolute paths, `.`/`/`, over-length (>63), empty, uppercase (FR-C5, AC-W2-R6).
2. Validation occurs at the parse edge **before** any filesystem use; rejected slug never reaches a path join.
3. Property/fuzz: no accepted slug resolves to a path outside `/data/.unimatrix/{slug}/`.
**Coverage Requirement**: Allowlist `^[a-z0-9][a-z0-9-]{0,62}$` enforced pre-filesystem; traversal corpus all rejected; no escape demonstrated.

### R-04: Local-UDS / cloud seam parity (High)
**Severity** High · **Likelihood** Med · **Impact** Local install silently diverges; the "common case is the proving ground" guarantee is void.
**Test Scenarios**:
1. **Wave-1 local-install regression test** (not deferred): local UDS resolves the path-hash store *through* the same `resolve_store` seam (AC-W1-X2, NFR-10).
2. Assert path-hash logic (ADR-004) is unchanged and lives behind the same trait as the slug resolver — slug never leaks into the local path, path-hash never leaks into cloud (A2).
3. Container move/remount: cloud slug identity is path-independent (operator-declared), unaffected by a hash that would change locally.
**Coverage Requirement**: One seam, two resolvers; local regression test in the Wave-1 set; no cloud-only isolation path.

### R-05: Bundle parser trust boundary (High)
**Severity** High · **Likelihood** Med · **Impact** Client crash or DoS on hostile/malformed bundle.
**Test Scenarios**:
1. Schema-validate `{v,base_url,token,fp}`; reject missing/extra/wrong-type fields (FR-B9, AC-W1-C9).
2. Reject bad scheme prefix, non-base64url body, truncated payload, and over-cap length (DoS guard, OQ-3 ≤4 KB).
3. Round-trip: server `unimatrix-bundle:` encode → client decode yields identical fields.
**Coverage Requirement**: Malformed/truncated/oversized corpus rejected with no crash; strict schema + length cap.

### R-06: 1:1 enforced at transport, not config (High)
**Severity** High · **Likelihood** Med · **Impact** A misconfigured client writes into another project's attributed hash chain — catastrophic, unrollbackable.
**Test Scenarios**:
1. Inspect request types: no payload field names a project; identity is transport-derived only (FR-X2, AC-W1-X3).
2. Client has no API/field to address a second project — mis-target is unrepresentable (AC-W1-C5).
3. Multi-LLM N:1: two distinct CLIs attach the *same* slug, share the store, attributed by `session_id` (AC-W1-C7, AC-W2-R5) — no per-LLM path.
**Coverage Requirement**: Source assertion that project identity has no payload carrier; unrepresentability, not runtime rejection.

### R-07: First-boot credential idempotence (High)
**Severity** High · **Likelihood** Med · **Impact** Restart regenerates cert/token; all pinned clients and the emitted bundle silently break.
**Test Scenarios**:
1. Boot twice; assert token AND cert+key are loaded, not regenerated (byte-identical) across restart (AC-W1-S3).
2. Operator override (own token / own cert mounted `:ro`) is honored, not overwritten (FR-A3).
3. Concurrent/partial-write first boot does not corrupt or duplicate credentials.
**Coverage Requirement**: load-not-regenerate proven across restart; override honored.

### R-08: Production cert params (High)
**Test Scenarios**: SAN set = `UNIMATRIX_PUBLIC_URL` host + `localhost`/`127.0.0.1`/`0.0.0.0` (FR-A4); defined validity period; private key mode `0600`; assert none inherited from the test-only helper.
**Coverage Requirement**: SAN/validity/key-mode asserted as production values; key `0600` on disk.

### R-09: C3 three-consumer derivation (Medium)
**Test Scenarios**: One `derive_public_url()`; bundle base-url, `allowed_hosts` default, cert SAN all read from it (AC-CT-C3); assert **bundle host ∈ cert SANs** (AC-W1-S9, SR-10); unset → loud `<EDIT-ME>` placeholder + permissive-with-warning; socket auto-detect absent.
**Coverage Requirement**: Single derivation; host∈SAN invariant test; no socket auto-detect.

### R-10: Enterprise seam preservation (Medium)
**Test Scenarios**: Source assertion `TlsConfig.enabled`, `BearerValidator` trait, slug-as-scope present and degenerate-but-documented (AC-CT-C6, NFR-08/09); TLS not hardcoded such that the proxy seam disappears.
**Coverage Requirement**: Seam interfaces exist (not just behavior); documented-but-degenerate per session_key precedent.

### R-11: Fail-loud provisioning (Medium)
**Test Scenarios**: Unwritable `/data` (UID-65532 mismatch) → actionable error, no panic, no `.unwrap()` (AC-W1-S8, FR-A9, NFR-03); missing-cred path errors loud.
**Coverage Requirement**: Negative provisioning test; grep non-test code for `.unwrap()` in provisioning paths = none.

### R-12: Hard invariants (Medium)
**Test Scenarios**: Only the TLS port published — plaintext connect fails (AC-W1-S2); token absent from logs/image layers/all DB schemas (AC-W1-S5, NFR-06); install `< 250 KB` hard gate (AC-W1-C3, NFR-01); no `/metrics`, no unauth endpoint beyond `GET /health` (AC-W1-S6).
**Coverage Requirement**: Each invariant a hard pass/fail gate.

### R-13: OQ-C additive addressing (Low)
**Test Scenarios**: `[[projects]]`-absent `/v1/tools/…` unchanged (AC-W2-R2); Wave-2 `/{slug}` additive — no Wave-1 client re-init (AC-CT-C4).
**Coverage Requirement**: Wave-1 clients unchanged after Wave-2 router lands.

## Integration Risks

The whole feature is integration-shaped. Highest-risk seams: (1) **router edge → tool dispatch** — the resolved `Arc<Store>` must be the sole write capability threaded from the edge; test that no downstream `McpAdapter`/`UnimatrixServer` path re-derives a store (FR-X3). (2) **Rust serve cert ↔ JS pin** — the only data crossing the server/client boundary is C1+C2; parity fixtures (R-02) own this. (3) **`client-bundle` sync pre-tokio subcommand** reads the same token+cert the async listener serves — test the bundle's fingerprint equals the *served* cert, not just an on-disk one. (4) **Wave-1 seam → Wave-2 router injection** (R-01) is the load-bearing integration seam of the umbrella.

## Edge Cases

- Empty / max-length (63-char) / single-char slug; slug == reserved word (`tools`, `health`, `observe`, `v1`).
- Bundle at exactly the length cap; bundle with valid base64url but invalid JSON.
- `UNIMATRIX_PUBLIC_URL` unset, with port, with path, IPv6 literal.
- Cert exactly at validity boundary; key file present but unreadable.
- First boot racing two container starts on one volume.
- Windows HTTPS-remote only (no local mode) — assert no local-mode code path reachable on Windows.
- N clients (≥2 LLM CLIs) attaching one slug concurrently; same-project multi-connection vs cross-project fan-out (the latter unrepresentable).

## Security Risks

Untrusted inputs and blast radius per component accepting external data:
- **Slug parser (C5/C4)** — untrusted operator/client URL segment. Blast radius: filesystem escape → any project's volume. Mitigation R-03 (allowlist pre-filesystem). Fix-before-merge.
- **Bundle parser (C1)** — untrusted pasted blob at client init. Blast radius: client crash/DoS. Mitigation R-05 (strict schema + length cap).
- **Bearer token** — secret in transit/at rest. Blast radius: full cloud access if leaked. Mitigation R-12 (never in DB/log/image; file `0600`).
- **Cert/key** — trust root. Blast radius: impersonation if key world-readable or SANs over-broad. Mitigation R-08 (key `0600`, bounded SANs).
- **Published port** — the only external network surface. Blast radius: unauth endpoint = unauthenticated access. Mitigation R-12 (TLS-only, `/health` the sole unauth route, no `/metrics`).
- **Cross-project write (integrity, not access-control)** — a client bound to B writing into A's hash chain. Blast radius: permanent unrollbackable corruption of A. Mitigation R-06 (unrepresentable at transport).

## Failure Modes

- Unwritable `/data` → loud, actionable error; no panic, no `.unwrap()`; exit non-zero (R-11).
- Unregistered slug on attach → client errors, creates no store (AC-W1-C4).
- Cert/fingerprint mismatch → client refuses connection with a diagnosable error (R-02).
- `ProjectKey::Slug` under Wave-1 `DefaultResolver` → `RouteError::UnknownProject`, never the default store (R-01).
- Malformed bundle → rejected at parse, client process survives (R-05).
- Cert rotation → re-bundle + re-`init` documented path; old fingerprint cleanly rejected (AC-CT-ROT).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-08 | Cert params specified as production requirements (SAN/validity/`0600`), not inherited from the test helper. |
| SR-02 | R-02 | Single Rust oracle + cross-stack parity fixtures; no hand-written JS golden. |
| SR-03 | R-12 (install size) | `< 250 KB` hard acceptance gate; pinning via custom `checkServerIdentity`, zero deps. |
| SR-04 | R-10 | Enterprise seams (`TlsConfig`/`BearerValidator`/slug-as-scope) held as explicit degenerate-but-documented interfaces. |
| SR-05 | R-13 | OQ-C resolved (ADR-005) to the additive `/v1/tools/…` alias; Wave-1 clients unchanged. |
| SR-06 | R-06 | 1:1 enforced at transport — identity has no payload carrier; mis-target unrepresentable. |
| SR-07 | R-01 | `resolve_store` is the single funnel from day one; Wave-1 store served *through* it; router injected behind one trait. |
| SR-08 | R-04 | One seam, two resolvers; local-install regression test in the Wave-1 set. |
| SR-09 | R-03, R-05 | Slug allowlist + bundle schema validation at the edge — both fix-before-merge security criteria. |
| SR-10 | R-09 | Single `derive_public_url()`; bundle host ∈ cert SAN test. |
| SR-11 | R-11 | Explicit fail-loud provisioning paths; no `.unwrap()` in provisioning. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 |
| High | 7 (R-02–R-08) | 21 |
| Medium | 4 (R-09–R-12) | 9 |
| Low | 1 (R-13) | 2 |
