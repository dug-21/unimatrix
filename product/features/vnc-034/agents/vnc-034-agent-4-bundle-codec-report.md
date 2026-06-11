# Agent Report — vnc-034-agent-4-bundle-codec

**Component:** BundleCodec (C1 build-first encoder, ADR-001) — Rust side
**Wave:** 1 (sub-deliverable C1/C2 contract)
**Status:** COMPLETE

## Summary

Implemented the C1 connection-bundle codec as a new self-contained module
`crates/unimatrix-server/src/client_bundle.rs` exposing the LOCKED signature
`pub fn run_client_bundle(project_dir: Option<PathBuf>) -> Result<(), ServerError>`
(sync, pre-tokio C-10). It consumes the committed Wave-1 symbols
`leaf_der_from_pem` + `fingerprint_leaf_der` (http/tls.rs) and `derive_public_url`
(http/public_url.rs) — none of those files were edited.

The wire form is exactly `unimatrix-bundle:<base64url-nopad(canonical-json)>` with
canonical field order `v,base_url,token,fp` (serde struct declaration order; never a
HashMap). The decode half (`decode_bundle`) mirrors the JS guard ordering byte-for-byte
(length-cap FIRST, then scheme, base64url, JSON, then the load-bearing strict schema).

### Output contract (FR-A5b / NFR-06) — HARD requirement met
- **stdout = the opaque blob ONLY** (single line, pipeable, no prose).
- **stderr = base-url + cert-fingerprint echo ONLY; token OMITTED**, with an `<EDIT-ME>`
  placeholder WARNING when `UNIMATRIX_PUBLIC_URL` is unset.
- The token appears in NEITHER stdout NOR stderr NOR any log line — it lives only inside
  the base64url blob payload. Proven by `test_client_bundle_token_absent_from_stdout_and_stderr`.

Achieved testability of the redaction contract by splitting a pure
`render_output(...) -> (String, String)` from the thin printing `emit_bundle` — no binary
spawn needed (and the bin link is OOM-prone in this environment).

## main.rs

NOT touched. The module is self-contained and fully unit/integration tested without main.rs
wiring. Declared only as `pub mod client_bundle;` in `lib.rs` (sibling of the other sync
subcommands like `health`). Sub-wave 3 owns the `Command::ClientBundle` dispatch + clap enum.

## Files Created / Modified

- `crates/unimatrix-server/src/client_bundle.rs` (new, 460 lines — under the 500 cap)
- `crates/unimatrix-server/tests/bundle_codec.rs` (new — public-API guard/schema/round-trip
  tests + the C1 golden oracle, mirroring `tests/fingerprint_parity.rs`)
- `crates/unimatrix-server/tests/fixtures/c1c2-parity/bundle-golden.json` (new — generated
  by the oracle; consumed byte-identically by the JS decoder test #725, never hand-written)
- `crates/unimatrix-server/src/lib.rs` (added single `pub mod client_bundle;` line)
- `crates/unimatrix-server/Cargo.toml` (added `base64 = "0.22"` — same workspace version
  already used by unimatrix-store; no new lockfile entry)

## Tests

- Lib unit tests (private helpers — render split, read_token_hex, validators): **8 passed, 0 failed**
- Integration tests (public API + golden oracle + drift guard): **17 passed, 0 failed, 1 ignored** (the `#[ignore]` oracle regen)
- Total: **25 active tests pass, 0 failures**

Coverage maps to: AC-W1-S5b (stdout/stderr split + token redaction), AC-W1-C9 (strict-schema
load-bearing), AC-W1-C10 (4 KB cap before decode — proven by asserting the `TooLong` variant
on an over-cap non-base64url string), AC-W1-S4/AC-CT-C2 (fp = fingerprint_leaf_der of served
leaf DER), R-05.3 (round-trip), R-05 (parser-never-crashes corpus).

`cargo clippy -p unimatrix-server --tests` is clean for both new files.

## Constraints honored

no new crates (base64 already workspace-present) · no `unsafe` (crate has
`#![forbid(unsafe_code)]`) · no `.unwrap()`/`.expect()` in non-test code · max 500 lines/file
(460) · errors via `ServerError` · did NOT edit tls.rs / public_url.rs / router.rs.

## Issues / Blockers

None for this component. Two environmental notes (NOT my code):
- The full `unimatrix` BINARY link OOM-kills (`ld signal 9`) in this container — a resource
  limit on the 257-object-file link, not a compile error. The lib + lib-test + integration-test
  targets all build and pass. Use `--lib` / `--test bundle_codec` (with `CARGO_BUILD_JOBS<=2`)
  to validate without the heavy bin link.
- Transient mid-edit build errors seen in `http/cert_provisioner.rs` were a parallel agent's
  in-progress file; resolved on its own — my module never depended on the broken state.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) — surfaced #4577 (sync CLI
  subcommand pattern), #4954 (ADR-001 wire form), #4952 (ADR-006 wave mapping). Applied the
  sync-subcommand + canonical-encode guidance.
- Stored: entry #4960 "Bundle-codec stdout/stderr split: render to strings, then print
  (testable redaction)" via /uni-store-pattern — captures the render/emit split for testable
  token redaction, the load_or_generate_token contamination trap, the token-free-error rule,
  and the base64 dependency note.
