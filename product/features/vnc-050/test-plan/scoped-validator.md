# C2 Scoped Validator — `http/auth/scoped.rs`, `http/auth.rs`

Risks: R-05, R-06, R-09, R-15 (legacy path). ACs: AC-01, AC-10, AC-16.

## Unit tests
| Test | Assertion |
|---|---|
| `legacy_token_matches_returns_legacy_principal` | `BearerPrincipal::Legacy` |
| `scoped_match_index_0_and_n_minus_1` | both resolve to correct entry (R-06 control flow) |
| `tombstone_match_rejected_after_full_scan` | 401; `lookup_ct_counted` visits == n |
| `presented_token_wrong_length_nonhex_uppercase` | 401, no panic |
| `no_early_exit_scan_visits_all_entries` | `lookup_ct_counted` visits == table length for hit at 0, miss, tombstone |
| `static_no_hashmap_no_break` | source-grep test (`include_str!("scoped.rs")`) asserts no `HashMap<` keyed lookup and no `break`/`return` inside the scan loop |

## HTTP integration (`tests/headless_access_http.rs`, real listener)
| Test | Assertion |
|---|---|
| `unknown_token_401_no_echo` | 401; body lacks presented token |
| `malformed_bearer_header_variants_401` | `Bearer  x` (double space), `bearer x`, missing header → 401; `/health` without header → 200 (companion, #5571) |
| `mint_then_call_200_then_revoke_401_no_restart` | write via `TokenTable::mint`, call 200; revoke via spawned CLI binary; next call 401 (R-05.1) |
| `mint_and_request_same_mtime_tick` | mint immediately followed by request → 200 |
| `corrupt_tokens_json_scoped_401_legacy_200` | then fix → scoped 200 (R-05.6) |
| `deleted_tokens_json_scoped_401_no_panic` | server alive (`/health` 200) |
| `bodies_contain_no_secret_or_hash` | 401/403 bodies and server stderr grep for sentinel token and its sha256 → zero hits (R-09) |
