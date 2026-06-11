# CertProvisioner — `load_or_generate_cert`

> `crates/unimatrix-server/src/http/tls.rs`. Promotes the test-only `generate_self_signed` (tls.rs:118–125) to a production first-boot provisioner. Realizes FR-A2/A3/A4/A9, SR-01, R-07 (idempotence), R-08 (production params), R-11 (fail-loud).

## Purpose

On first boot, generate a self-signed cert + key into `{data_dir}/tls/{cert.pem,key.pem}` with production SANs, a defined validity period, and key mode `0600`. On subsequent boots, LOAD the existing pair byte-identically (idempotence — a regenerated cert silently invalidates every pinned client and the emitted bundle, R-07). Operator override (own cert+key mounted read-only) is honored, never overwritten (FR-A3).

## Locked signature

```rust
pub fn load_or_generate_cert(data_dir: &Path, sans: &[String])
    -> Result<(CertPem, KeyPem), ServerError>;
// CertPem = Vec<u8>, KeyPem = Vec<u8>  (PEM byte buffers; same shape as today's test helper)
```

Constants (production params — SR-01/R-08):
```
CERT_DIR_NAME      = "tls"
CERT_FILE_NAME     = "cert.pem"
KEY_FILE_NAME      = "key.pem"
KEY_FILE_MODE      = 0o600
CERT_VALIDITY_DAYS = 825        // bounded, defined; not rcgen's unbounded default
```

## Function: load_or_generate_cert

Mirrors `load_or_generate_token`'s generate-first + `O_CREAT|O_EXCL` idempotence pattern (token.rs:37–54) to avoid a TOCTOU race on concurrent first boots (R-07 scenario 3 / edge "first boot racing two container starts").

```
fn load_or_generate_cert(data_dir, sans):
    tls_dir  = data_dir.join(CERT_DIR_NAME)
    cert_path = tls_dir.join(CERT_FILE_NAME)
    key_path  = tls_dir.join(KEY_FILE_NAME)

    // Step 0: ensure tls/ dir exists; fail loud-and-actionable if /data unwritable (FR-A9, R-11)
    fs::create_dir_all(tls_dir)
        .map_err(|e| ServerError::ProjectInit(actionable_msg(tls_dir, e)))?
    // actionable_msg = "cannot create {path}: {e}. Ensure /data is writable by UID 65532
    //                   (see container bind-mount docs)."  — names the path + the fix.

    // Step 1: BOTH files already present -> LOAD, do not regenerate (idempotence + override, R-07/FR-A3).
    if cert_path.exists() AND key_path.exists():
        cert_pem = fs::read(cert_path).map_err(|e| ServerError::Config(read_msg(cert_path, e)))?
        key_pem  = fs::read(key_path ).map_err(|e| ServerError::Config(read_msg(key_path , e)))?
        if cert_pem.is_empty() OR key_pem.is_empty():
            return Err(ServerError::Config("existing TLS material is empty: {path}"))
        // Defensive: re-assert key mode 0600 on load in case an override arrived looser.
        best_effort_chmod_0600(key_path)   // ignore error on non-unix; log at debug
        return Ok((cert_pem, key_pem))

    // Step 2: PARTIAL state (exactly one present) -> fail loud, never silently regenerate the pair.
    if cert_path.exists() XOR key_path.exists():
        return Err(ServerError::Config(
            "incomplete TLS material: one of {cert,key}.pem present, the other missing — \
             remove both to regenerate or supply both (override)."))

    // Step 3: NEITHER present -> generate. Atomic create-with-mode for the KEY (0600, R-08).
    (cert_pem, key_pem) = generate_self_signed_production(sans)?   // see below

    // Write cert first (0644 ok — public), then key atomically with 0600.
    write_atomic(cert_path, &cert_pem, 0o644)
        .map_err(|e| cleanup_then_err(cert_path, e))?
    write_atomic_excl_mode(key_path, &key_pem, KEY_FILE_MODE)      // O_CREAT|O_EXCL|0600
        .map_err(|e| { let _ = fs::remove_file(cert_path);          // roll back cert on key failure
                       map_create_err(key_path, e) })?
    // On AlreadyExists for the key (a racing boot won) -> fall back to Step 1 load (re-read both).
    return Ok((cert_pem, key_pem))
```

### Helper: generate_self_signed_production (R-08 — production params, NOT test defaults)

