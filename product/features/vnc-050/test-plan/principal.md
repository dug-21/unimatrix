# C4 Principal Mapping — `http/principal.rs`

Risks: R-02 (mode table), R-13, R-22. ACs: AC-08, AC-27.

## Unit tests
| Test | Assertion |
|---|---|
| `read_mode_capabilities` | `TokenMode::Read.capabilities() == [Read, Search]` |
| `read_write_mode_capabilities` | `[Read, Write, Search, SessionWrite]`; never `Admin` |
| `auth_identity_scoped_is_token_label` | `agent_id == "token:ci"`, credential_type `scoped_token` |
| `auth_identity_legacy` | `http-bearer`, `static_token` |
| `observe_and_tool_ceiling_agree_per_mode` | for each mode, `ResolvedIdentity` caps == `ceiling_from` caps (single table) |

## Reserved namespace (R-13) — real transports
| Test | Assertion |
|---|---|
| `token_prefix_agent_id_invalid_input_stdio` | stdio `context_search {agent_id:"token:ci"}` → `InvalidInput`; companion `agent_id:"ci"` → OK |
| `..._uds` | same over UDS |
| `..._http_legacy`, `..._http_scoped` | same over HTTP with each token kind |
| `no_token_rows_in_agents_table_after_scoped_session` | after rw session with several calls: `SELECT count(*) FROM agents WHERE agent_id LIKE 'token:%'` == 0 |

## R-22 pin
| Test | Assertion |
|---|---|
| `rw_token_omitted_agent_id_writes_as_anonymous` | store OK; entry attributed `anonymous`; `http_access` `token:<label>`; registry grant rows unchanged (AC-27) |
| `read_token_omitted_agent_id_write_denied` | `CapabilityDenied` despite anonymous holding Write |
| `stdio_supplied_agent_id_enrolled_as_today` | companion |
