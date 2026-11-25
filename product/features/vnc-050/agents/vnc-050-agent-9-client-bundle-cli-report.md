# vnc-050-agent-9-client-bundle-cli — Wave 2 Report (C7)

## Report
Created: src/client_bundle/{args.rs, args_tests.rs, tokens_cli.rs}, src/mcp/legacy_token_http_tests.rs, tests/client_bundle_headless_e2e.rs, tests/bundle_codec_fp_null.rs.
Moved (git mv): src/client_bundle.rs -> src/client_bundle/mod.rs; src/client_bundle_tests.rs -> src/client_bundle/tests.rs.
Modified: src/main.rs (ClientBundle(ClientBundleArgs) + dispatch), src/projects/slug_store.rs (is_registered_slug), src/http/principal.rs and src/http/token_table/write.rs (dead_code allows removed; list_for is now #[cfg(test)]), src/mcp/cap_ceiling.rs (registers the test module), src/mcp/cap_ceiling_http_tests.rs (post_at/open_at/call_at slug variants; the alpha wrappers are unchanged), tests/bundle_codec.rs (fp is Option), tests/client_bundle_e2e.rs (fp is Option), tests/fixtures/c1c2-parity/bundle-golden.json (one fp:null row added; existing rows byte-identical).

Tests: cargo test --workspace: 7240 passed, 0 failed. New: 13 binary E2E, 3 flag-matrix, 2 render/encode unit, 1 rfc3339, 2 fp-null parity, 2 legacy HTTP (two slugs; corrupt tokens.json).

Deviations / issues:
- The flag-matrix tests are in client_bundle/args_tests.rs instead of tests.rs. Reason: the 500-line cap.
- action() also rejects a bad --label regex with exit 2. The test plan lists it as a usage error; mint would otherwise exit 1.
- Registration check: is_registered_slug lives in projects/slug_store.rs. Reason: projects.rs is at 500 lines. It reads [[projects]] from data_dir/config.toml, the same source `project list` uses.
- The legacy HTTP tests use context_lookup and context_deprecate because the test embed service is never ready (#5828).
- Not done: ac16_end_to_end (real server + mcp-bridge.js) and audit_attribution_retained_after_revoke. Both need a booted server binary or the JS bridge, owned by the tester/JS side.
- JS decoder: the new golden row has fp:null. bundle.test.js and remote-client.test.js (packages/, not touched) fail until the JS decoder accepts null.
- SR-05 met: no `token` command, no mint API, no second bundle format.
- Nothing committed.

## Knowledge Stewardship
- Queried: context_search vnc-050 decisions -- #5815 ADR-005 (applied), #5818 ADR-006; #5827 boot fallback (not relevant to the sync CLI path).
- Stored: entry #5828 "In-process HTTP tool tests (make_server): embed service is never ready, so use embedding-free tools" via context_store (pattern).
