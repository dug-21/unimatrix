# PublicUrl — `derive_public_url`

> `crates/unimatrix-server/src/http/public_url.rs` (new module). Realizes C3 (ADR derived), FR-A7, SR-10, R-09. ONE derivation feeding THREE consumers: bundle base-url, `allowed_hosts` default, cert SAN. Socket auto-detect is rejected.

## Purpose

`UNIMATRIX_PUBLIC_URL` is the single piece of operator knowledge the cloud cannot auto-derive. This component turns it (or a loud placeholder when unset) into a `PublicUrl { base_url, host, sans }` consumed identically by all three sites, so the cert SAN can never desync from the bundle base-url (R-09). The invariant `bundle.host ∈ cert.sans` (SR-10) holds by construction because both read this one struct.

## Locked signature + shared type

```rust
pub struct PublicUrl {
    pub base_url: String,     // verbatim bundle base-url, e.g. "https://cloud.example:8443"
    pub host: String,         // host[:port]-stripped host for allowed_hosts + SAN, e.g. "cloud.example"
    pub sans: Vec<String>,    // ["localhost","127.0.0.1","0.0.0.0", host]  (host omitted if placeholder)
}

pub fn derive_public_url(env: &Env) -> PublicUrl;
```

`Env` is a thin, testable env accessor (Rust 2024 forbids `std::env::set_var` under `#![forbid(unsafe_code)]`, mirroring `resolve_env_config_path`'s pure-function approach in config.rs):
```rust
pub struct Env<'a> { get: &'a dyn Fn(&str) -> Option<String> }
// production: Env{ get: &|k| std::env::var(k).ok() }
// tests:      Env{ get: &|k| map.get(k).cloned() }
```

Constants:
```
PUBLIC_URL_VAR    = "UNIMATRIX_PUBLIC_URL"
PLACEHOLDER       = "https://<EDIT-ME>:8443"     // loud, un-pasteable-by-accident
LOCAL_SANS        = ["localhost", "127.0.0.1", "0.0.0.0"]
DEFAULT_PORT      = 8443
```

## Function: derive_public_url

```
fn derive_public_url(env: &Env) -> PublicUrl:
    raw = env.get(PUBLIC_URL_VAR)

    match raw:
      None or empty:
        // UNSET -> loud placeholder (FR-A7). base_url is intentionally NOT a valid host so the
        // operator notices via the client-bundle stderr echo before distributing (FR-A5b pairing).
        log WARN "UNIMATRIX_PUBLIC_URL unset — emitting placeholder base-url and permissive-with-
                  warning allowed_hosts. Set it before distributing the bundle."
        return PublicUrl{
            base_url: PLACEHOLDER,
            host:     "<EDIT-ME>",            // sentinel host; allowed_hosts goes permissive-with-warning
            sans:     LOCAL_SANS.to_vec(),    // host NOT appended (sentinel is not a real SAN)
        }

      Some(s):
        // SET -> parse and derive all three from it.
        url = parse_url(s)                    // tolerant: accept with/without scheme, with port, IPv6 literal
        // Normalize: require https scheme for OSS posture (no plaintext-to-client, NFR-07).
        if url.scheme present and != "https":
            return loud error path OR coerce-with-warning? -> CHOICE: coerce base_url to https and WARN.
            // Rationale: operator typo http:// should not silently ship a plaintext base-url; the
            // bundle schema (ADR-001) requires base_url https, so emit https + warn.
        host = url.host (brackets stripped for IPv6)        // e.g. "cloud.example" or "::1"
        port = url.port or DEFAULT_PORT
        base_url = "https://" + host_with_brackets_if_ipv6 + ":" + port
        sans = LOCAL_SANS.to_vec(); sans.push(host)         // host appended -> SR-10 host ∈ SAN holds
        dedup(sans)                                         // avoid duplicate if host == "localhost" etc.
        return PublicUrl{ base_url, host, sans }
```

### parse_url tolerance (edge cases — RISK-TEST §Edge Cases)

- No scheme (`cloud.example:8443`) -> prepend `https://` then parse.
- With path (`https://h:8443/foo`) -> path discarded; base_url is scheme+host+port only.
- IPv6 literal (`https://[::1]:8443`) -> host = `::1`; base_url keeps brackets; SAN entry is the bracket-less `::1`.
- Port absent -> `DEFAULT_PORT` (8443).
- Reject only on un-parseable garbage -> return placeholder + WARN (never panic).

## The three consumers (wired by their owners — this module only derives)

| Consumer | Reads | Site |
|----------|-------|------|
| Bundle `base_url` (C1) | `pu.base_url` verbatim | bundle-codec.md `run_client_bundle` |
| `allowed_hosts` default | `pu.host` when real; permissive-with-warning when placeholder | listener/config wiring (`HttpConfig.allowed_origins`/host posture) |
| Cert SAN (C2/SR-01) | `&pu.sans` | cert-provisioner.md `load_or_generate_cert(data_dir, &pu.sans)` |

`allowed_hosts` posture: when `host == "<EDIT-ME>"` sentinel, the host check is permissive (accept any Host) but logs a WARN each startup; when real, default `allowed_hosts` to `[host]`. (Auto-detect from the accept socket is explicitly NOT implemented — FR-A7, R-09.)

## Data flow

- **Input:** `Env` (reads `UNIMATRIX_PUBLIC_URL`).
- **Output:** `PublicUrl` value, read by three consumers.
- Pure function over `Env` — no I/O, no socket inspection.

## Error handling

Total function — never errors, never panics. Un-parseable/unset input degrades to the loud placeholder + WARN. (A scheme typo is coerced to https with a warning so the bundle schema invariant cannot be violated downstream.)

## Key test scenarios (hints for tester)

- Set knob -> all three derivations reflect it; assert `pu.host ∈ pu.sans` (SR-10, AC-W1-S9, R-09).
- Single-derivation source: bundle base-url, allowed_hosts, cert SAN all trace to `derive_public_url` (AC-CT-C3).
- Unset -> `base_url == PLACEHOLDER`, WARN logged, `sans == LOCAL_SANS` (host not appended), permissive-with-warning allowed_hosts (FR-A7).
- Edge: no scheme / with path / IPv6 literal / no port / `http://` typo (coerced to https + warn) (RISK-TEST edge cases).
- No socket auto-detect path exists (source assertion) (R-09).
- `Env` is injectable: tests exercise all branches without `std::env::set_var`.
```
