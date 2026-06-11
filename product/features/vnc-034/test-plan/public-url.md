# Test Plan — PublicUrl (`derive_public_url`)

> New module in `crates/unimatrix-server/src/http/`. `derive_public_url(env: &Env) -> PublicUrl { base_url, host, sans }` (C3, ADR cross-ref). **One knob (`UNIMATRIX_PUBLIC_URL`), three consumers. Lead risk: R-09.**

## AC-IDs covered
AC-CT-C3 (single derivation, three consumers, no socket auto-detect), AC-W1-S9 (bundle host ∈ cert SAN).

---

## Unit tests (Rust)

### Single derivation, three consumers (R-09)
- `test_derive_public_url_base_url_verbatim` — set `UNIMATRIX_PUBLIC_URL=https://cloud.example:8443`; assert `base_url` == verbatim.
- `test_derive_public_url_host_extracted` — assert `host` == `cloud.example` (host only, port/scheme stripped where the consumer needs the bare host).
- `test_derive_public_url_sans_set` — assert `sans` == `["localhost","127.0.0.1","0.0.0.0","cloud.example"]` (the cert-SAN consumer, FR-A4).
- `test_three_consumers_read_one_derivation` (AC-CT-C3) — structural: bundle `base_url`, `allowed_hosts` default, and cert SAN all read from the **single** `PublicUrl` value; assert there is no second/independent host parse in the bundle or cert path. (Source assertion + a test that mutating the input changes all three consumers together.)

### host ∈ SAN invariant (AC-W1-S9, SR-10)
- `test_bundle_host_in_cert_sans` — for any `UNIMATRIX_PUBLIC_URL`, assert `host` (the bundle base-url host) is an element of `sans`. The connect-time fingerprint+host mismatch (R-09 impact) is unrepresentable when this holds.

### Unset / placeholder behavior
- `test_unset_public_url_yields_edit_me_placeholder` — env unset → `base_url` contains the loud `https://<EDIT-ME>:8443` placeholder (the onboarding tripwire the stderr echo surfaces, FR-A5b).
- `test_unset_public_url_allowed_hosts_permissive_with_warning` — unset → `allowed_hosts` default is permissive-with-warning (not a hard fail), per C3 table.

### No socket auto-detect (R-09 negative)
- `test_no_socket_autodetect` — source/structural assertion: `derive_public_url` derives only from `env`, never from a bound socket address. Confirm no `local_addr()`/`peer_addr()` feeds the public URL.

## Edge cases (assigned here — `RISK-TEST-STRATEGY §Edge Cases`)
- `UNIMATRIX_PUBLIC_URL` **with explicit port** (`:9000`) → carried into `base_url`; SAN host has no port.
- `UNIMATRIX_PUBLIC_URL` **with a path** → defined handling (path ignored for host/SAN; base_url policy explicit).
- **IPv6 literal** (`https://[::1]:8443`) → host extracted correctly (bracketed literal), present in SAN.
- Trailing slash / scheme-less input → normalized or rejected deterministically (define which).

## Concrete assertions
Assert exact `sans` vectors and exact `host` strings, not "host looks right". The `host ∈ sans` test iterates the SAN vector for membership.
