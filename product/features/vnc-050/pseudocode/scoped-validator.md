# C2 Scoped Validator — `http/auth/scoped.rs` (new), `http/auth.rs` (wire)

ADR-001 (#5814), ADR-002 (#5806). Depends on C1 (`TokenTable::lookup_ct`), C4 (`BearerPrincipal`, `auth_identity`), C5 (enabled flag).

## Purpose
Validate bearer tokens: legacy constant-time compare + constant-time full scan of the token table. On success insert both `ResolvedIdentity` (existing consumers) and `BearerPrincipal` (C3, C4b) into request extensions.

## Interface Gap (flagged) and Resolution
`BearerValidator::validate` returns only `ResolvedIdentity`; the principal must also cross the seam. Resolution (backward-compatible): add a second trait method with a default body, so `StaticTokenValidator` and existing tests compile unchanged:

```
trait BearerValidator {
    fn validate(&self, token: &str) -> BoxFuture<Result<ResolvedIdentity, AuthError>>;          // unchanged
    fn validate_principal(&self, token: &str)
        -> BoxFuture<Result<(ResolvedIdentity, BearerPrincipal), AuthError>> {
        // default: legacy-shaped
        let fut = self.validate(token);
        Box::pin(async move { fut.await.map(|id| (id, BearerPrincipal::Legacy)) })
    }
}
```
`StaticTokenAuth::call` switches to `validate_principal`. Also `TokenTableValidator::new(legacy, table)` (architecture signature) cannot express "legacy present but disabled" (needed for the FR-11 body); add builder `with_legacy_enabled(self, enabled: bool) -> Self` (default true).

## `AuthError` change (`http/auth.rs`)
Add variant `LegacyDisabled`. Display: `"legacy token disabled"`.

## `TokenTableValidator`
```
pub(crate) struct TokenTableValidator {
    legacy: Option<[u8; 32]>,
    legacy_enabled: bool,
    table: Arc<TokenTable>,
}
pub(crate) fn new(legacy: Option<[u8;32]>, table: Arc<TokenTable>) -> Self { legacy, legacy_enabled: true, table }
pub(crate) fn with_legacy_enabled(mut self, enabled: bool) -> Self
```

### `fn validate_sync(&self, token: &str) -> Result<(ResolvedIdentity, BearerPrincipal), AuthError>`
```
bytes = hex::decode(token).map_err(|_| InvalidToken)?          // early reject allowed (ADR-001)
if bytes.len() != 32 -> Err(InvalidToken)
presented: [u8;32] = bytes.try_into()

// (1) legacy compare — always computed, even when disabled
legacy_match: bool = match self.legacy { Some(l) => l.ct_eq(&presented).into(), None => false }
// (2) full scan — always run, regardless of legacy result (no short-circuit between 1 and 2)
scoped: Option<ScopedEntry> = self.table.lookup_ct(&presented)

if legacy_match:
    if !self.legacy_enabled -> Err(LegacyDisabled)
    p = BearerPrincipal::Legacy
else match scoped:
    Some(e) -> p = BearerPrincipal::Scoped{ label: e.label, mode: e.mode, allowed_slugs: e.allowed_slugs }
    None    -> Err(InvalidToken)        // unknown, revoked tombstone, corrupt table: identical
Ok((principal::auth_identity(&p), p))
```
Legacy identity from `auth_identity(&Legacy)` must equal today's `StaticTokenValidator` output exactly (`http-bearer`, Restricted, Read/Write/Search/SessionWrite) — FR-39.

`validate` (trait) = `validate_sync(token).map(|(id, _)| id)`; `validate_principal` overrides default with `validate_sync`.

## `StaticTokenAuth::call` changes (`http/auth.rs`)
```
match validator.validate_principal(&token).await:
  Ok((identity, principal)) -> ext.insert(identity); ext.insert(principal); inner.call(request)
  Err(AuthError::LegacyDisabled) -> Ok(unauthorized_response_with(r#"{"error":"legacy token disabled"}"#))
  Err(_) -> Ok(unauthorized_response())            // unchanged body
```
Refactor `unauthorized_response()` into `unauthorized_with(body: &'static str)`; existing fn calls it with the existing body (byte-identical).

Add constructor `StaticTokenAuthLayer::with_validator(v: Arc<dyn BearerValidator>) -> Self`. Existing `new(token_bytes)` unchanged.

## Boot Wiring (`main.rs` near line 1524 — 3 lines; flagged: architecture lists main.rs as variant swap only)
```
let table = Arc::new(TokenTable::open(&data_dir));
let validator = TokenTableValidator::new(Some(token_array), table).with_legacy_enabled(config.http.legacy_token_enabled);
let auth_layer = StaticTokenAuthLayer::with_validator(Arc::new(validator));
```
Same `data_dir` that holds `{data_dir}/token`.

## Error Handling
All failures → 401; body never echoes the token. Corrupt `tokens.json` → scoped None → 401; legacy unaffected (C1 never panics).

## Key Test Scenarios (test seams: OVERVIEW §Test-Only Seams)
Full-scan assertions use `TokenTable::lookup_ct_counted` (visits == table length on hit-at-0, miss, tombstone).
- Legacy enabled → 200 on 2 slugs (AC-01); disabled → 401 body exactly `{"error":"legacy token disabled"}`, no token bytes.
- Scoped read/rw → principal inserted with correct mode/slugs; revoked → 401 identical to unknown (status + body).
- Wrong length, non-hex, uppercase hex (hex::decode accepts uppercase — matches legacy behavior; hash compare still exact), `Bearer  x` (two spaces), missing header → 401.
- Revoke while server runs: next request 401 without restart (AC-16), including inside an open MCP session.
- Corrupt table: scoped 401, legacy 200.
- Static: no early return between legacy compare and scan.
