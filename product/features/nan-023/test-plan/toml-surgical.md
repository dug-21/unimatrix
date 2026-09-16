# Test Plan — Surgical TOML Helper (`upsertTomlTable` / `readTomlTable`)

> Component: `lib/codex-install.js` (internal, ADR-002) · Runner: `node --test`
> Primary risk: **R-04** (foreign-key/comment/order preservation). Secondary: R-10 (injection), R-11 (malformed skip).
> ACs: AC-06 (owned table present + foreign preserved), AC-04 (idempotence).

The surgical writer is text-only: it manages **exactly** `[mcp_servers.unimatrix]` and copies every
other byte verbatim. These are pure-function unit tests over `raw:string` — no filesystem. Foreign-byte
preservation is a **first-class assertion** (NFR-08), not best-effort.

## Test file

`test/codex-toml-surgical.test.js` — imports `upsertTomlTable`, `readTomlTable` from `../lib/codex-install.js`.

## `readTomlTable`

| Test | Arrange | Assert |
|------|---------|--------|
| `test_readTomlTable_present_returns_body` | raw with `[mcp_servers.unimatrix]\ncommand="x"` | `{present:true, body}` where body is the region header→next-top-level-`[`/EOF |
| `test_readTomlTable_absent_returns_not_present` | raw with only foreign tables | `{present:false, body:""}` |
| `test_readTomlTable_stops_at_next_top_level_table` | owned table followed by `[other]` | body excludes `[other]` and everything after |
| `test_readTomlTable_distinguishes_parent_table` | a stray `[mcp_servers]` parent + `[mcp_servers.unimatrix]` child | reads the child region only, not the parent (ADR-002 nested/dotted edge) |

## `upsertTomlTable` — insert (absent)

- `test_upsertTomlTable_absent_appends_with_single_blank_separator` — non-empty file not ending in newline → appends owned table preceded by **exactly one** blank-line separator; `changed:true`.
- `test_upsertTomlTable_empty_file_appends_no_leading_blank` — empty raw → owned table only, no leading blank.
- `test_upsertTomlTable_file_ending_in_newline_no_double_blank` — off-by-one guard: file already ends in `\n` → not two blank lines.

## `upsertTomlTable` — update (present)

- `test_upsertTomlTable_present_replaces_only_region_body` — stale `command` value → only the owned region's body bytes change; every byte before the header and after the region is identical (byte-diff of foreign regions = empty).
- `test_upsertTomlTable_idempotent_when_body_equal` — desired body == current body → `changed:false`, text byte-identical (AC-04 / NFR-01).

## Foreign preservation (R-04 — first-class)

- `test_upsertTomlTable_preserves_foreign_tables` — fixture with foreign `[mcp_servers.other]`, a top-level `[tools]`, interleaved comments, and **non-alphabetical** key order; after upsert, byte-diff every foreign region = empty; owned table present (AC-06).
- `test_upsertTomlTable_preserves_comment_adjacent_above_owned` — a `# comment` immediately **above** `[mcp_servers.unimatrix]` survives (common off-by-one, RISK-STRATEGY R-04 scenario 3).
- `test_upsertTomlTable_preserves_comment_adjacent_below_owned` — comment immediately **below** the owned table, before the next `[`, survives on update.
- `test_upsertTomlTable_preserves_crlf_line_endings` — foreign CRLF config → foreign bytes (incl. `\r\n`) unchanged (edge inventory).

## Malformed / fail-safe (R-11)

- `test_upsertTomlTable_ambiguous_boundary_signals_malformed` — a table header inside a multi-line string or no clear table boundary → the writer signals malformed (caller emits `skipped-malformed`, file preserved); **never** a partial write (AC-15). Verified at the writer's contract boundary; the WireLeg-level surfacing is asserted in `codex-install.md`.

## Injection (R-10) — string escaping

- `test_upsertTomlTable_escapes_metacharacter_path_value` — body built from a path containing a space and a `"` → emitted `command`/`args` value is TOML-escaped and **re-parses** (via a TOML read-back) to the intended argv; not naive concatenation. (Full end-to-end injection fixture is in `codex-install.md` / `c14-verifier.md`.)

## Coverage note

Every assertion is a byte-level or parse-back check — no test asserts merely "table looks right". Foreign-region byte-diff = empty is the load-bearing assertion for R-04/NFR-08.
