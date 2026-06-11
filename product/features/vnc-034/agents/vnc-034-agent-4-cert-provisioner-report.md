# Agent Report — vnc-034-agent-4-cert-provisioner (Wave 1)

## Component
CertProvisioner — `load_or_generate_cert(data_dir, sans) -> Result<(CertPem, KeyPem), ServerError>` (LOCKED signature, SR-01). First-boot self-signed cert/key provisioner for the OSS fingerprint-pinning trust model (ADR-002). Realizes FR-A2/A3/A4/A9, SR-01, R-07/R-08/R-11.

## Files Modified
- `crates/unimatrix-server/src/http/cert_provisioner.rs` (NEW, 437 lines) — production provisioner + private-helper unit tests.
- `crates/unimatrix-server/tests/cert_provisioner.rs` (NEW, 205 lines) — public-API integration tests (idempotence, override, mode, fail-loud, concurrency, acceptor seam).
- `crates/unimatrix-server/src/http/mod.rs` — registered `cert_provisioner` submodule; re-exported `CertPem`, `KeyPem`, `load_or_generate_cert`.
- `crates/unimatrix-server/Cargo.toml` — moved `rcgen = "0.13"` from `[dev-dependencies]` to `[dependencies]` (required to promote test-only code to production; no NEW crate added — rcgen was already permitted).

`tls.rs` was NOT edited: the file would have exceeded the 500-line limit (it was ~450), so per the brief's instruction I extracted the provisioner into a submodule and flagged it. The C2 fingerprint helpers (`fingerprint_leaf_der`, `leaf_der_from_pem`) and the test-only `generate_self_signed` helper in tls.rs are untouched — referenced, never redefined.

Did NOT touch main.rs / router.rs / client_bundle.rs / public_url.rs. Not wired into main.rs (deferred to Sub-wave 3 per brief). The `mod.rs` re-export makes the symbol available for that wiring.

## Tests — 13 pass / 0 fail
Lib unit tests (`http::cert_provisioner`, 4): SAN-set==C3 derivation (DNS verbatim + IP octets in DER, no stray SAN), production validity window (825-day span, not_before<=today<=not_after), leaf-self-signed-no-chain, civil_from_days known epochs.

Integration tests (`tests/cert_provisioner.rs`, 9): first-call-generates, second-call-byte-identical-no-rewrite (mtime unchanged, AC-W1-S3), key mode 0600, operator-override honored-not-overwritten (FR-A3), partial-state cert-only / key-only loud error (no silent regen), unwritable /data actionable ProjectInit error naming UID 65532 + path (R-11, no panic), concurrent first boot (4 threads converge on one pair, key 0600), provisioned cert builds TlsConfig acceptor (AC-CT-C6 seam).

Regression: existing `http::tls` suite (12) still green. Clippy clean for the new files (no warnings referencing cert_provisioner); pre-existing crate-wide warnings unchanged.

## Design Notes
- Validity is date-granular (today .. today+825d UTC), computed via `civil_from_days` from `SystemTime` epoch-days + rcgen's re-exported `date_time_ymd` — avoids adding the `time` crate (a transitive rcgen dep, not direct) while keeping the window bounded and deterministic (R-08).
- Key-first ownership claim: the KEY is written `O_CREAT|O_EXCL|0600` first; only the winner publishes the matching cert. Loser sees `AlreadyExists` and loads the winner's pair via a bounded retry. This corrects an initial cert-first ordering that let racing threads clobber each other's cert and break cert/key pairing (caught by the concurrency test).
- Defensive load-time re-chmod of key to 0600 (best-effort; non-fatal on read-only override mounts).

## Issues / Blockers
None. One flagged deviation from "touch ONLY tls.rs": creating the submodule + its mod.rs registration + the Cargo.toml rcgen move were unavoidable consequences of (a) the 500-line limit and (b) promoting dev-only rcgen usage to production. All within the spirit of "+a submodule if size requires."

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search (pattern, decision) -- no prior cert-provisioning pattern; ADR-002 (#4948) confirmed leaf-DER fingerprint trust model and that SR-01 cert params (validity + key 0600) determine the hashed leaf.
- Stored: entry #4959 "rcgen 0.13 cert provisioning: bound validity without the time crate + key-first concurrent ownership claim" via /uni-store-pattern.
