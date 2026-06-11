# Test Plan — FingerprintComputer (`fingerprint_leaf_der`)

> `crates/unimatrix-server/src/http/tls.rs` (new fn). `fingerprint_leaf_der(der: &[u8]) -> String` → `"sha256:" + lowercase_hex(sha256(der))` (ADR-002, C2). **This is the single Rust oracle for the C1/C2 parity corpus. Lead risk: R-02.**

## AC-IDs covered
AC-CT-C2 (format + cross-stack parity, oracle side), AC-W1-S4 (served-cert equality, contributes the compute), AC-W1-C2 (the value the client pins).

---

## Unit tests (Rust)

### Format correctness (R-02)
- `test_fingerprint_has_sha256_prefix` — output starts with literal `sha256:`.
- `test_fingerprint_is_64_lowercase_hex` — the post-prefix body matches `^[0-9a-f]{64}$`; assert NO uppercase, NO `0x`, no separators, no PEM artifacts.
- `test_fingerprint_matches_known_sha256_vector` — feed a fixed byte slice with a precomputed SHA-256; assert exact `sha256:<hex>` string equality. (Anchors the algorithm independently of any cert.)
- `test_fingerprint_hashes_der_not_pem` — given the same logical cert as DER and as PEM bytes, assert the function is only ever called on DER and the DER-hash differs from a naive PEM-bytes hash (guards the DER-not-PEM contract that SR-02 calls out as the silent-break vector).
- `test_fingerprint_deterministic` — same input → identical output across calls.

### Oracle / parity-corpus generation (R-02, ADR-002, ADR-006)
- `test_generate_c2_fingerprint_golden` (regen test — `--ignored` or explicit) — over a fixed set of synthetic leaf DERs, emit rows `{ der_b64, fp }` into `crates/unimatrix-server/tests/fixtures/c1c2-parity/fingerprint-golden.json`. **This test is the oracle — the JS golden is derived from it, never hand-written.** Synthetic DERs only; no real-provider-shaped secrets (lesson #4792).
- `test_c2_fingerprint_golden_is_stable` (regression guard, runs in normal CI) — re-derive `fp` for every committed row's `der_b64` via `fingerprint_leaf_der`; assert == the committed `fp`. Divergence (e.g. a hex-casing or DER-handling change) fails CI here, not at user connect.

### Served-cert linkage (AC-W1-S4 — the C2 wire-contract equality)
- `test_bundle_fp_equals_served_leaf_der_fingerprint` — take the cert `cert-provisioner` provisions and the listener serves; independently SHA-256 its leaf DER; assert == the `fp` the bundle carries. Proves the bundle pins the *served* cert, not a stale on-disk one. (Spans cert-provisioner + bundle-codec; the fingerprint compute is owned here.)

## Edge cases (assigned here)
- Empty DER slice → defined behavior (still produces a `sha256:` of the empty input; documented, not a panic).
- Large DER (multi-KB cert) → correct hash, no truncation.

## Cross-stack contract (consumed by `bundle-codec.md` JS side)
The committed `fingerprint-golden.json` rows are read by the JS client test (`bundle-codec.md` / `remote-client.md`): JS computes its pin over `row.der_b64` and asserts == `row.fp`. Server==client byte-equality by construction. No JS expected value is authored by hand.
