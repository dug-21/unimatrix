## ADR-002: Cert-Fingerprint Format — `sha256:<lowercase-hex>` over Leaf DER, Single Oracle + Parity Fixtures (C2)

### Context

C2 is the cert-fingerprint pinning trust model — the wire contract that motivated this umbrella. The OSS trust model is fingerprint pinning, NOT CA-trust / SAN hostname validation (a Non-Goal; CA+SAN is the enterprise/proxy path). The server computes a fingerprint of its served leaf certificate; the client pins that exact value and rejects any cert that does not match.

The fingerprint is computed **independently on two stacks**: Rust (DER → SHA-256 → hex) on the server, and JavaScript (the cert presented in Node's `checkServerIdentity`) on the client. SR-02: divergent DER serialization or hex casing silently breaks pinning at connect time — the client would reject the legitimate server, or (worse, if a normalization bug went the other way) accept a wrong one. This is a high-severity correctness contract that must be byte-identical across stacks.

A certificate has multiple serializations (PEM vs DER) and a chain may have multiple certs. The fingerprint must be unambiguous about *which bytes* are hashed.

### Decision

The fingerprint format is:

```
sha256:<lowercase-hex>
```

- Algorithm prefix `sha256:` — explicit, future-proof (an `sha384:` could be added behind a version bump without ambiguity).
- `<lowercase-hex>` = 64 lowercase hex characters = SHA-256 over the **served leaf certificate's DER encoding**. Specifically:
  - **DER, not PEM** — the raw DER bytes of the certificate, not the base64-PEM text. PEM line-wrapping/headers would make the hash serialization-dependent.
  - **The leaf only**, not the chain — the single self-signed cert the server serves. (OSS self-signed has no chain; pinning the leaf is the model.)
  - The exact same DER bytes that rustls serves to the client — on the Rust side, hash the cert's DER as loaded for the acceptor; on the JS side, hash the DER recovered in `checkServerIdentity` (`cert.raw`).
- **Lowercase hex, always.** Casing is part of the contract; the comparison is case-sensitive on the canonical lowercase form.
- **Single oracle + cross-stack parity fixtures (SR-02, pattern #4766):** the Rust side is the oracle. A throwaway Rust test emits golden lines (`GOLDEN\t<der-hex>\t<fingerprint>`) into a committed JSON fixture; the JS test asserts its computation matches the golden. The JS-side expected value is **never hand-written** — it is generated from the Rust oracle, exactly as the path-hash parity corpus was (vnc-026).

Client pinning mechanism (SR-03): a custom `checkServerIdentity(host, cert)` that computes `sha256(cert.raw)` and constant-form-compares to the pinned `fp`; CA chain validation is bypassed (self-signed). No CA path, no new dependency, minimal size — satisfies the < 250 KB gate.

### Consequences

- **Easier:** Unambiguous — "SHA-256 of the leaf DER, lowercase hex" has exactly one interpretation; no PEM/chain/casing drift.
- **Easier:** Parity is provable and regression-guarded by a committed fixture generated from one oracle; a divergence fails CI, not a user's connect attempt.
- **Easier:** Self-describing prefix leaves room for algorithm agility without breaking existing pins.
- **Harder:** Cert rotation invalidates the pin — rotating the cert requires re-`client-bundle` + re-`init`, documented as the rotation runbook deliverable (ARCHITECTURE §4.5), paired with a clean/diagnosable fingerprint-mismatch rejection so rotation-without-re-bundle is a legible 3-step fix, not an opaque TLS error. Acceptable for the single-operator model.
- **Harder:** The client must reach into platform TLS internals (`cert.raw`) and supply a custom identity check — fiddly but bounded, and the size/dependency gate is a hard Wave-1 acceptance test (SR-03).

### Related

- C1 bundle (ADR-001): the `fp` field carries this value; its schema validator enforces `^sha256:[0-9a-f]{64}$`.
- C3 (cert SAN derivation): SANs are generous and do not participate in trust (pinning, not SAN validation) — but bundle host ∈ SAN is asserted for connect-time host acceptance (SR-10).
- SR-01 (cert params): production cert generation (ADR in ARCHITECTURE §5) sets validity + key `0600`; the fingerprint is computed over whatever leaf that produces.
