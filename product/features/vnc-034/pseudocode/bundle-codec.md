# BundleCodec — `run_client_bundle` (Rust encoder) + JS decoder mirror

> `crates/unimatrix-server/src/client_bundle.rs` (new) + the JS decoder in remote-client.md. Realizes C1 (ADR-001), FR-A5/A5b/B9, SR-09, R-05. The C1 contract is authored ONCE here and consumed by both #726 (server emit) and #725 (client ingest). Sync, pre-tokio subcommand (C-10).

## Purpose

Encode the connection bundle `{v, base_url, token, fp}` into the locked wire form `unimatrix-bundle:<base64url(canonical-json)>` and emit it on stdout (opaque blob only), with a token-redacted human echo on stderr. The decode half (schema + guard ordering) is the trust-boundary contract; the Rust encoder is the oracle, the JS decoder (remote-client.md) mirrors it.

## Locked wire form (ADR-001 — encode precisely)

```
unimatrix-bundle:<base64url-nopad(canonical-json)>
canonical-json (FIXED field order): {"v":1,"base_url":"https://...:8443","token":"<64hex>","fp":"sha256:<64hex>"}
```
- Scheme prefix `unimatrix-bundle:` (literal). Single line. base64url = RFC 4648 §5 URL-safe alphabet, **no padding**.
- Canonical encode: field order `v, base_url, token, fp`; no insignificant whitespace. The server is the only encoder.
- Strict schema (the load-bearing guard): exactly four keys; `v == 1`; `base_url` starts `https://`; `token` matches `^[0-9a-f]{64}$`; `fp` matches `^sha256:[0-9a-f]{64}$`.

Constants:
```
BUNDLE_SCHEME   = "unimatrix-bundle:"
BUNDLE_VERSION  = 1
MAX_RAW_LEN     = 4096          // 4 KB cap on the RAW pasted string (bytes), BEFORE decode/parse
TOKEN_RE        = ^[0-9a-f]{64}$
FP_RE           = ^sha256:[0-9a-f]{64}$
```

## Locked signature

```rust
pub fn run_client_bundle(project_dir: Option<PathBuf>) -> Result<(), ServerError>;  // sync, pre-tokio
```

## Function: run_client_bundle (server emit — ARCHITECTURE §4.2)

```
fn run_client_bundle(project_dir) -> Result<(), ServerError>:
    // Sync, no tokio. Resolve the same data_dir the listener uses.
    paths = ensure_data_directory(project_dir.as_deref(), None)
                .map_err(|e| ServerError::ProjectInit(e.to_string()))?
    data_dir = paths.data_dir

    // 1. Read token (hex string) from {data_dir}/token  (do NOT log it).
    token_hex = read_token_hex(data_dir)?            // reuse token.rs read+validate (64 lowercase hex)

    // 2. Read served leaf cert PEM, extract DER, fingerprint it (C2).
    cert_pem = fs::read(data_dir/"tls"/"cert.pem")
                 .map_err(|e| ServerError::Config("cannot read cert: {e}. Run the server once to \
                                                    provision, or check /data."))?
    der = leaf_der_from_pem(&cert_pem)?              // fingerprint-computer.md helper
    fp  = fingerprint_leaf_der(&der)                 // "sha256:<64hex>"

    // 3. base_url from the single C3 derivation.
    pu = derive_public_url(&Env::system())
    base_url = pu.base_url

    // 4. Encode the bundle.
    blob = encode_bundle(BUNDLE_VERSION, &base_url, &token_hex, &fp)?

    // 5. Output contract (HARD — stdout/stderr split, FR-A5b / NFR-06):
    //    stdout = the opaque blob ONLY (pipeable; no token, no extra text, no trailing prose).
    println!("{blob}")                               // to STDOUT
    //    stderr = human echo of base_url + fp ONLY; TOKEN NEVER PRINTED.
    eprintln!("unimatrix connection bundle (paste into: unimatrix init --remote <bundle>)")
    eprintln!("  base-url : {base_url}")
    eprintln!("  cert-fp  : {fp}")
    if base_url contains "<EDIT-ME>":
        eprintln!("  WARNING  : UNIMATRIX_PUBLIC_URL is unset — base-url is a placeholder. \
                                 Set it and re-run before distributing this bundle.")
    // token appears ONLY inside the base64url blob on stdout — nowhere else (AC-W1-S5/S5b).
    return Ok(())
```

### encode_bundle

