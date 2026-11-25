# C11 Docs + Scripted Smoke

ACs: AC-12, AC-13 (delivery, scripted local equivalent), AC-01 (doc statement). AC-13 live and AC-14 are post-delivery operator checks — no tests.

Smoke script (`product/test/` or `packages/unimatrix/test/` per pseudocode): starts local CA + server binary with temp data_dir, two slugs.

| Test | Assertion |
|---|---|
| `recipe_blocks_extracted_verbatim` | script extracts fenced blocks tagged for smoke from the remote-access guide; fails if zero blocks found or block text modified (hash compared to file) |
| `curl_recipe_runs_unmodified` | executed with only documented env vars substituted; initialize + tools/call succeed (AC-12) |
| `generic_mcp_client_recipe` | documented generic client config attaches |
| `ac13_local_read_bundle_search_ok_write_denied` | mint read token via CLI; bridge search OK; store → CapabilityDenied |
| `ac13_local_rw_token_writes_only_its_slug` | rw@A writes A; on B → 403 |
| `docs_contain_legacy_baseline_and_gh_actions_example` | grep checks |
| `docs_no_reject_unauthorized_false` | doc contains no `rejectUnauthorized` / `NODE_TLS_REJECT_UNAUTHORIZED=0` advice |
