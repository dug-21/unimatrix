# tls-config (C5) -- `src/http/tls.rs`

## Purpose

Construct a `tokio_rustls::TlsAcceptor` from PEM certificate and key files (ADR-005). Validates cert/key at startup. Returns `None` when TLS is disabled (proxy-terminated deployments).

## Functions

### `build_tls_acceptor(config: &TlsConfig) -> Result<Option<TlsAcceptor>, ServerError>`

Returns `Some(TlsAcceptor)` when TLS is enabled, `None` when disabled.

```
fn build_tls_acceptor(config: &TlsConfig) -> Result<Option<TlsAcceptor>, ServerError>:
    if !config.is_enabled():
        tracing::info!("TLS disabled — binding plain HTTP (proxy-terminated mode)")
        return Ok(None)

    // Both paths are guaranteed present by config validation (C7)
    let cert_path = config.cert_path.as_ref()
        .ok_or(ServerError::Config("TLS enabled but cert_path missing"))?
    let key_path = config.key_path.as_ref()
        .ok_or(ServerError::Config("TLS enabled but key_path missing"))?

    // Load certificate chain
    let certs = load_certs(cert_path)?
    if certs.is_empty():
        return Err(ServerError::Config(
            format!("no certificates found in {}", cert_path.display())
        ))

    // Load private key
    let key = load_private_key(key_path)?

    // Build rustls ServerConfig
    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()             // no mTLS for personal cloud
        .with_single_cert(certs, key)
        .map_err(|e| ServerError::Config(
            format!("TLS configuration error: {e}")
        ))?

    let acceptor = TlsAcceptor::from(Arc::new(server_config))
    tracing::info!("TLS enabled — loaded cert from {}", cert_path.display())

    return Ok(Some(acceptor))
```

### `load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, ServerError>`

```
fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, ServerError>:
    let file = File::open(path)
        .map_err(|e| ServerError::Config(
            format!("cannot read TLS certificate {}: {e}", path.display())
        ))?
    let mut reader = BufReader::new(file)

    // rustls_pemfile::certs reads all PEM-encoded certificates
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ServerError::Config(
            format!("invalid PEM data in {}: {e}", path.display())
        ))?

    return Ok(certs)
```

### `load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, ServerError>`

```
fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, ServerError>:
    let file = File::open(path)
        .map_err(|e| ServerError::Config(
            format!("cannot read TLS key {}: {e}", path.display())
        ))?
    let mut reader = BufReader::new(file)

    // Try all key formats: PKCS#8, RSA, EC
    // rustls_pemfile::private_key reads the first private key found
    let key = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| ServerError::Config(
            format!("invalid PEM key in {}: {e}", path.display())
        ))?
        .ok_or_else(|| ServerError::Config(
            format!("no private key found in {}", path.display())
        ))?

    return Ok(key)
```

## Error Handling

| Error Case | Error Type | Caller Action |
|-----------|-----------|--------------|
| cert_path unreadable | `ServerError::Config` | Server refuses to start (FR-05) |
| key_path unreadable | `ServerError::Config` | Server refuses to start |
| No certificates in PEM | `ServerError::Config` | Server refuses to start |
| No private key in PEM | `ServerError::Config` | Server refuses to start |
| Cert/key mismatch | `ServerError::Config` (from rustls) | Server refuses to start |
| Invalid PEM format | `ServerError::Config` | Server refuses to start |
| TLS disabled | No error | Returns `None` |

## Key Test Scenarios

1. **Valid cert+key**: Load self-signed cert and key PEM files. Verify `Some(TlsAcceptor)` returned.
2. **Missing cert file**: Path does not exist. Verify `ServerError::Config` with descriptive message.
3. **Missing key file**: Path does not exist. Verify descriptive error.
4. **Invalid PEM**: File with garbage content. Verify rejection.
5. **Empty cert file**: Valid PEM structure but no certs. Verify "no certificates found" error.
6. **TLS disabled**: `config.is_enabled() = false`. Verify `Ok(None)`.
7. **PKCS#8 key format**: Verify accepted (rustls_pemfile handles multiple formats).
8. **EC key format**: Verify accepted.
9. **Cert/key mismatch**: Valid cert PEM, valid but mismatched key PEM. Verify rustls rejects at ServerConfig construction.
