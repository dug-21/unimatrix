# C5 Legacy-Token Config — `infra/legacy_token_config.rs`, `infra/config.rs`

Risks: R-15, R-21. ACs: AC-01, AC-19, AC-23.

## Unit
| Test | Assertion |
|---|---|
| `override_accepts_true_1_false_0` | `Some(true/false)` |
| `override_empty_or_unset_is_none` | `Ok(None)` |
| `override_rejects_yes_TRUE_space_2_off` | `Err` naming `UNIMATRIX_LEGACY_TOKEN_ENABLED`, no trimming |
| `config_missing_key_defaults_true` | existing config fixture files parse unchanged |
| `config_env_matrix` | config {absent,true,false} × env {unset, "", true, false, 1, 0} → env wins when set, else config, else true |

## Boot / HTTP integration
| Test | Assertion |
|---|---|
| `bad_env_value_fails_boot` | spawn binary with `=yes` → non-zero exit, stderr names var |
| `disabled_legacy_401_no_echo` | env `false` and separately config `false`: legacy → 401 `legacy token disabled`, body lacks token |
| `disabled_legacy_scoped_unaffected` | scoped rw token 200 (companion) |
| `legacy_read_write_two_slugs` | default config: read + write on A and B via HTTP → 200 (AC-01) |
| `legacy_unaffected_by_corrupt_tokens_json` | 200 |
| `docs_state_legacy_baseline` | grep remote-access guide for legacy-token statement (AC-01) |
