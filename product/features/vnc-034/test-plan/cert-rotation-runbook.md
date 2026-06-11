# Test Plan — Cert-Rotation Runbook (required deliverable, FR-A11)

> A short operator doc shipped with the container: **rotate cert → re-run `client-bundle` → re-`init` clients**. NOT rotation tooling — the existing `client-bundle` + `init` flow IS the rotation flow. Paired with a clean/diagnosable stale-fingerprint rejection. **Lead AC: AC-CT-ROT.**

## AC-IDs covered
AC-CT-ROT (runbook exists as a deliverable + rotate-without-rebundle is diagnosable + rotate-with-rebundle reconnects).

---

## Deliverable existence (file-check)
- `test_cert_rotation_runbook_exists` — assert the runbook doc exists and ships with the container (a concrete file path under the container docs / shipped assets, referenced from the image or compose docs). It is a **required deliverable** — its absence is an AC failure, not a nice-to-have.
- `test_runbook_documents_three_steps` — the runbook content covers exactly: (1) rotate cert (replace `{data_dir}/tls/{cert,key}.pem` or delete to trigger regeneration + restart), (2) re-run `client-bundle` (new `fp`, unchanged base-url + token), (3) re-`init --remote` clients to re-pin. (file-check / content assertion.)

## Diagnosable rejection (integration — the load-bearing pairing)
The runbook is only a 3-step fix (not an opaque failure) because the client rejects a stale fingerprint legibly. These tests span `remote-client` (the error) + `cert-provisioner` (the rotation):

- `test_rotate_without_rebundle_yields_diagnosable_mismatch` (AC-CT-ROT.3) — rotate the server cert; do NOT re-bundle; existing client (pinned to the old `fp`) attempts reconnect → the client surfaces a **clear fingerprint-mismatch error** naming **expected vs presented** `sha256:` and pointing the operator to re-run `client-bundle` + `init`. Assert it is NOT a bare opaque TLS handshake failure. (This is the diagnosable rejection from `remote-client.md`, exercised through a real rotation.)
- `test_rotate_with_rebundle_reconnects` (AC-CT-ROT.2) — rotate the cert; re-run `client-bundle` (new bundle, new `fp`); re-`init --remote <new-bundle>`; assert the client re-pins and reconnect **succeeds**. Proves the runbook procedure actually works end-to-end.

## Cross-references
- The diagnosable-error contract is defined and unit-tested in `remote-client.md` (`test_pin_mismatch_rejects_with_diagnosable_error`) and `bundle-codec.md` (new `fp` emitted). This file proves the **operator procedure** wraps them correctly under a real rotation.
- Rotation = re-bundle + re-init is by-design of fingerprint pinning (ADR-002): a new cert invalidates every pin until re-pinned. No automation, no rotation tooling.

## Concrete assertions
The mismatch test asserts the error **text/structure names both fingerprints and the remediation** — "diagnosable" is the AC's literal requirement, so a pass requires the legible message, not merely a rejection.
