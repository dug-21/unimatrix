# C7 client-bundle CLI — `client_bundle/{mod,args,tokens_cli}.rs`, `main.rs`

Risks: R-05, R-09, R-12, R-14, R-10. ACs: AC-16, AC-17, FR-07. Behavioral tests run the built binary (extend `tests/client_bundle_e2e.rs`).

## Unit — flag matrix (`args.rs`)
Table test, each invalid combo → usage error (exit 2): `--mode` without `--headless`; `--label` without `--headless`; `--headless` without `--mode` or `--label`; `--trust` with `--list`/`--revoke`; `--list` with `--revoke`; `--list`/`--revoke` with `--headless`; bad mode; bad label regex; bad `--trust` value. Companions: each valid shape parses.

## Binary E2E
| Test | Assertion |
|---|---|
| `invalid_combos_exit_2_no_io` | each combo: exit 2; `tokens.json` absent stays absent, present keeps mtime + sha256 |
| `headless_mint_stdout_blob_only` | stdout == one bundle line; decodes v:2 with mode slug; secret appears nowhere in stderr, file (only hash) |
| `trust_ca_emits_fp_null`; `trust_pin_default_emits_fp` | decoded `fp` null / `sha256:<64hex>` |
| `list_after_two_mints` | stderr table has both labels, modes, slug; stdout empty; no secret or hash sentinel |
| `revoke_then_list_shows_revoked_and_remint_fails` | AC-17; re-mint exit non-zero, file byte-identical |
| `revoke_other_slug_label_errors` | non-zero, file byte-identical |
| `revoke_unknown_or_twice_errors` | non-zero, byte-identical |
| `twenty_parallel_cli_mints` | 20 processes → 20 entries, parses, 0600, no tmp (R-12) |
| `audit_attribution_retained_after_revoke` | `http_access` rows for label still resolvable (AC-17) |
| `ac16_end_to_end` | real server process; mint read bundle via binary; spawn `mcp-bridge.js` with only `UNIMATRIX_BUNDLE`, clean HOME; initialize + search OK; store denied; mint rw bundle → observe event lands; CLI `--revoke`; next call in the open session → 401; no `remote.json` created |
| `existing_interactive_mode_unchanged` | no-flag `client-bundle <slug>` output identical to main (golden) |
