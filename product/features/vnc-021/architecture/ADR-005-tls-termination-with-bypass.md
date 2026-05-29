## ADR-005: rustls TLS Termination with Configurable Bypass

### Context

HTTPS transport requires TLS. Two deployment models exist:
1. **Direct TLS**: Unimatrix terminates TLS itself (personal cloud VPS with no reverse proxy)
2. **Proxy-terminated TLS**: A reverse proxy (nginx, Caddy, cloud LB) terminates TLS; Unimatrix receives plain HTTP on a loopback interface

The product vision (PRODUCT-VISION.md) calls for "zero infrastructure" -- the server should work without a reverse proxy. But proxy deployments are common in production.

TLS library options:
- **rustls** (already transitive via reqwest/hyper-rustls): Pure Rust, no OpenSSL dependency, audited, `#![forbid(unsafe_code)]` compatible
- **native-tls** (OpenSSL wrapper): Links to system OpenSSL, requires `unsafe`, platform-specific behavior
- **openssl crate**: Direct OpenSSL bindings, `unsafe`, heavy

### Decision

Use `rustls` 0.23 via `tokio-rustls` 0.26 for TLS termination. Both are already transitive dependencies (Unimatrix #4661). Configurable bypass via `[tls] enabled`:

- When `tls.enabled = true` (default when both `cert_path` and `key_path` are present): Server constructs `TlsAcceptor` from PEM files at startup. Invalid cert/key paths cause startup failure (AC-11). All connections are TLS-wrapped.
- When `tls.enabled = false`: Server binds plain HTTP. No TLS handshake. Intended for proxy-terminated deployments.
- When `tls.enabled` is absent: Auto-detect from `cert_path`/`key_path` presence. Both present = TLS on. Either absent = TLS off.

No hot-reload of certificates (non-goal per SCOPE.md). Certificate rotation requires server restart.

### Consequences

Easier: Pure Rust TLS stack, no OpenSSL build dependency, compatible with `#![forbid(unsafe_code)]`. Both deployment models supported with a single config knob. No new transitive dependencies.

Harder: Certificate management is manual (no ACME/Let's Encrypt integration). Operators in direct-TLS mode must provide PEM cert+key files and restart to rotate. This is acceptable for personal cloud; enterprise deployments use proxy-terminated TLS.