```
fn generate_self_signed_production(sans: &[String]) -> Result<(CertPem, KeyPem), ServerError>:
    params = rcgen::CertificateParams::new(sans.to_vec())     // SANs from derive_public_url (C3)
        .map_err(|e| ServerError::Config("cert params: {e}"))?
    // SANs are DNS names + IP literals; classify each: IP-parseable -> SanType::IpAddress,
    // else SanType::DnsName. (sans already include localhost,127.0.0.1,0.0.0.0,<public host> per C3.)
    rebuild params.subject_alt_names from sans with correct SanType per element
    params.not_before = now()
    params.not_after  = now() + Duration::days(CERT_VALIDITY_DAYS)    // defined validity (SR-01)
    key_pair = rcgen::KeyPair::generate().map_err(...)?
    cert = params.self_signed(&key_pair).map_err(...)?
    Ok((cert.pem().into_bytes(), key_pair.serialize_pem().into_bytes()))
```

### Helpers: atomic writes

```
write_atomic_excl_mode(path, bytes, mode):   // unix: OpenOptions.create_new(true).mode(mode)
    open O_CREAT|O_EXCL with mode; write_all; on write error remove_file(path) (no 0-byte leak)
    // non-unix: create_new(true) then best-effort set_permissions
write_atomic(path, bytes, mode): same minus O_EXCL strictness (cert is public)
```

## Initialization sequence (where this is called — ARCHITECTURE §4.1, §6)

In `main.rs` listener wiring (~L840–900), gated on HTTP enabled, BEFORE `build_tls_acceptor`:
```
pu = derive_public_url(env)
(cert_pem, key_pem) = load_or_generate_cert(paths.data_dir, &pu.sans)?
// write resolved paths into TlsConfig so the existing PEM-file acceptor loads them:
tls_cfg = TlsConfig{ enabled:Some(true),
                     cert_path:Some(data_dir/tls/cert.pem),
                     key_path :Some(data_dir/tls/key.pem) }
acceptor = build_tls_acceptor(&tls_cfg)?     // existing fn, unchanged
```
(`build_tls_acceptor` reads PEM files; we provision the files first. The returned PEM buffers are also reused by `client-bundle` to recompute the served leaf DER — see bundle-codec.md.)

## Data flow

- **Input:** `data_dir` (resolved `/data/.unimatrix/{hash}`), `sans: &[String]` from `derive_public_url().sans`.
- **Output:** `(cert_pem, key_pem)` PEM buffers; side effect: `tls/{cert,key}.pem` on disk, key `0600`.
- **Downstream:** files consumed by `build_tls_acceptor` (serving) and `run_client_bundle` (fingerprint of served leaf DER).

## Error handling

| Condition | Result |
|-----------|--------|
| `/data` (tls dir) unwritable by UID 65532 | `ServerError::ProjectInit` — names path + UID-65532 fix; no panic, no `.unwrap()` (R-11) |
| Only one of cert/key present | `ServerError::Config` — incomplete material, do not regenerate (R-07) |
| Existing file empty/unreadable | `ServerError::Config` with path |
| rcgen params/keygen/self-sign failure | `ServerError::Config` mapped from rcgen error |
| Key write fails after cert written | remove cert (rollback), return mapped error |
| Concurrent creator won the EXCL race | fall back to load both (converge, like token.rs) |

No `.unwrap()`/`.expect()` anywhere in this file's non-test code.

## Key test scenarios (hints for tester)

- Generate-then-load: two calls on the same `data_dir` return byte-identical PEM; cert+key NOT regenerated (R-07, AC-W1-S3).
- Key file mode is exactly `0600` at creation (atomic, no chmod window) (R-08, AC-W1-S3).
- SAN set = `derive_public_url().sans` (`localhost`,`127.0.0.1`,`0.0.0.0`,public host); validity within `CERT_VALIDITY_DAYS`; none inherited from the test helper (R-08, AC-W1-S9).
- Override: pre-place operator cert+key -> loaded, not overwritten (FR-A3).
- Partial state (cert only / key only) -> loud error, no regeneration.
- Unwritable `/data` -> actionable `ProjectInit` error naming UID 65532; no panic (R-11, AC-W1-S8).
- Concurrent first boot (two threads, shared dir) -> both converge on the same cert, no corruption (R-07).
- Bundle parity: the leaf DER recovered from the generated cert PEM equals the DER rustls serves (cross-check feeds fingerprint-computer.md / AC-W1-S4).
```
