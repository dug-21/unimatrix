# Agent Report — vnc-038 Component 1: Bundle Codec (Rust, sole encoder)

Agent ID: vnc-038-agent-3-bundle-codec-rust
ADR: ADR-002 (#5081), ADR-008 (#5088) · AC: AC-05, AC-11 · Risk: R-03, R-04

## Summary

Implemented the `v:2` bundle codec — the sole encoder. Bundle struct is now
`{v:2, mcp_url, observe_url, token, fp}` (was `{v:1, base_url, token, fp}`).
`BUNDLE_VERSION` bumped to 2. URLs are composed server-side from `{public_base}` +
route grammar via the new `compose_route_urls`. `validate_schema` enforces exactly
5 keys, `v == 2`, `https://` on both URLs, and the token/fp grammar. Guard ordering
preserved (length cap → scheme → base64url → JSON → strict schema). A `v:1` bundle
fails closed with a re-issue message (R-04). Parity corpus regenerated from the Rust
oracle to v:2 and committed.

## Files modified

- `crates/unimatrix-server/src/client_bundle.rs` — v:2 struct, `BUNDLE_VERSION=2`,
  `compose_route_urls` (NEW route-grammar owner), `encode_bundle`/`decode_bundle`/
  `validate_schema` two-URL signatures, `run_client_bundle(project_dir, slug)` now
  per-project (mandatory slug, loud Config error on invalid/missing), `emit_bundle`/
  `render_output` echo both URLs (token never on stdout/stderr). 423 lines.
- `crates/unimatrix-server/src/client_bundle_tests.rs` — NEW. Private-helper unit
  tests split out via `#[cfg(test)] #[path] mod tests;` to keep the source file
  under the 500-line cap (C-06). 165 lines.
- `crates/unimatrix-server/src/main.rs` — `Command::ClientBundle { slug: String }`
  (was unit variant); dispatch passes `&slug` to `run_client_bundle`.
- `crates/unimatrix-server/tests/bundle_codec.rs` — parity corpus + oracle rewritten
  for v:2: `GoldenFields {v, mcp_url, observe_url, token, fp}`, encode/round-trip,
  full strict-reject matrix, v:1 hard-cut tests, guard-ordering, golden drift guard.
  451 lines.
- `crates/unimatrix-server/tests/fixtures/c1c2-parity/bundle-golden.json` —
  REGENERATED from the Rust oracle as v:2 (3 rows incl. IPv6-literal authority).
  This is the shared oracle the JS decoder (Component 2) will consume.

## Tests

- Integration `tests/bundle_codec.rs`: **21 passed, 0 failed, 1 ignored** (the
  ignored is the `#[ignore]` oracle generator). Includes round-trip, URL
  composition, strict-reject matrix (missing/extra/wrong-type/non-https/unknown-
  major), v:1 hard-cut (`test_reject_v1_shaped_bundle`, `test_no_v1_fallback_decode_path`),
  guard-ordering (`test_max_raw_len_runs_first`), and the golden drift guard.
- Corpus regeneration: **RC=0** — v:2 golden fixture written and verified.
- Lib unit tests (`client_bundle_tests.rs`): could not LINK in this run because
  Component 5's sibling test modules (`http/router/tests.rs`,
  `infra/projects_config_tests.rs`) still reference the removed `ProjectKey::Default`
  / `DefaultResolver` and fail to compile. NONE of those errors are in my files.
  My production code is clippy-clean on the `--lib` (non-test) build. My unit tests
  will run green once Component 5 updates its own test modules.

## Issues / blockers

- **Shared-worktree concurrency (non-blocking for my component):** the crate
  oscillated through broken intermediate states while Components 5 (resolver) and 6
  (observe) landed their edits to `seam.rs`, `default_resolver.rs`, `router.rs`,
  and their test modules. I waited for clean-build windows; my integration suite +
  corpus regen ran green. The remaining lib-test link failure is entirely in
  Components 5's test files — flag for the Delivery Leader: the `--lib` test target
  for the whole crate will stay red until Component 5 finishes updating
  `http/router/tests.rs` and `infra/projects_config_tests.rs`.
- I ran `cargo fmt` only on my own files via `rustfmt <my files>` (NOT `cargo fmt -p`)
  to avoid clobbering other agents' in-flight edits, per the swarm shared-worktree
  hazard.

## Cross-component contract notes for the JS decoder (Component 2, next wave)

The v:2 wire contract is now LOCKED by the Rust oracle. Component 2 must mirror it
byte-identically:

- Canonical JSON (exact key order): `{"v":2,"mcp_url":"...","observe_url":"...","token":"...","fp":"..."}`.
- `EXPECTED_KEYS = ["v","mcp_url","observe_url","token","fp"]` — exactly 5; reject
  missing OR extra.
- `obj.v !== 2` → reject (no v:1 compat arm; a v:1 bundle must fail closed with a
  re-issue message — R-04).
- `mcp_url` and `observe_url` must both be `https://` strings, posted VERBATIM (no
  append/normalization — ADR-001).
- `token` `^[0-9a-f]{64}$`; `fp` `^sha256:[0-9a-f]{64}$` (unchanged).
- Guard ordering unchanged: length cap (`MAX_RAW_LEN=4096`, raw bytes FIRST) →
  scheme (`unimatrix-bundle:`) → base64url-nopad decode → JSON → strict schema.
- **Golden corpus the JS test consumes:**
  `crates/unimatrix-server/tests/fixtures/c1c2-parity/bundle-golden.json` — 3 v:2
  rows; the JS decoder decodes each `row.wire` and asserts equality with `row.fields`.
  Error messages must NEVER include the token (NFR-06).
- `settings.local.json` subtree change (Components 3/4): `unimatrix.remote` now
  carries `{mcp_url, observe_url, token, fingerprint?}` (was `{url, ...}`).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` / `context_search` -- the Unimatrix
  MCP context tools were NOT available in this session (ToolSearch returned no
  matching deferred tools for the context_* family; the SubagentStart hook surfaced
  one tangential lesson about long-wait Wave-2 re-reads, not codec-relevant). Per
  the non-blocking rule, proceeded without. Read ADR-002/ADR-008 content via the
  architecture/pseudocode/test-plan files instead.
- Stored: nothing novel to store via the MCP path (tools unavailable). The one
  pattern worth recording for a future steward: in a shared-worktree swarm, run
  `rustfmt <only-my-files>` rather than `cargo fmt -p <crate>` — the package-scoped
  formatter rewrites every file in the crate and will clobber other agents'
  in-flight edits (matches the existing swarm-shared-worktree hazard memory).
