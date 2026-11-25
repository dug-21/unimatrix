# C1 Token Table — `http/token_table.rs`

Risks: R-05, R-12, R-10 (strict parse), edge cases. ACs: AC-07, AC-10 (hash at rest), AC-17.

## Unit tests
| Test | Assertion |
|---|---|
| `parse_valid_two_of_each_mode` | file with 2 `read` + 2 `read-write` → 4 entries, modes/slugs preserved (AC-07) |
| `parse_rejects_unknown_key` / `_duplicate_label` / `_empty_allowed_slugs` / `_bad_label_regex` (uppercase, leading `-`, 64 chars) | `Err`, whole file rejected; 63-char label accepted (companion) |
| `parse_rejects_bad_sha256` (not 64 lowercase hex) | `Err` |
| `parse_empty_tokens_array_ok` | empty table, no error |
| `open_missing_file_empty_table` | no panic, `lookup_ct` → None |
| `mint_returns_32_bytes_and_stores_sha256_only` | file contains `sha256(raw)` hex; raw hex absent from file bytes |
| `mint_existing_label_fails` / `mint_revoked_label_fails` | `Err`, file byte-identical (sha256 of file before/after) |
| `revoke_sets_revoked_at_keeps_record` | record keeps hash,label,mode,slugs,created_at; `revoked_at` set |
| `mint_after_revoke_preserves_tombstone` | tombstone still present after later mint of other label |
| `revoke_unknown_or_already_revoked_or_other_slug_errors` | `Err`, file byte-identical |
| `list_for_slug_filters_and_shows_revoked` | only slug's labels, revoked flag set, no hash field in `EntryView` |

## Reload / stamp (R-05)
| Test | Assertion |
|---|---|
| `snapshot_reloads_on_stamp_change` | external rewrite → next lookup sees it |
| `same_mtime_same_len_rewrite_detected_by_inode` | force identical mtime (`filetime` set) and equal length via tmp+rename → change detected |
| `corrupt_file_fails_closed_never_last_good` | load good, corrupt → lookup of previously valid token = None; one error log per stamp; fix file → recovers |
| `revoked_token_not_resurrected_by_corrupt_file` | revoke, corrupt, fix-to-pre-revoke-content impossible: assert revoked stays rejected through corrupt phase |

## Concurrency / perms (R-12)
| Test | Assertion |
|---|---|
| `twenty_parallel_mints_all_present` | 20 threads (and 20 spawned CLI processes in client-bundle-cli.md) → 20 entries, parses |
| `reader_never_sees_partial_file` | loop reader during 200 writes; every read parses or sees previous stamp |
| `file_and_tmp_mode_0600` | stat mode `0o600` on `tokens.json`; tmp created 0600 before write (`AFTER_TMP_CREATE` hook stats tmp → `0o600` before `write_all`) |
| `no_tmp_left_after_write`; `lock_file_created_if_missing` | dir listing contains only `tokens.json`, `tokens.lock` |
