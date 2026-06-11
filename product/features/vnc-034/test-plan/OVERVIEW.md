# vnc-034 Test Plan — OVERVIEW (Wave 1 only)

> Wave 1 = single-project HTTPS serving (#726) + pure-JS remote client (#725) + the build-first C1/C2 connection-contract sub-deliverable. **Wave 2 (#727 — ProjectRouter, ProjectRegistry, slug resolver, AC-W2-R*) is OUT OF SCOPE for this test plan.** Where a component has a Wave-2 dimension (SlugRouter slug resolution, ProjectSlug allowlist), only the Wave-1 surface (route-shape parse, `RouteError::UnknownProject`, the `TryFrom` parse-edge guard the seam already needs) is planned here.

Rooted in `RISK-TEST-STRATEGY.md` (R-01..R-13), `ACCEPTANCE-MAP.md` (Wave-1 + cross-wave AC-IDs), `ARCHITECTURE.md`, and ADR-001/002/005/006.

---

## 1. Test Strategy

Three layers, in increasing cost/scope:

| Layer | Tooling | What it proves |
|-------|---------|----------------|
| **Unit** | `cargo test --workspace` (Rust), JS test runner (node `--test` / existing hook-client harness) | Per-function correctness: fingerprint hex/casing, bundle encode/decode, guard ordering, `derive_public_url` derivation, `ProjectSlug::TryFrom`, `DefaultResolver` resolution, fail-loud provisioning. The bulk of Wave-1 coverage. |
| **Cross-stack parity** | committed fixture corpus (Rust oracle → JSON) consumed by both a Rust test and a JS test | C1/C2 server↔client byte-equality. The umbrella's reason to exist (SR-02). |
| **Integration / system** | sibling-container HTTPS (docker compose), per-OS client `init --remote`, infra-001 stdio smoke as a regression baseline | End-to-end: served-cert == bundle fp, pinned reconnect, plaintext refusal, fail-loud `/data`, rotation-without-rebundle diagnosable error. |

**Guiding principle (R-01, R-06):** the highest-priority risks are proven by **source-grade / unrepresentability assertions**, not only runtime behavior. "No call site obtains `Arc<Store>` outside `resolve_store`" and "no request payload field names a project" are structural assertions (a compile-time/source-inspection test), because the failure mode is silent corruption, not a catchable error.

**Edge-case discipline (lesson #3386):** every edge case in `RISK-TEST-STRATEGY.md §Edge Cases` that applies to Wave 1 is assigned to a component file below and is NOT optional — Stage 3b/3c implements them, not just the happy path.

---

## 2. Risk → Test Mapping (Wave 1)

| Risk | Pri | Primary component plan(s) | Key AC-IDs | Coverage target |
|------|-----|---------------------------|-----------|-----------------|
| R-01 deferred seam swap | **Critical** | slug-router, default-resolver | AC-W1-X1, AC-CT-C4 | single-funnel source assertion + resolver-swap test; `ProjectKey::Slug`→`UnknownProject` |
| R-02 fingerprint parity | High | fingerprint-computer, bundle-codec | AC-CT-C2, AC-W1-S4, AC-W1-C2 | Rust-oracle parity fixture; served-cert==bundle-fp; reject on mismatch/uppercase/PEM |
| R-03 slug allowlist | High | slug-router | (parse-edge guard; AC-W2-R6 is Wave-2) | `ProjectSlug::TryFrom` traversal corpus rejected pre-filesystem — **the guard itself is Wave-1**, even though slug *routing* is Wave-2 |
| R-04 local/cloud parity | High | default-resolver, slug-router | AC-W1-X2 (NFR-10) | local-install regression test IN the Wave-1 set |
| R-05 bundle parser | High | bundle-codec, remote-client | AC-W1-C9, AC-W1-C10 | malformed/truncated/oversized corpus; length-cap-before-decode |
| R-06 1:1 at transport | High | slug-router, remote-client | AC-W1-X3, AC-W1-C5 | unrepresentability source assertion; no project-naming payload field |
| R-07 credential idempotence | High | cert-provisioner | AC-W1-S3 | boot-twice byte-identical; override honored |
| R-08 production cert params | High | cert-provisioner | AC-W1-S3, AC-W1-S9 | SAN set / validity / key `0600`; not test-helper defaults |
| R-09 C3 derivation | Med | public-url | AC-W1-S9, AC-CT-C3 | single `derive_public_url`; host ∈ SAN; no socket auto-detect |
| R-10 enterprise seams | Med | slug-router, cert-provisioner | AC-CT-C6 | `TlsConfig`/`BearerValidator`/`StoreResolver` present, degenerate-but-documented |
| R-11 fail-loud provisioning | Med | cert-provisioner, container-posture | AC-W1-S8 | unwritable `/data` → actionable error, no panic/`.unwrap()` |
| R-12 hard invariants | Med | container-posture, remote-client, cert-provisioner | AC-W1-S2/S5/S5b/S6, AC-W1-C3 | TLS-only port; token absent; `<250 KB`; only `/health` unauth |
| R-13 additive addressing | Low | slug-router | AC-CT-C4 (Wave-1 half) | `/v1/tools/...`→Default shape stable; `/{slug}` parses inert |
| (rotation) | — | cert-rotation-runbook | AC-CT-ROT | runbook file-check + rotate-without-rebundle diagnosable mismatch |

Wave-2-only risk facets explicitly NOT planned here: per-slug store isolation (AC-W2-R1/R3), register/list/delete lifecycle (AC-W2-R4), N-clients-on-a-live-slug data sharing (AC-W2-R5), full slug-routing escape proof (AC-W2-R6 routing half). The slug **parser** guard (R-03) IS planned because the seam's `ProjectSlug::TryFrom` parse edge is Wave-1 work (it must reject before Wave 2 can route).

---

## 3. C1/C2 Parity Fixture — Location & Ownership (LOAD-BEARING)

The single most important Wave-1 test asset. Per ADR-002 / ADR-006 / SR-02: **the JS golden is NEVER hand-written** — it is emitted by the one Rust oracle and committed, then consumed identically by the server-side Rust test and the client-side JS test. If #726 and #725 each computed their own expected value, the contract could silently diverge at connect; the committed corpus makes divergence fail CI, not user connect.

### Fixture corpus location
```
crates/unimatrix-server/tests/fixtures/c1c2-parity/
  fingerprint-golden.json     # C2: array of { der_b64, fp } rows from the Rust oracle
  bundle-golden.json          # C1: array of { fields:{v,base_url,token,fp}, wire } rows
  README.md                   # "GENERATED — DO NOT HAND-EDIT. Regenerate via <test name>."
```
(Rust-side canonical home so the oracle test writes it in-tree. The JS test reads the same committed path via a relative import from `lib/hook-client/`. If the JS test runner cannot reach across the repo, a build step copies — never re-authors — the corpus into a JS-visible fixtures dir; the copy is byte-identical and CI-verified equal to the source.)

### Generation (the oracle)
- A `#[test]`-gated (or `--ignored` regen) Rust test in `fingerprint-computer` / `bundle-codec` plans calls `fingerprint_leaf_der(der)` and the bundle encoder over a fixed input set (synthetic DERs + canonical field sets) and writes the golden JSON. **Synthetic token values in fixtures MUST NOT match real-provider prefixes** (lesson #4792 — no `sk-`-style tokens; use plainly-synthetic 64-hex like `aaaa…`/`0123…`) so secret scanners never trip on branch history.
- Re-running the oracle on an unchanged implementation produces a byte-identical corpus (the regression guard).

### Consumption
- **Server (Rust, #726):** a test asserts `fingerprint_leaf_der(row.der)` == `row.fp` for every row, and bundle-encode(`row.fields`) == `row.wire`.
- **Client (JS, #725):** a test decodes each `row.wire` → asserts fields == `row.fields`, and computes the pin over `row.der_b64` → asserts == `row.fp` (the exact compute path `checkServerIdentity` uses on `cert.raw`).
- Both sides reading one corpus = parity proven by construction (R-02 closed).

### Ownership
- Oracle + corpus authored by the **C1/C2 build-first sub-deliverable** (lands before either half depends on it — ADR-006).
- `fingerprint-computer.md` and `bundle-codec.md` own the per-row assertions; this OVERVIEW owns the corpus location/contract.

---

## 4. Integration Harness Plan (Wave 1)

### 4.1 infra-001 (existing stdio harness) — regression baseline only
The infra-001 harness exercises the `unimatrix` binary **over stdio MCP**, NOT over HTTPS. None of its 9 suites currently cover TLS, the connection bundle, cert pinning, or the HTTP listener. Its role for vnc-034 is a **regression baseline**: prove that adding the `SlugRouter` seam, cert provisioning, and the `client-bundle` subcommand did not break existing stdio tool dispatch.

| infra-001 suite | Run? | Why |
|-----------------|------|-----|
| `smoke` (`-m smoke`) | **YES — mandatory minimum gate** | Any change at all. Proves stdio dispatch + store + restart still green after the seam insert. |
| `tools`, `protocol` | YES | Server tool logic + dispatch path now passes through `SlugRouter`→`resolve_store`→`McpAdapter`; assert tool surface unchanged. |
| `lifecycle` | YES | Store/retrieval + restart persistence — the seam must thread the same `Arc<Store>`; restart must still load (interacts with R-07 idempotence in spirit). |
| `edge_cases` | YES | Store behavior unchanged through the new layer. |
| `volume`, `confidence`, `contradiction`, `adaptation`, `security` | OPTIONAL | Not touched by Wave 1; run if the real diff reaches store/scan logic. `security` is stdio content-scanning, NOT TLS/bundle — does not cover Wave-1 security surface. |

No new tests are added to infra-001 stdio suites for Wave 1 — the Wave-1 security/transport surface is not stdio-shaped. (If a future infra change adds an HTTPS harness, that is a separate GH Issue per USAGE-PROTOCOL §"Adding New Tests".)

### 4.2 New Wave-1 integration tests (HTTPS / client / container — NOT in infra-001 stdio)

These validate behavior only visible end-to-end. They are **new** and live with the feature, not in infra-001:

| New integration test | AC-ID | Where | Scenario |
|----------------------|-------|-------|----------|
| **sibling-container HTTPS reachability** | AC-W1-S1 | docker compose test (server container + sibling client/curl container) | `docker compose up` no operator config → `GET https://<service>:8443/health` from a sibling succeeds over TLS. |
| **plaintext-port refusal** | AC-W1-S2 | compose config + runtime probe | only the TLS port published; plaintext connect to it fails. |
| **served-cert == bundle fp** | AC-W1-S4 | server integration test | independently SHA-256 the leaf DER served on `:8443`; assert byte-equal to the bundle's `fp` (proves the bundle pins the *served* cert, not a stale on-disk one). |
| **boot-twice idempotence** | AC-W1-S3 | container/integration test | boot, capture token+cert+key bytes; restart; assert byte-identical; mount override `:ro`, assert honored. |
| **unwritable `/data` fail-loud** | AC-W1-S8 | container test | mount unwritable `/data` (UID mismatch) → actionable error, non-zero exit, no panic. |
| **token absent everywhere** | AC-W1-S5, S5b | shell/grep + run-capture | grep logs + image layers + stderr for the token → absent; stdout = opaque blob only; stderr = base-url+fp only. |
| **only `/health` unauth** | AC-W1-S6 | HTTPS probe | unauth probe of endpoints; only `/health` answers; `/metrics` absent. |
| **per-OS client `init --remote`** | AC-W1-C1, C7 | CI matrix (Linux, macOS-arm, Windows) | `init --remote <bundle>` then a live knowledge call over HTTPS; two distinct CLIs attach one bundle → identical code path. |
| **pinned reconnect / mismatch reject** | AC-W1-C2 | client integration | matching cert → connect; mismatched/changed cert → rejected with diagnosable error. |
| **install size gate** | AC-W1-C3 | shell | measure install footprint < 250 KB (hard gate). |
| **rotate-without-rebundle** | AC-CT-ROT | client integration | rotate server cert, do NOT re-bundle, reconnect → clear fingerprint-mismatch error naming expected-vs-presented `sha256:` + "re-bundle". Then re-bundle + re-init → reconnect succeeds. |

**Per-OS note:** macOS-arm and Windows clients are HTTPS-remote-only (no local UDS mode). The Windows path must assert no local-mode code branch is reachable (edge case in §Edge Cases). Where a live per-OS CI runner is unavailable, the per-OS assertion degrades to a documented manual walkthrough (AC-W1-C8 is `manual` by spec) + a platform-independent unit test of the pin/parse logic; flag in the coverage report rather than silently dropping.

### 4.3 Cross-component dependencies
- `cert-provisioner` produces the leaf DER that `fingerprint-computer` hashes that `bundle-codec` emits that `remote-client` pins — the C2 chain. Test at each boundary AND end-to-end (served-cert==fp==pinned).
- `public-url` feeds `cert-provisioner` (SAN), `bundle-codec` (base_url), and `allowed_hosts` — the `host ∈ SAN` invariant (AC-W1-S9) is a cross-component assertion owned by `public-url.md`.
- `slug-router` + `default-resolver` are the R-01 seam pair — the swap test spans both.

---

## 5. Per-Component Test Plan Files (Wave 1)

| File | Component | Lead risks |
|------|-----------|-----------|
| `cert-provisioner.md` | `load_or_generate_cert` | R-07, R-08, R-11 |
| `fingerprint-computer.md` | `fingerprint_leaf_der` | R-02 (oracle side) |
| `public-url.md` | `derive_public_url` | R-09 |
| `bundle-codec.md` | `run_client_bundle` (Rust) + JS decoder | R-02, R-05 |
| `slug-router.md` | `SlugRouter` + `StoreResolver`/`ProjectKey`/`ProjectSlug` parse edge | R-01, R-03, R-06, R-13 |
| `default-resolver.md` | `DefaultResolver` | R-01, R-04 |
| `remote-client.md` | `init --remote` (pure JS) | R-02, R-05, R-06, R-12 |
| `container-posture.md` | Dockerfile / compose / env | R-11, R-12, AC-W1-S1/S2/S7 |
| `cert-rotation-runbook.md` | runbook doc + diagnosable rejection | AC-CT-ROT |

NOT produced (Wave 2): `project-router.md`, `project-registry.md`.

---

## 6. Open Questions
- **JS test runner choice** for the parity-corpus consumer and `init --remote` tests (existing hook-client harness vs node `--test`) — Stage 3b/3c picks; the plan only requires the JS test reads the committed corpus, never hand-writes expected values.
- **Per-OS CI availability** (macOS-arm, Windows runners). If absent, AC-W1-C1 per-OS coverage is partial-by-manual (flagged in coverage report), not a silent gap.
- **Corpus cross-repo reach** — whether the JS test imports the Rust-tree fixture directly or via a CI-verified byte-identical copy. Either is acceptable; re-authoring is not.
