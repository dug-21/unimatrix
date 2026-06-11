# FingerprintComputer — `fingerprint_leaf_der`

> `crates/unimatrix-server/src/http/tls.rs` (new fn). Realizes C2 (ADR-002), FR-A6, SR-02, R-02. This is the **Rust oracle** for the cross-stack parity corpus — the JS client's golden values are generated from it, never hand-written.

## Purpose

Compute the canonical cert-fingerprint `sha256:<64 lowercase hex>` over the **served leaf certificate's DER** bytes. One interpretation only: DER (not PEM), leaf only (not chain), lowercase hex. The server emits this in the bundle (`fp` field); the JS client recomputes `sha256(cert.raw)` in `checkServerIdentity` and constant-form-compares. Divergence silently breaks pinning at connect time (R-02), so the format is byte-exact and oracle-driven.

## Locked signature

```rust
pub fn fingerprint_leaf_der(der: &[u8]) -> String;   // -> "sha256:" + lowercase_hex(sha256(der))
```

Constant:
```
FP_PREFIX = "sha256:"
```

## Function: fingerprint_leaf_der

```
fn fingerprint_leaf_der(der: &[u8]) -> String:
    digest = sha256(der)                      // 32 bytes
    // Use the crypto already present: ring (rustls' provider) Sha256, or the `sha2` crate if
    // already in the tree. NO new crate (NFR-02).
    hex = lowercase_hex_encode(digest)        // hex::encode is lowercase by default -> 64 chars
    return FP_PREFIX + hex                     // total form: "sha256:" + 64 lowercase hex
```

Notes that pin the contract (ADR-002):
- **DER in, not PEM.** Callers MUST pass the DER bytes (`CertificateDer` as served by rustls), never the PEM text. The PEM->DER extraction happens in the caller (see "DER extraction" below), so this fn never sees base64/PEM headers.
- **Leaf only.** The OSS self-signed cert has no chain; callers pass the single leaf DER. If a caller ever holds a chain, it passes `chain[0]` (the leaf rustls serves).
- **Lowercase always.** `hex::encode` yields lowercase; do NOT uppercase. Comparison downstream is case-sensitive on this canonical form.

## DER extraction helper (caller-side, same module)

`client-bundle` and the listener wiring hold PEM (from `load_or_generate_cert`); rustls serves DER. To fingerprint the *served* leaf (AC-W1-S4 — bundle fp == served cert, not a stale on-disk one), extract DER from the same PEM the acceptor loads:

```
fn leaf_der_from_pem(cert_pem: &[u8]) -> Result<Vec<u8>, ServerError>:
    reader = BufReader::new(cert_pem)
    certs  = rustls_pemfile::certs(reader).collect::<Result<Vec<_>,_>>()
                .map_err(|e| ServerError::Config("invalid cert PEM: {e}"))?
    leaf   = certs.first().ok_or(ServerError::Config("no certificate in PEM"))?
    return Ok(leaf.as_ref().to_vec())     // CertificateDer derefs to &[u8] DER
```
This reuses the existing `rustls_pemfile` dependency and the same parse `load_certs` uses (tls.rs:82–94), guaranteeing the bundle fingerprints exactly the bytes the acceptor serves.

## Oracle / parity-fixture generation (C1/C2 build-first — ADR-006, SR-02)

A throwaway Rust test (the oracle) emits the committed golden corpus consumed by the JS test. The JS expected value is NEVER hand-written.

```
#[test] generate_fingerprint_parity_corpus():    // gated/ignored or run on demand
    for each fixture cert in a small fixed set (generated deterministically or checked-in DER):
        der = fixture_der
        fp  = fingerprint_leaf_der(&der)
        emit line  "GOLDEN\t" + hex(der) + "\t" + fp     // tab-separated
    write to the committed fixture file (location decided in test-plan Stage 3a;
        e.g. packages/unimatrix/test/fixtures/fingerprint-parity.jsonl or a shared corpus path)
```
The JS side (remote-client.md) reads this corpus and asserts its `sha256(der)` compute equals the golden `fp` for the same `der` — closing R-02 by construction.

## Data flow

- **Input:** `der: &[u8]` (leaf DER), produced by `leaf_der_from_pem(cert_pem)` from the provisioned/served cert.
- **Output:** `String` `"sha256:<64hex>"`.
- **Consumers:** `run_client_bundle` (`fp` field of the bundle, bundle-codec.md); the parity corpus (JS pin verification, remote-client.md).

## Error handling

`fingerprint_leaf_der` itself is total (cannot fail — it hashes bytes). The fallible part is `leaf_der_from_pem` (invalid/empty PEM -> `ServerError::Config`). No `.unwrap()` in non-test code.

## Key test scenarios (hints for tester)

- Known-vector: a fixed DER hashes to a known `sha256:<hex>`; assert prefix + exactly 64 lowercase hex chars (R-02 scenario 4).
- Cross-stack parity: golden corpus from this oracle == JS `checkServerIdentity` compute over the same DER (AC-CT-C2, R-02 scenario 1) — the load-bearing C2 test.
- Served-cert equality: `fingerprint_leaf_der(leaf_der_from_pem(served_cert_pem))` == bundle `fp`; proves the bundle pins the served cert, not a stale one (AC-W1-S4, R-02 scenario 2).
- Casing: output is lowercase; an uppercase or PEM-derived input would NOT match (reject path lives in the JS schema `^sha256:[0-9a-f]{64}$` and bundle schema) (R-02 scenario 4).
- `leaf_der_from_pem` on empty/garbage PEM -> `ServerError::Config`, no panic.
```
