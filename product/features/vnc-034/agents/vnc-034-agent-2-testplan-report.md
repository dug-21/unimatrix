# Agent Report — vnc-034-agent-2-testplan (Stage 3a, Test Plan Design)

**Phase:** Test Plan Design (Stage 3a) · **Scope:** Wave 1 only (#726 server + #725 client + C1/C2 build-first contract). Wave 2 (#727) explicitly excluded.

## Deliverables (all absolute paths)
- /workspaces/unimatrix/product/features/vnc-034/test-plan/OVERVIEW.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/cert-provisioner.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/fingerprint-computer.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/public-url.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/bundle-codec.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/slug-router.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/default-resolver.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/remote-client.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/container-posture.md
- /workspaces/unimatrix/product/features/vnc-034/test-plan/cert-rotation-runbook.md

NOT produced (Wave 2, out of scope): project-router.md, project-registry.md.

## Risk → component coverage (Wave 1)
| Risk | Pri | Component plan(s) | AC-IDs |
|------|-----|-------------------|--------|
| R-01 seam swap | Critical | slug-router, default-resolver | AC-W1-X1, AC-CT-C4 |
| R-02 fp parity | High | fingerprint-computer, bundle-codec, remote-client | AC-CT-C2, AC-W1-S4, AC-W1-C2 |
| R-03 slug guard (parse edge — Wave-1) | High | slug-router | (AC-W2-R6 routing = Wave 2) |
| R-04 local/cloud parity | High | default-resolver | AC-W1-X2 (NFR-10) |
| R-05 bundle parser | High | bundle-codec, remote-client | AC-W1-C9, AC-W1-C10 |
| R-06 1:1 transport | High | slug-router, remote-client | AC-W1-X3, AC-W1-C5 |
| R-07 idempotence | High | cert-provisioner | AC-W1-S3 |
| R-08 cert params | High | cert-provisioner | AC-W1-S3, AC-W1-S9 |
| R-09 C3 derivation | Med | public-url | AC-W1-S9, AC-CT-C3 |
| R-10 enterprise seams | Med | slug-router, cert-provisioner | AC-CT-C6 |
| R-11 fail-loud | Med | cert-provisioner, container-posture | AC-W1-S8 |
| R-12 hard invariants | Med | container-posture, remote-client, cert-provisioner | AC-W1-S2/S5/S5b/S6, AC-W1-C3 |
| R-13 additive addressing | Low | slug-router | AC-CT-C4 (W1 half) |
| rotation | — | cert-rotation-runbook | AC-CT-ROT |

Every Wave-1 AC-ID (AC-W1-S1..S9, S5b; C1..C10; X1..X3) and the four cross-wave contracts Wave 1 must satisfy (AC-CT-C2, C3, C4, C6, ROT) maps to ≥1 concrete test. Wave-2 AC-IDs (AC-W2-R1..R6) not planned.

## Critical Wave-1 concerns — how addressed
- **C2 parity (SR-02):** corpus at `crates/unimatrix-server/tests/fixtures/c1c2-parity/{fingerprint-golden,bundle-golden}.json`, emitted by the single Rust oracle (`fingerprint-computer` regen test), consumed byte-identically by Rust (#726) and JS (#725) tests. JS golden never hand-written. Stability regression test re-derives every row in normal CI → divergence fails CI not connect. Synthetic tokens avoid `sk-`-style scanner trips (lesson #4792).
- **C1 guard ordering:** `bundle-codec.md` has an explicit `test_bundle_length_cap_before_decode` asserting an over-cap NON-base64url string rejects on the **length-cap error variant**, not a decode/parse error — proving the 4 KB cap ran before decode+parse. Strict-schema reject (missing/extra/wrong-type/unknown-major) is the load-bearing guard.
- **AC-W1-X2 / NFR-10:** local-install regression test is IN the Wave-1 set (`default-resolver.md`), asserting local UDS resolves the path-hash store through the same `resolve_store` seam — a call-graph assertion, not deferred.
- **AC-W1-S5b / NFR-06:** `test_client_bundle_token_absent_from_stdout_and_stderr` + stdout-opaque-blob-only + stderr-base-url+fp-only.
- **AC-CT-ROT:** runbook file-check + content-3-steps; rotate-without-rebundle → diagnosable mismatch naming expected-vs-presented `sha256:`; rotate-with-rebundle → reconnect succeeds.

## Integration suite plan
- **infra-001 stdio harness = regression baseline only.** It exercises the binary over stdio MCP, NOT HTTPS — none of its 9 suites cover TLS/bundle/pin. Mandatory: `smoke` (min gate); run `tools`, `protocol`, `lifecycle`, `edge_cases` to prove the `SlugRouter`/cert/`client-bundle` insert didn't regress stdio dispatch. No new tests added to infra-001 stdio suites for Wave 1.
- **New Wave-1 integration tests (HTTPS/client/container, live with the feature):** sibling-container HTTPS `/health` (AC-W1-S1); plaintext-port refusal (S2); served-cert==bundle-fp (S4); boot-twice idempotence + override (S3); unwritable `/data` fail-loud (S8); token-absent-everywhere (S5/S5b); only-`/health`-unauth + no-`/metrics` (S6); per-OS `init --remote` matrix Linux/macOS-arm/Windows (C1, C7); pinned reconnect + mismatch reject (C2); install-size gate (C3); rotate-without-rebundle (AC-CT-ROT).

## Open questions
1. JS test runner choice (existing hook-client harness vs node `--test`) for parity-corpus consumption and `init --remote` — Stage 3b/3c decides; plan only requires the JS test reads the committed corpus, never hand-writes expected values.
2. Per-OS CI runner availability (macOS-arm, Windows). If absent, AC-W1-C1 per-OS coverage degrades to manual walkthrough + platform-independent pin/parse unit tests — flagged in coverage report, not silently dropped.
3. Corpus cross-repo reach — JS imports the Rust-tree fixture directly vs a CI-verified byte-identical copy. Either acceptable; re-authoring is not.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get -- surfaced ADR-001 (#4954 bundle wire form + guard ordering), ADR-002 (#4948 fingerprint single-oracle/parity), ADR-005 (#4949 default alias), ADR-006 (#4952 wave-to-issue + C1/C2 build-first); lesson #3386 (3b skips edge-case tests — drove the "edge cases assigned, not optional" discipline in OVERVIEW §1 and each component file); lesson #4792 (synthetic fixtures must not match real-provider prefixes — applied to the parity-corpus token values).
- Stored: nothing novel to store -- the test-plan patterns applied (single-oracle parity fixtures, source-grade unrepresentability assertions, guard-order-proving tests) are already captured in the surfaced ADRs/lessons (#4766 parity pattern, #3386 edge-case lesson). No new reusable testing technique emerged that isn't already in Unimatrix.
