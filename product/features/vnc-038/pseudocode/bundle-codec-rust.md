# Component 1 — Bundle Codec (Rust, sole encoder)

**File:** `crates/unimatrix-server/src/client_bundle.rs`
**ADR:** ADR-002 (#5081), ADR-008 (#5088) · **AC:** AC-05, AC-11 · **Risk:** R-03, R-04

## Purpose

Sole encoder of the `v:2` client bundle. Composes BOTH the MCP and observe endpoint URLs from one route-grammar helper and emits `unimatrix-bundle:<base64url(canonical-json)>`. The Rust `decode_bundle` mirrors the JS decoder for round-trip/corpus parity. The token travels ONLY inside this blob (ADR-008).

## Modified Types

```
CHANGE const BUNDLE_VERSION: u8  from 1 to 2
KEEP   const BUNDLE_SCHEME = "unimatrix-bundle:", const MAX_RAW_LEN = 4096

REPLACE struct Bundle:
  // was { v, base_url, token, fp }
  struct Bundle {
    v: u8,            // declaration order IS canonical JSON key order — do not reorder
    mcp_url: String,
    observe_url: String,
    token: String,
    fp: String,
  }
  derive(Serialize) with NO map reordering (serde struct = declaration order)

KEEP enum BundleError { TooLong, BadScheme, BadBase64, BadJson, Schema(String) }
     (no new variant — a malformed v:2 URL is a Schema(...) rejection)
```

## New / Modified Functions

### `compose_route_urls` (NEW — the single route-grammar owner, ADR-002)

```
fn compose_route_urls(public_base: &str, slug: &ProjectSlug) -> (String, String):
    // public_base e.g. "https://cloud.example:8443" (no trailing slash);
    // normalize defensively: strip trailing '/' from public_base.
    base  = public_base.trim_end_matches('/')
    mcp_url     = format!("{base}/v1/{slug}")            // MCP root for the slug
    observe_url = format!("{base}/v1/{slug}/observe")    // observe segment under the slug
    return (mcp_url, observe_url)
    // INVARIANT: this is the SAME grammar parse_project_key routes by, so the
    // bundle URL can never disagree with the live route (ADR-002 rationale).
```

### `run_client_bundle` (MODIFY — now per-project, takes a slug)

```
fn run_client_bundle(project_dir: Option<PathBuf>, slug_arg: &str) -> Result<(), ServerError>:
    paths     = ensure_data_directory(project_dir)            // unchanged
    slug      = ProjectSlug::try_from(slug_arg)               // validate at the edge; map_err -> ServerError::Config
    token_hex = read_token_hex(&paths.data_dir)               // unchanged; never logs token
    cert_pem  = read {data_dir}/tls/cert.pem                  // unchanged
    fp        = fingerprint_leaf_der(leaf_der_from_pem(cert_pem))
    public    = derive_public_url(&Env::from_process()).base_url   // unchanged source (public_url.rs)
    (mcp_url, observe_url) = compose_route_urls(&public, &slug)
    blob      = encode_bundle(BUNDLE_VERSION, &mcp_url, &observe_url, &token_hex, &fp)?
    emit_bundle(&blob, &mcp_url, &observe_url, &fp)           // stdout = blob ONLY; token never in stderr
    Ok(())
```

> NOTE: the `client-bundle` subcommand must now accept a `<slug>` argument. The `main.rs` sync dispatch block that calls `run_client_bundle` is updated to parse it. If `slug` is absent or invalid → loud `ServerError::Config` ("client-bundle requires a registered <slug>"), never a default-aliased bundle (ADR-001/004).

### `encode_bundle` (MODIFY signature — two URLs)

```
fn encode_bundle(v: u8, mcp_url: &str, observe_url: &str, token_hex: &str, fp: &str)
    -> Result<String, ServerError>:
    bundle = Bundle { v, mcp_url:owned, observe_url:owned, token:owned, fp:owned }
    json   = serde_json::to_string(&bundle).map_err(|e| ServerError::Config("bundle JSON encode failed: {e}"))?
    b64    = URL_SAFE_NO_PAD.encode(json.bytes())
    return Ok("{BUNDLE_SCHEME}{b64}")
    // field order in `json` is fixed by Bundle declaration order -> fixture-stable
```

### `emit_bundle` / `render_output` (MODIFY — echo both URLs on stderr, token never)

```
fn render_output(blob, mcp_url, observe_url, fp) -> (stdout: String, stderr: String):
    stdout = blob                                            // opaque, one line, pipeable
    stderr = lines:
       "unimatrix connection bundle (paste into: unimatrix init --bundle <bundle>)"
       "  mcp-url     : {mcp_url}"
       "  observe-url : {observe_url}"
       "  cert-fp     : {fp}"
       if mcp_url contains "<EDIT-ME>": warn "UNIMATRIX_PUBLIC_URL unset — placeholder; set and re-run"
    return (stdout, stderr)
    // ADR-008: the TOKEN appears in NEITHER stdout-echo NOR stderr; it lives only inside `blob`.
```

### `decode_bundle` (MODIFY — mirror JS v:2 for parity tests)

```
fn decode_bundle(raw: &str) -> Result<Bundle, BundleError>:
    GUARD 1  if raw.len() > MAX_RAW_LEN -> Err(TooLong)            // raw bytes, FIRST
    GUARD 2  body = raw.strip_prefix(BUNDLE_SCHEME) ok_or BadScheme
    GUARD 3  bytes = URL_SAFE_NO_PAD.decode(body) map_err BadBase64
    GUARD 4  value: serde_json::Value = from_slice(bytes) map_err BadJson
    GUARD 5  validate_schema(value)
```

### `validate_schema` (MODIFY — exact 5 keys, v==2, two https URLs)

```
fn validate_schema(value) -> Result<Bundle, BundleError>:
    obj = value.as_object() ok_or Schema("payload is not a JSON object")
    if obj.len() != 5: Err(Schema("expected exactly 5 keys (v, mcp_url, observe_url, token, fp), found {n}"))
    for key in obj.keys():
        if key not in {"v","mcp_url","observe_url","token","fp"}: Err(Schema("unexpected key '{key}'"))
    v = obj["v"].as_u64() ok_or Schema("'v' must be an integer")
    if v != BUNDLE_VERSION(2): Err(Schema("unsupported bundle version {v} (expected 2)"))   // R-04
    mcp_url = obj["mcp_url"].as_str() ok_or Schema("'mcp_url' must be a string")
    if not mcp_url.starts_with("https://"): Err(Schema("'mcp_url' must be https://"))
    observe_url = obj["observe_url"].as_str() ok_or Schema("'observe_url' must be a string")
    if not observe_url.starts_with("https://"): Err(Schema("'observe_url' must be https://"))
    token = obj["token"].as_str() ok_or Schema("'token' must be a string")
    if not is_token(token): Err(Schema("'token' must be 64 lowercase hex characters"))   // reason carries NO token value
    fp = obj["fp"].as_str() ok_or Schema("'fp' must be a string")
    if not is_fingerprint(fp): Err(Schema("'fp' must match sha256:<64 lowercase hex>"))
    return Ok(Bundle { v:2, mcp_url, observe_url, token, fp })
```

`is_token` (`^[0-9a-f]{64}$`) and `is_fingerprint` (`^sha256:[0-9a-f]{64}$`) — UNCHANGED.

## Parity corpus (extend, do not re-scaffold — NFR-02/NFR-07, #4956)

```
- Re-export the oracle fns the corpus tests consume (encode_bundle / decode_bundle) at the
  pub(crate) module surface the `tests/` integration target imports (#4956 mechanic).
- Replace v:1 hex fixtures with v:2 hex vectors: encode a known {v:2, mcp_url, observe_url, token, fp},
  store the hex of the wire bytes; the JS test (Component 2) decodes the SAME hex and asserts field equality.
- Add reject fixtures: missing key, extra key, wrong-type, non-https mcp_url, non-https observe_url,
  v:1-shaped payload (obj.v==1) -> Schema reject.
```

## Error Handling

- Encode errors → `ServerError::Config` with context (cannot fail for owned String/u8 in practice, but propagated, never `.unwrap()`).
- Decode errors → `BundleError`; messages NEVER include the token (NFR-06).
- Slug arg invalid → `ServerError::Config`, loud, no default bundle.

## Key Test Scenarios (hints for the tester)

1. Round-trip: `encode_bundle(2, mcp, obs, tok, fp)` → `decode_bundle` → field equality (R-03 sc.1).
2. Strict-reject matrix: exactly-5-keys violated (4 or 6 keys), wrong `v` (1 or 3), non-https `mcp_url`/`observe_url`, bad token/fp → each `BundleError::Schema` (R-03 sc.2).
3. Guard ordering: an over-cap raw string that is NOT valid base64url rejects on `TooLong`, not `BadBase64` (R-03 sc.3 / NFR-08).
4. Hex-vector parity with the JS decoder over the SAME bytes (the shared oracle).
5. `render_output`: token substring appears in NEITHER stdout NOR stderr (AC-11 unit-level).
6. `run_client_bundle("badslug!")` → loud Config error; no blob emitted.
