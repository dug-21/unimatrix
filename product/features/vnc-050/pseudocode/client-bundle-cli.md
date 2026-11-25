# C7 client-bundle CLI — `client_bundle.rs` → `client_bundle/{mod.rs, args.rs, tokens_cli.rs}`, `main.rs`

ADR-005 (#5815), ADR-001 (#5814). Depends on C1 writers, C4 `TokenMode`. Each file < 500 lines; tests stay in `client_bundle/tests.rs` (moved from `client_bundle_tests.rs`, `#[path]` updated).

## Module Layout
- `mod.rs` — existing encode/decode/validate_schema/compose_route_urls/emit; `run_client_bundle(project_dir, args)` dispatch. `Bundle.fp` → `Option<String>`.
- `args.rs` — `ClientBundleArgs` (clap) + `Action` + `fn action(&self) -> Result<Action, UsageError>`.
- `tokens_cli.rs` — `run_mint`, `run_list`, `run_revoke`, stderr table.

## `args.rs`
```
#[derive(clap::Args, Debug)]
pub struct ClientBundleArgs {
    /// Registered project slug.
    pub slug: String,
    #[arg(long, requires_all = ["mode", "label"], conflicts_with_all = ["list", "revoke"])]
    pub headless: bool,
    #[arg(long, value_parser = ["read", "read-write"], requires = "headless")]
    pub mode: Option<String>,
    #[arg(long, requires = "headless")]
    pub label: Option<String>,
    #[arg(long, value_parser = ["pin", "ca"], conflicts_with_all = ["list", "revoke"])]
    pub trust: Option<String>,
    #[arg(long, conflicts_with_all = ["headless", "mode", "label", "trust", "revoke"])]
    pub list: bool,
    #[arg(long, value_name = "LABEL", conflicts_with_all = ["headless", "mode", "label", "trust", "list"])]
    pub revoke: Option<String>,
}
pub enum Trust { Pin, Ca }
pub enum Action {
    Legacy { trust: Trust },                                  // today's command (+ optional --trust)
    Mint   { mode: TokenMode, label: String, trust: Trust },
    List,
    Revoke { label: String },
}
pub struct UsageError(pub String);

impl ClientBundleArgs {
    /// Belt-and-braces re-check of the matrix (clap enforces it first). Pure; no I/O.
    pub fn action(&self) -> Result<Action, UsageError> {
        trust = match self.trust.as_deref() { None | Some("pin") => Pin, Some("ca") => Ca, _ => Err }
        match (self.headless, self.list, self.revoke.as_ref()):
          (true, false, None) -> mode = TokenMode::parse_wire(self.mode?)  else Err("--headless requires --mode read|read-write")
                                 label = self.label? else Err("--headless requires --label <name>")
                                 Ok(Mint{mode, label, trust})
          (false, true, None) -> if mode|label|trust set -> Err; Ok(List)
          (false, false, Some(l)) -> if mode|label|trust set -> Err; Ok(Revoke{label: l})
          (false, false, None) -> if mode|label set -> Err("--mode/--label require --headless"); Ok(Legacy{trust})
          _ -> Err("--headless, --list and --revoke are mutually exclusive")
    }
}
```
clap usage errors exit 2 before any I/O (clap default). `action()` errors: `run_client_bundle` prints `error: <msg>` + usage line to stderr and calls `std::process::exit(2)` (UsageError is not a ServerError, so it never maps to exit 1).

## `main.rs` (variant swap)
```
ClientBundle(unimatrix_server::client_bundle::ClientBundleArgs),
...
Some(Command::ClientBundle(args)) => unimatrix_server::client_bundle::run_client_bundle(cli.project_dir, args).map_err(Into::into),
```
Doc comment moves to `ClientBundleArgs`.

## `mod.rs` — `pub fn run_client_bundle(project_dir: Option<PathBuf>, args: ClientBundleArgs) -> Result<(), ServerError>`
```
action = match args.action() { Ok(a) => a, Err(UsageError(m)) => { eprintln!("error: {m}\nusage: unimatrix client-bundle <slug> [--headless --mode read|read-write --label <L>] [--trust pin|ca] | --list | --revoke <L>"); std::process::exit(2) } }
slug = ProjectSlug::try_from(&args.slug) (existing error text)            // all forms
data_dir = ensure_data_directory(...) (existing)
match action:
  Legacy{trust} -> token_hex = read_token_hex(&data_dir)?; emit_for(slug, data_dir, token_hex, trust, None)
  Mint{mode,label,trust} -> tokens_cli::run_mint(&data_dir, &slug, mode, &label, trust)
  List -> tokens_cli::run_list(&data_dir, &slug)
  Revoke{label} -> tokens_cli::run_revoke(&data_dir, &slug, &label)
```

### `fn emit_for(slug, data_dir, token_hex, trust, minted: Option<(&str, TokenMode)>) -> Result<(), ServerError>`
```
fp: Option<String> = match trust {
    Pin -> Some(fingerprint_leaf_der(leaf_der_from_pem(read cert.pem)?))     // existing code path
    Ca  -> None                                                              // cert not read
}
(mcp_url, observe_url) = compose_route_urls(derive_public_url(..).base_url, slug)
blob = encode_bundle(BUNDLE_VERSION, &mcp_url, &observe_url, &token_hex, fp.as_deref())?
emit_bundle(&blob, &mcp_url, &observe_url, fp.as_deref(), minted, slug)
```

### `encode_bundle(v, mcp_url, observe_url, token, fp: Option<&str>)`
`Bundle { .., fp: Option<String> }` with plain `#[derive(Serialize)]` (None → `null`, key always present; NO `skip_serializing_if`). Field order unchanged.

### `validate_schema` fp branch
```
fp = match obj.get("fp") {
    Some(Value::Null) => None,
    Some(Value::String(s)) if is_fingerprint(s) => Some(s.clone()),
    _ => return Err(Schema("'fp' must be null or match sha256:<64 lowercase hex>")),
}
```
Exact-5-key check unchanged (missing `fp` still rejects).

### `render_output(blob, mcp_url, observe_url, fp: Option<&str>, minted: Option<(&str, TokenMode)>, slug)`
```
stderr lines as today; cert-fp line:
    Some(fp) -> "  cert-fp     : {fp}"
    None     -> "  trust       : ca (no pin; server cert must chain to a trusted CA)"
if minted Some((label, mode)):
    "  label       : {label}" ; "  mode        : {mode.as_wire()}" ; "  slug        : {slug}"
    header line: "unimatrix headless bundle (set as UNIMATRIX_BUNDLE; shown once — store it now)"
stdout = blob only
```

## `tokens_cli.rs`
```
pub(super) fn run_mint(data_dir, slug, mode, label, trust) -> Result<(), ServerError> {
    ensure_registered(slug)?                         // FR-08 / ADR-005: --headless requires registered slug
    table = TokenTable::open(data_dir)
    secret = table.mint(label, mode, slug)?          // duplicate label (active or revoked) -> InvalidInput -> exit 1
    emit_for(slug, data_dir, hex::encode(secret), trust, Some((label, mode)))
       // if emit fails after mint (cert unreadable): return Err with message
       // "token '<label>' was minted but the bundle could not be emitted; revoke it with --revoke <label>"
}
```
Ordering note: for `--trust pin`, read and fingerprint cert.pem BEFORE `mint` so a missing cert fails without creating an orphan record. Implement by computing `fp` first, then mint, then encode/emit.

```
fn ensure_registered(slug) -> Result<(), ServerError>
    // Same source `project list` uses (projects.rs ProjectRegistry / per_slug_data_dir).
    // Registered = per-slug dir exists with its db (ProjectStatus.store_open == Some(true) or dir present).
    // Not registered -> ServerError::Config("project '<slug>' is not registered; run `unimatrix project register <slug>` first")

pub(super) fn run_list(data_dir, slug) -> Result<(), ServerError> {
    rows = TokenTable::open(data_dir).list_for_checked(slug)?
    eprintln!("{:<32} {:<10} {:<24} {:<20} {}", "LABEL","MODE","SLUGS","CREATED","STATE")
    for r in rows: eprintln!(label, mode.as_wire(), slugs.join(","), rfc3339(created_at), revoked_at.map(|t| format!("revoked {}", rfc3339(t))).unwrap_or("active"))
    if rows.is_empty(): eprintln!("(no scoped tokens for {slug})")
    // stdout empty; never hash/secret
}

pub(super) fn run_revoke(data_dir, slug, label) -> Result<(), ServerError> {
    TokenTable::open(data_dir).revoke(label, slug)?            // unknown / other slug / already revoked -> Err, file unchanged
    eprintln!("revoked token '{label}' for {slug}; next request using it gets 401")
}
```
Exit codes: usage → 2; operational errors (`ServerError`) → existing main mapping (1).

## Key Test Scenarios (binary-driven, R-14)
- Every invalid combination → exit 2, `tokens.json` mtime unchanged / not created: `--mode` alone, `--label` alone, `--headless` w/o mode, w/o label, `--list --trust ca`, `--revoke x --headless`, `--list --revoke x`, `--mode admin`.
- Mint: stdout = one blob line only; secret appears nowhere else (stderr, tokens.json); `--list` shows label/mode/slug; `--trust ca` → decoded fp null, stderr `trust : ca`.
- Re-mint same label (active / revoked) → exit 1, file unchanged. Unregistered slug → error, no file write.
- Revoke unknown / other slug's label / already revoked → non-zero, byte-identical file.
- Legacy form `client-bundle <slug>` output byte-identical to pre-vnc-050 (pinned); `--trust ca` without `--headless` → legacy token + fp null.
- Parity corpus (`tests/bundle_codec.rs` + JS): `fp:null` accepted; `""`, `"sha256:"`, uppercase hex, missing key, extra key rejected by both.
