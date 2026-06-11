# Agent Report — vnc-034-agent-3-public-url (PublicUrl, C3)

## Summary
Implemented `derive_public_url(env: &Env) -> PublicUrl` — the C3 single-derivation
knob — in a new module `crates/unimatrix-server/src/http/public_url.rs`. One read of
`UNIMATRIX_PUBLIC_URL` produces the single `PublicUrl { base_url, host, sans }` value
that all three consumers (bundle base-url, allowed_hosts default, cert SAN) read, so
`bundle.host ∈ cert.sans` (SR-10) holds by construction. Socket auto-detect is rejected
(pure function over `Env`, no I/O).

## Files modified
- `crates/unimatrix-server/src/http/public_url.rs` (new) — module + 22 unit tests
- `crates/unimatrix-server/src/http/mod.rs` — added `pub(crate) mod public_url;` (the
  one shared line; the surrounding fingerprint re-exports were added concurrently by the
  tls.rs agent, not by me)

## Design notes
- `Env<'a>` is an injectable getter (`&dyn Fn(&str)->Option<String>`) per the locked
  pseudocode — Rust 2024 forbids `std::env::set_var` under `#![forbid(unsafe_code)]`, so
  tests exercise every branch with a `HashMap`-backed closure, no process-env mutation.
  `Env::from_process()` is the production accessor (env reads are safe; only set/remove
  are unsafe).
- Total function: unset / empty / un-parseable -> loud `https://<EDIT-ME>:8443`
  placeholder + WARN; non-https scheme -> coerced to https + WARN (ADR-001 requires
  base_url https). Never errors, never panics — so no `ServerError` is constructed (the
  brief's "errors via ServerError" is moot for a total function; the pseudocode is
  explicit that this function never errors).
- No `url` crate (not in the allowed list `rcgen/tokio-rustls/rustls-pemfile/rand`).
  Hand-rolled tolerant parser handles: missing scheme, http->https coercion,
  path/query/fragment discard, IPv6 literals (`[::1]` brackets kept in base_url, stripped
  in host/SAN), absent port -> 8443, userinfo strip, and host==local-SAN dedup.
- Module carries `#![allow(dead_code)]` with justification: the three consumer call sites
  are owned by sibling components (cert-provisioner, bundle-codec, listener/config wiring
  in main.rs) and are out of my scope; until they land, the module is referenced only by
  its own tests. The public API is the contract those owners consume.

## Scope adherence
Touched ONLY the new `public_url.rs` and the single `pub mod` line in `http/mod.rs`.
Did NOT edit tls.rs, router.rs, main.rs, client_bundle.rs.

## Tests
22 passed / 0 failed (`cargo test -p unimatrix-server --lib public_url`).
Covers: base_url verbatim, host extraction, exact sans vector, three-consumers-one-source,
host∈sans invariant across 4 URL shapes, unset placeholder + permissive-with-warning sans,
no-socket-autodetect (structural), env injectability, and edge cases (explicit port, path,
query/fragment, IPv6 ±port, no-scheme, no-port, http coercion, trailing slash, local-SAN
dedup, garbage->placeholder, bad-port->placeholder).
`cargo clippy -p unimatrix-server --lib`: no warnings from this module.
`cargo build -p unimatrix-server`: passes.

## Issues / blockers
- None blocking. One transient `signal: 9 (SIGKILL)` during a full lib-test compile
  (resource kill, not a code error); the targeted `--lib public_url` run compiled and
  passed cleanly.
- Downstream note for the wiring agents: `Env::from_process()` is the production
  constructor; `derive_public_url(&Env::from_process())` is the intended first-boot call.
  When all three consumers are wired, the `#![allow(dead_code)]` can be removed.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced vnc-034 ADRs (4953/4954/4951)
  but nothing specific to env-var URL parsing; proceeded on the pseudocode/ARCHITECTURE.
- Stored: pattern via /uni-store-pattern (Rust 2024 testable-env accessor for pure
  config-derivation functions under forbid(unsafe_code)).
