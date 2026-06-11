# Agent Report — vnc-034 Wave 2, Agent 5: ProjectRegistry + lifecycle CLI

Issue #727, Stage 3b Wave 3 of 3. Component: `ProjectRegistry` + `register`/`list`/`delete`
as a pre-tokio sync subcommand (C-10), wired into `main.rs`.

## Files created / modified

- `crates/unimatrix-server/src/projects.rs` (new, 456 lines) — `ProjectCommand`,
  `ProjectStatus`, `ProjectRegistry`, `run_project_command`, and the single
  `per_slug_data_dir` path-join site.
- `crates/unimatrix-server/src/projects/tests.rs` (new) — full test plan coverage.
- `crates/unimatrix-server/src/lib.rs` — added `pub mod projects;`.
- `crates/unimatrix-server/src/main.rs` — added `Command::Project { command }` variant
  and its C-10 sync dispatch arm (alongside `Health`/`ClientBundle`/`Eval`).

## Tests

`cargo test -p unimatrix-server projects` → **50 passed, 0 failed**.
Full server lib suite (`cargo test -p unimatrix-server --lib`) → **4002 passed, 0 failed,
1 ignored** (no regressions). `cargo clippy -p unimatrix-server --tests` → no findings in
the new files. `cargo fmt` applied.

Key plan tests present and green: `test_register_rejects_reserved_tools_shadowing`,
`test_register_reserved_is_separate_from_charset`,
`test_deregister_reregister_reattaches_to_preserved_chain` (the integrity test — asserts
prior entries readable + chain head `content_hash` identical, not fresh genesis),
`test_purge_then_register_is_fresh_store` (contrast guard),
`test_purge_requires_slug_confirmation_or_no_destroy`, and the D6 two-state distinct-outcome
tests.

## OQ-CLI-7 outcome (re-attach vs genesis — load-bearing)

**`Store::open` (`SqlxStore::open`) IS non-destructive on an existing DB.** It runs idempotent
migrations + `create_tables_if_needed` only — it does NOT truncate or re-run a genesis that
clobbers existing rows, so the hash chain (the `entries` rows + their `previous_hash` links) is
preserved across an open. Verified by reading `crates/unimatrix-store/src/db.rs::open`.

**AND, as defence in depth, I added an explicit `data_exists` genesis gate** per the brief's
instruction: `register` branches on `data_exists = db_path(&dir).exists()`. The per-slug
*directory creation* (`create_dir_all` + vector dir) runs ONLY in State C (`!data_exists`); the
re-attach (State B) path opens the existing db without any create/genesis step. So even if
`open` semantics ever changed, the "fresh" provisioning branch can never run over preserved data.
The integrity test asserts the chain head survives a de-register→re-register round-trip.

## Locked-decision confirmation

- **D5 (reserved, incl. tools):** `validate_slug` does ProjectSlug charset (D1) FIRST, then a
  SEPARATE `is_reserved_slug` check (imported from `infra::config` — the single source, not a
  second list). `tools` is charset-valid yet rejected with a message naming the `/v1/tools/...`
  shadow. `test_register_reserved_is_separate_from_charset` is the discriminator;
  `test_register_reserved_exact_match_only` guards against over-broad prefix/substring matching.
- **D6 (two-state):** branches on BOTH `data_exists` and `is_routed` (read from `[[projects]]`
  config, NOT dir existence). State A (data + routing) → loud "already registered and routing"
  error; State B (data, de-registered) → re-attach (Ok, distinct message); State C (no data) →
  create. Not collapsed.
- **D4 (delete = de-register; --purge loud; re-attach):** default `delete` prints de-register +
  preserves the dir/db/chain (never calls `remove_dir_all`). `--purge` requires
  `--confirm <slug>` matching exactly — a bare `--purge` or a mismatch is refused with nothing
  destroyed. De-register → re-register re-attaches (State B). `--purge` is the ONLY `remove_dir_all`
  caller.
- **D3 (list status, no network):** `list` reports configured slugs with a local `store_open`
  status derived ONLY from db-file presence/readability — no HTTP/network surface, no
  `--list-slugs` flag.

## Deviation from pseudocode (with rationale — flagged, not silent)

The pseudocode's `scan_registered` enumerated dirs under the base and skipped names that don't
parse as a `ProjectSlug`. **That filter is insufficient:** path-hash data_dirs are siblings of
slug dirs under `.unimatrix` and their 16-hex names ARE charset-valid slugs (and contain a db),
so a directory scan emits all ~11k path-hash dirs as "projects". I confirmed this against the real
`~/.unimatrix` base. I made `list` **config-driven** instead (read the `[[projects]]` array — the
routing source of truth, consistent with D6 — and report each slug's local store status). This is
the correct D3/D6 semantics and is the only way to distinguish a slug dir from a path-hash dir.
Stored as Unimatrix pattern #4972.

## Knowledge Stewardship

- Queried: `context_search` (pattern: sync CLI subcommand → #4577/#2651 confirmed the C-10
  pre-tokio `block_export_sync` bridge pattern; decision/vnc-034 → ADR-004 #4951, ADR-005 #4949,
  ADR-001 #4954). Findings applied (reserved set, register-vs-attach, default-alias shadow).
- Stored: entry #4972 "Per-slug project `list` must be config-driven, not a directory scan
  (path-hash dirs are charset-valid slugs)" via `/uni-store-pattern`.

## Issues / blockers

None. Did not modify the Wave 2 resolver or Wave 1 config (only imported their public items:
`ProjectSlug`, `is_reserved_slug`). Did not run integration tests (Stage 3c). Did not commit —
leaving the wave commit to the leader.
