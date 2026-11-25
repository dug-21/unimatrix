# C6c Observe 403 Client Handling — `lib/hook-client` (config.js / transport-http.js)

Risks: R-17 (client), R-09. ACs: AC-18.

Design (human-approved, ADR-004 amended): latch is per credential, persisted as `observe-denied.json` in the client stateDir, keyed on SHA-256 of the credential (secret never stored), mode 0600, no expiry, fail-open.

All tests spawn the real hook process (`test/run-hook-client.js`) with clean HOME, via a capturing proxy in front of a real server (`test/helpers/real-server.js`) or a stub returning fixed statuses.

| Test | Assertion |
|---|---|
| `read_bundle_n_events_one_warning` (AC-18) | N=4 separately spawned hook processes, read `UNIMATRIX_BUNDLE`: exactly 1 warning line across all stderr; each exit 0; stdout identical to a no-remote run |
| `latch_stops_further_posts` (AC-18) | proxy sees exactly 1 observe POST (one 403) across the N spawns |
| `server_db_zero_observation_rows` | real server: observations unchanged |
| `latch_only_on_sessionwrite_403` (cond. 1) | stub 403 with other body (`{"error":"forbidden"}`), 401, 500, connection refused: each spawn POSTs as today, behavior unchanged, no marker file exists after N spawns |
| `marker_keyed_on_credential_hash` (cond. 2) | marker contains SHA-256 hex of credential; file bytes contain no token/bundle sentinel; `stat` mode `0o600`; path inside stateDir |
| `marker_no_expiry` | marker mtime backdated 1 year → next spawn still sends 0 POSTs, no warning |
| `credential_change_resets_latch` (AC-18) | after latch, spawn with different read credential → 1 POST, 1 warning; original credential still latched |
| `corrupt_marker_retries_once_and_rewrites` (AC-18, cond. 4) | marker = garbage / truncated JSON → next spawn POSTs once, gets 403, rewrites valid marker; following spawn 0 POSTs |
| `rw_credential_never_creates_marker` (AC-18) | rw bundle, N events → N POSTs, 0 warnings, no marker file |
| `missing_or_unreadable_marker_fails_open` (cond. 4) | marker deleted, or chmod 000 → treated as not refused: POST attempted, exit 0 |
| `marker_write_failure_no_behavior_change` (cond. 4) | stateDir read-only → exit code and stdout identical to writable run; warning still printed once for that process; no crash |
| `warning_sentinel_free` | stderr lacks token / header sentinels |