```
fn encode_bundle(v, base_url, token_hex, fp) -> Result<String, ServerError>:
    // Build canonical JSON with FIXED key order. Do NOT use a HashMap (order undefined).
    // Use a serde struct with #[serde(rename)] in declared order, or hand-build the string.
    json = format canonical:  {"v":<v>,"base_url":"<base_url>","token":"<token_hex>","fp":"<fp>"}
    // (If hand-building: base_url/fp/token are constrained values already; still serde_json::to_string
    //  a struct whose fields are declared v, base_url, token, fp to guarantee order + escaping.)
    b64 = base64url_nopad_encode(json.as_bytes())
    return Ok(BUNDLE_SCHEME + b64)
```

## Decode contract (the shared trust-boundary algorithm — mirrored in JS, remote-client.md)

This is the canonical guard ordering (ADR-001, FR-B9, AC-W1-C9/C10). The Rust side provides a `decode_bundle` used in round-trip tests; the JS side is the production decoder. **Both implement this exact order:**

```
fn decode_bundle(raw: &str) -> Result<Bundle, BundleError>:
    // GUARD 1 — LENGTH CAP FIRST, on the RAW pasted string, BEFORE any decode/parse (DoS pre-filter).
    if raw.as_bytes().len() > MAX_RAW_LEN:
        return Err(TooLong)            // MUST reject here even if raw is not valid base64url (AC-W1-C10)

    // GUARD 2 — scheme prefix.
    body = raw.strip_prefix(BUNDLE_SCHEME).ok_or(BadScheme)?

    // GUARD 3 — base64url-decode (no pad).
    bytes = base64url_nopad_decode(body).map_err(|_| BadBase64)?

    // GUARD 4 — JSON parse.
    value = json_parse(bytes).map_err(|_| BadJson)?

    // GUARD 5 — STRICT SCHEMA (LOAD-BEARING): exactly the four keys, correct types/shapes.
    require value is object with EXACTLY keys {v, base_url, token, fp}   // missing OR extra -> reject
    require value.v == 1                                                  // unknown major -> reject (forward-compat)
    require value.base_url is string starting "https://"
    require value.token matches TOKEN_RE
    require value.fp matches FP_RE
    return Ok(Bundle{ v, base_url, token, fp })
```

Ordering is non-negotiable: GUARD 1 runs before GUARD 3/4 (an over-cap string that is NOT valid base64url must still reject on **length**, not on a decode error — AC-W1-C10). GUARD 5 is the load-bearing guard; GUARD 1 is belt-and-suspenders.

## State / lifecycle

`run_client_bundle` is a one-shot sync subcommand. Dispatched in `main.rs` C-10 block (~L247–389) alongside `Health`/`Version`:
```
Some(Command::ClientBundle) => return unimatrix_server::client_bundle::run_client_bundle(cli.project_dir),
```
Add `ClientBundle` to the `Command` enum (clap). No tokio, no tracing init beyond what sync subcommands use.

## Data flow

- **Input:** `project_dir` (-> data_dir); reads `token`, `tls/cert.pem`, `UNIMATRIX_PUBLIC_URL`.
- **Output:** stdout = `unimatrix-bundle:...` blob; stderr = base-url + fp echo (token redacted); returns `Ok(())`.
- **Round-trip invariant:** `decode_bundle(encode_bundle(v,b,t,f))` yields identical fields (R-05 scenario 3).

## Error handling

| Condition | Result |
|-----------|--------|
| data_dir / token / cert unreadable | `ServerError` naming the path + fix; no panic (no `.unwrap()`) |
| token not 64 hex | `ServerError::Config` (reuse token validation) |
| cert PEM invalid/empty | `ServerError::Config` |
| (decode side) over-cap / bad scheme / bad base64 / bad json / schema fail | `BundleError::{TooLong,BadScheme,BadBase64,BadJson,Schema(reason)}` — each a clean reject, never a crash |

Bundle errors carry no token (it is never in an error message).

## Key test scenarios (hints for tester)

- Round-trip: encode -> decode yields identical `{v,base_url,token,fp}` (R-05 scenario 3).
- Canonical order: encoded JSON key order is exactly `v,base_url,token,fp` (fixture-stable for parity).
- stdout = blob ONLY (no trailing prose, pipeable); stderr has base-url + fp; **token absent from stdout AND stderr AND logs** (AC-W1-S5b, NFR-06).
- bundle `fp` == `fingerprint_leaf_der` of the *served* leaf DER (AC-W1-S4, ties to fingerprint-computer.md).
- Unset `UNIMATRIX_PUBLIC_URL` -> placeholder base-url + stderr WARNING (FR-A7/A5b pairing).
- Decode guard ordering: over-cap raw string that is invalid base64url rejects on **length** before decode (AC-W1-C10).
- Strict schema: missing field, extra field, wrong type, `v:2`, non-https base_url, non-hex token, malformed fp -> all reject (AC-W1-C9, R-05 scenario 1).
- Bad scheme prefix, non-base64url body, truncated payload -> reject, process survives (R-05 scenario 2).
```
