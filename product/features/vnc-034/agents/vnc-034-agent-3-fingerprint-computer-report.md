# Agent Report — vnc-034-agent-3-fingerprint-computer

**Component:** FingerprintComputer (C2 build-first contract — ADR-002, ADR-006, SR-02, R-02)
**Wave:** 1 (lands FIRST — both #726 server and #725 client consume its parity corpus)
**Status:** COMPLETE

## Files Modified / Created

- `crates/unimatrix-server/src/http/tls.rs` (modified) — added:
  - `pub const FP_PREFIX: &str = "sha256:"`
  - `pub fn fingerprint_leaf_der(der: &[u8]) -> String` — LOCKED signature (ADR-002); returns `"sha256:" + lowercase_hex(sha256(der))`. Total function, cannot fail. Uses already-present `sha2` + `hex`.
  - `pub fn leaf_der_from_pem(cert_pem: &[u8]) -> Result<Vec<u8>, ServerError>` — caller-side helper to extract the served leaf DER from the acceptor's PEM (AC-W1-S4 served-cert equality). Errors via `.map_err()` into `ServerError::Config`.
- `crates/unimatrix-server/src/http/mod.rs` (modified) — re-export `pub use tls::{FP_PREFIX, fingerprint_leaf_der, leaf_der_from_pem}` so the integration oracle (external test crate) can reach the otherwise-`pub(crate)` functions. (NOTE: this file also carries the public-url agent's `pub(crate) mod public_url;` declaration — entangled in the shared checkout; my commit's `mod.rs` includes it but it is additive/idempotent.)
- `crates/unimatrix-server/tests/fingerprint_parity.rs` (new) — unit tests + oracle + drift guard + served-cert linkage. (Kept here, not inline in tls.rs, to hold tls.rs at 450 lines < 500.)
- `crates/unimatrix-server/tests/fixtures/c1c2-parity/fingerprint-golden.json` (new, COMMITTED) — the cross-stack parity corpus, 6 synthetic rows `{der_hex, fp}`. Source of truth for #725's JS test (JS golden NEVER hand-written — SR-02).

## Design decisions worth flagging

- **Corpus encoding = hex, not base64.** The test plan suggested a `der_b64` field, but `unimatrix-server` has no base64 dependency (not even dev). I used the already-present `hex` crate and a `der_hex` field. JS hex-decodes trivially; the byte-identity contract is unaffected. This honors "no new crates."
- **Corpus format = JSON array** (`fingerprint-golden.json`) per the test-plan's named file, rather than the OVERVIEW's TSV sketch. JSON is the most robust byte-identical form for the JS consumer (`serde_json` already present). Rows are `{der_hex, fp}`.
- **Synthetic DERs are deterministic** (fixed byte patterns + an LCG, not rcgen random keys) so the `#[ignore]` regen oracle is idempotent — verified: regeneration produces a byte-identical corpus. No real-provider-shaped secrets (lesson #4792).
- Did NOT add cert-generation logic (cert-provisioner, next wave). The served-cert linkage test uses `rcgen` directly as a dev-dependency to produce a throwaway cert for the DER-extraction assertion only.

## Tests

`cargo test -p unimatrix-server --test fingerprint_parity` — **12 passed, 0 failed, 1 ignored** (the oracle regen).
`cargo test -p unimatrix-server --lib http::tls` — **12 passed, 0 failed** (no regression in existing TLS tests).

Coverage vs test plan:
- Format correctness (R-02): prefix, 64-lowercase-hex, known SHA-256 vectors (empty + "abc"), DER-not-PEM divergence, determinism. ✓
- Oracle / parity corpus (ADR-002/006): `test_generate_c2_fingerprint_golden` (#[ignore] regen) + `test_c2_fingerprint_golden_is_stable` (normal-CI drift guard, re-derives every committed row). ✓
- Served-cert linkage (AC-W1-S4): `test_bundle_fp_equals_served_leaf_der_fingerprint`. ✓
- Edge cases: empty DER (defined, not panic), large DER (no truncation), garbage/empty PEM → `ServerError::Config`. ✓

`cargo fmt` clean. `cargo clippy -p unimatrix-server --test fingerprint_parity` — no warnings on my files (pre-existing lib warnings from other in-flight Wave-1 agents are unrelated).

## Self-check

- `cargo build -p unimatrix-server` passes. tls.rs = 450 lines (< 500).
- No `.unwrap()`/`.expect()` in non-test code; no `unsafe`; no `todo!()`/`TODO`.
- Touched only `tls.rs`, `mod.rs` (re-export), the new test, and the new fixtures dir. Did NOT edit main.rs, router.rs, public_url.rs, client_bundle.rs.
- Committed: `bc6e9646` — `impl(fingerprint-computer): C2 fingerprint_leaf_der + cross-stack parity oracle (#726)` (4 files; other agents' staged files unstaged before commit).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing + context_get(4751) — surfaced ADR-002 (#4948), the vnc-026 oracle-corpus precedent (#4751), and the parity-port pattern (#4789). Applied: Rust-as-oracle, committed goldens, normal-CI drift check.
- Stored: entry #4956 "Integration tests can't reach pub(crate) — re-export oracle fns at module surface; hex (not base64) for parity corpus" via /uni-store-pattern.

## Blockers

None. The corpus is committed and ready for #725's JS test to consume byte-identically.
