# vnc-034 Acceptance Criteria Map

> Every AC from SPECIFICATION §Acceptance Criteria (which derives the umbrella-level SCOPE acceptance into per-wave, testable AC-IDs). Grouped by delivery wave (Wave 1 = #726 + #725; Wave 2 = #727) and cross-wave contracts. Verification methods: `test` (cargo test / JS test), `manual`, `file-check`, `grep`, `shell`.
>
> Regenerated 2026-06-11 after the four locked decisions: adds **AC-W1-S5b** (client-bundle stderr echo, token redacted) and **AC-W1-C10** (4 KB raw-string cap before decode), strengthens **AC-W1-C9** (strict schema = load-bearing guard) and **AC-CT-ROT** (cert-rotation runbook as a required deliverable + diagnosable stale-fingerprint rejection).

## Wave 1 — Server (Group A, #726)

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|---------------------|---------------------|--------|
| AC-W1-S1 | `docker compose up` with no operator config serves HTTPS on the TLS port, reachable from a sibling container by service name | test | Integration: sibling-container HTTPS request to `https://<service>:8443/health` succeeds | PENDING |
| AC-W1-S2 | No plaintext port is published | shell | Inspect published ports (`docker compose config` / runtime); assert only TLS port; plaintext connect to it fails | PENDING |
| AC-W1-S3 | First boot auto-generates token + cert; on restart they are loaded, not regenerated; operator override works | test | Boot twice, assert token+cert+key byte-identical across restarts; mount override token/cert `:ro`, assert used not overwritten | PENDING |
| AC-W1-S4 | `client-bundle` emits `{base-url, token, sha256:fingerprint}`; the fingerprint **equals** SHA-256 of the served leaf DER (C2 wire-contract equality) | test | Independently SHA-256 the served leaf DER; assert byte-equality with the bundle `fp`. Cross-stack parity fixture | PENDING |
| AC-W1-S5 | Token never logged, imaged, committed, or stored in any DB | grep | Grep logs + image layers for the token; assert absent. Schema audit: no token column in any DB | PENDING |
| AC-W1-S5b | `client-bundle` echoes decoded base-url + cert-fingerprint to **stderr** (human-readable); the token appears nowhere in stdout, stderr, or logs; stdout carries only the opaque bundle blob (FR-A5b, NFR-06, NFR-13) | test | Run `client-bundle`; assert (a) base-url + fingerprint present on stderr, (b) token string absent from stdout AND stderr AND any log output, (c) stdout is the opaque `unimatrix-bundle:…` blob only | PENDING |
| AC-W1-S6 | No unauthenticated endpoint on the published port beyond `GET /health`; no `/metrics` endpoint | test | Probe endpoints unauthenticated; assert only `/health` responds; assert `/metrics` absent | PENDING |
| AC-W1-S7 | nan-014 hardening preserved (UID 65532, distroless, ORT pinning, `/shared` `:ro`) | shell | Image inspection / container runtime assertions for UID, distroless base, ORT pin, `/shared` mount mode | PENDING |
| AC-W1-S8 | Host bind-mounted `/data` writable by UID 65532 (documented); binary fails loud-and-actionable if not | test | Mount unwritable `/data`; assert actionable error, no panic, no `.unwrap()`, non-zero exit | PENDING |
| AC-W1-S9 | `UNIMATRIX_PUBLIC_URL` feeds base-url + allowed_hosts + SAN from one derivation; bundle host ∈ cert SAN (C3, SR-10) | test | Set the knob; assert all three consumers reflect it; assert bundle host ∈ cert SANs | PENDING |

## Wave 1 — Client (Group B, #725)

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|---------------------|---------------------|--------|
| AC-W1-C1 | `init --remote <bundle>` produces a working pure-JS client on Linux, macOS-arm, Windows, no native binary | test | Per-OS integration: init then a live knowledge call over HTTPS | PENDING |
| AC-W1-C2 | Client pins the exact server cert by the C2 fingerprint; a wrong/changed cert is rejected (C2, SR-02) | test | Connect with matching cert (succeeds); connect with mismatched cert (rejected) | PENDING |
| AC-W1-C3 | Remote install `< 250 KB` (hard gate) | shell | Measure install footprint; assert `< 250 KB` | PENDING |
| AC-W1-C4 | Client attaches to an existing slug; never auto-creates a project; errors if the slug is unregistered (C5) | test | Attach unregistered slug → error; attach registered slug → success; assert no store created by client | PENDING |
| AC-W1-C5 | Resulting client is bound to exactly one project — no multi-project connection/multiplexing is representable (C5, SR-06) | test | Source assertion: client has no API/field to target a second project; mis-target unrepresentable | PENDING |
| AC-W1-C6 | Skills copied; CLAUDE.md knowledge block NOT appended; `/unimatrix-init` pointer printed | file-check | Inspect post-init filesystem (skills present, no CLAUDE.md block) and stdout (pointer printed) | PENDING |
| AC-W1-C7 | Multi-LLM proven as the N-clients-one-project path: ≥2 distinct CLIs attach the same slug, connect identically over HTTPS; no per-LLM code path | test | Two distinct CLIs attach one slug; both connect; assert single shared client code path | PENDING |
| AC-W1-C8 | Onboarding (install → ingest bundle → connect) is fast and safe for a solo developer | manual | End-to-end timed walkthrough; no manual cert handling beyond the bundle | PENDING |
| AC-W1-C9 | Bundle parser validates against a schema and rejects malformed bundles at the trust boundary; the **strict schema reject is the load-bearing guard** (missing / extra / wrong-type field) (SR-09, FR-B9) | test | Feed malformed / truncated / missing-field / extra-field / wrong-type bundles; assert rejection, no crash | PENDING |
| AC-W1-C10 | The 4 KB byte-length cap is enforced on the RAW pasted string **BEFORE** base64url-decode and **BEFORE** JSON-parse (length-check-first, belt-and-suspenders) (SR-09, ADR-001, FR-B9) | test | Feed an over-cap raw string; assert rejection by the length check **prior to** any decode/parse (e.g. an over-cap string that is not valid base64url still rejects on length, not on decode error); assert the cap rejects before schema validation runs | PENDING |

## Wave 1 — Isolation seam (C4, required in Wave 1)

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|---------------------|---------------------|--------|
| AC-W1-X1 | All store access funnels through `resolve_store`; the Wave-1 single store is served **through** the seam, not around it (SR-07) | test | Source assertion: single resolution funnel; Wave-1 resolver returns the one store; zero bypass call sites | PENDING |
| AC-W1-X2 | Seam reduces identically to the local-UDS path-hash store; local single-project install exercises the **same** code path (NFR-10, SR-08) | test | Local-install regression test in the Wave-1 set (not deferred): local UDS resolves the path-hash store through the seam | PENDING |
| AC-W1-X3 | Mis-targeting unrepresentable: project identity is transport-derived; no request payload field names a project (C5, FR-X2, SR-06) | test | Inspect request types; assert no project-naming field; transport is the sole identity source | PENDING |

## Wave 2 — Routing (Group C, #727)

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|---------------------|---------------------|--------|
| AC-W2-R1 | `/v1/{slug}/…` routes to the per-slug store | test | Request two slugs; assert each lands in its own store | PENDING |
| AC-W2-R2 | `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged (single-project backward-compat; C4 default alias, OQ-C) | test | Run with no `[[projects]]`; assert current behavior, zero change | PENDING |
| AC-W2-R3 | Per-slug isolation: no cross-project read or write | test | Write into slug A; assert unreadable/unwritable from slug B's path | PENDING |
| AC-W2-R4 | Register / list / delete lifecycle works | test | CLI exercises each; assert store created / listed / removed | PENDING |
| AC-W2-R5 | N clients on one slug share the store, attributed by `session_id`; each client stays bound to that one slug (C5) | test | Multiple clients on one slug; assert shared store + correct per-session attribution | PENDING |
| AC-W2-R6 | Slug allowlist rejects path-traversal / encoded separators; no escape from `/data/.unimatrix/{slug}/` (SR-09 — security, fix-before-merge) | test | Feed `../`, encoded `/`/`%2e`/`%2f`, over-length, uppercase, empty slugs; assert rejection + no filesystem escape | PENDING |

## Cross-wave Contracts (both waves)

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|---------------------|---------------------|--------|
| AC-CT-C2 | Fingerprint format `sha256:<lowercase-hex>` identical server↔client (C2) | test | Cross-stack parity fixture (Rust oracle): server-computed == client-pinned for the same DER; reject uppercase / PEM-derived | PENDING |
| AC-CT-C3 | `UNIMATRIX_PUBLIC_URL` feeds base-url + allowed_hosts + SAN from a single derivation function (C3) | test | Unit-test `derive_public_url`; assert all three consumers read from it; bundle host ∈ SAN; no socket auto-detect | PENDING |
| AC-CT-C4 | Store access funnels through `resolve_store`; the route shape admits `/{slug}` **additively** (Wave 2 adds no bypass, re-points no Wave-1 client) (C4, OQ-C/OQ-D) | test | Resolver-swap test: Wave-2 router injects into the existing seam; assert Wave-1 clients unchanged | PENDING |
| AC-CT-C6 | Token authorizes, slug scopes, cert secures — three concerns not collapsed; `BearerValidator` / `TlsConfig` / slug seams intact for enterprise (C6, SR-04, NFR-09) | test | Source assertion: seam interfaces present and documented-but-degenerate (session_key precedent) | PENDING |
| AC-CT-ROT | A cert-rotation runbook is a **required deliverable** (documented operator procedure: rotate cert → re-`client-bundle` → re-`init` clients); rotating **without** re-bundling surfaces a clear, diagnosable fingerprint-mismatch error directing the operator to re-bundle (FR-A11) | file-check + test | (1) Assert the runbook doc exists and ships as a deliverable. (2) Rotate cert; re-bundle; re-init client; assert reconnect succeeds. (3) Rotate cert WITHOUT re-bundling; attempt reconnect; assert a clear/diagnosable fingerprint-mismatch error (not opaque), naming expected-vs-presented `sha256:` and pointing to re-bundle | PENDING |
