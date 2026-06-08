# Test Plan — config-transport-selection (`lib/hook-client/config.js`)

Component 2 / ADR-002 §3, ADR-007 / FR-12..FR-16 / **AC-02** / Risks R-05 (Med), R-13 (Low).
`resolve(cwd)` gains `mode: "http"|"uds"` + (UDS) derived `socketPath`. Single derivation: `socketPath` from the
SAME `walkToProjectRoot` + `computeProjectHash` as `stateDir`. `node --test` on `config.test.js`.

## Unit expectations — mode matrix (FR-12, FR-13)

- `test_remote_env_pair_yields_http_mode` — env remote pair present → `mode:"http"`, behavior unchanged from F3, no `socketPath`.
- `test_settings_local_remote_yields_http_mode` — `unimatrix.remote` in `settings.local.json` → `mode:"http"`.
- `test_http_wins_even_if_local_socket_live` — remote config present → HTTP unconditionally; no probing for a live local socket, no local-override knob (FR-13, OQ1).
- `test_no_remote_yields_uds_mode_with_socketpath` — no remote config → `mode:"uds"` + `socketPath = ~/.unimatrix/{projectHash}/unimatrix.sock`.
- `test_missing_breadcrumb_path_removed` — the former terminal `{ok:false, reason:"missing"}` path no longer exists (it now resolves to UDS).
- `test_partial_env_stays_terminal` — `partial_env` remains a terminal misconfig breadcrumb (signals intent to use remote).
- `test_malformed_config_stays_terminal` — `malformed` remains terminal.

## Single-derivation invariant (ADR-007 §1, AC-02)

- `test_socketpath_dirname_equals_statedir_parent` — in UDS mode, `path.dirname(socketPath) === path.dirname(stateDir)` (both are `~/.unimatrix/{projectHash}/`) for every layout — the state dir and socket path can never disagree.
- `test_socketpath_uses_same_projecthash_as_statedir` — both derive from one `computeProjectHash(walkToProjectRoot(cwd))` call; no second hash implementation.

## Hash parity — AC-02 / R-05 (TS-vs-Rust, drift-checked corpus)

These cases consume the Rust-generated hash-fixture corpus (see parity-corpus-uds.md for corpus definition/drift):
- `test_hash_main_repo_root` — `/workspaces/unimatrix` → `0d62f3bf1bf46a0a` (matches live daemon).
- `test_hash_deep_subdir_resolves_to_root` — a deep subdir resolves to the same root and hash.
- `test_hash_linked_worktree_resolves_to_main_root` — git worktree cwd → main-repo root + identical hash (#679 resolution).
- `test_hash_symlinked_repo_path` — realpath/canonicalize parity → same hash.
- `test_hash_non_git_fallback` — non-git dir → resolved-cwd fallback hash matches Rust.
- `test_corrupt_worktree_divergence_is_exactly_documented` — dangling `gitdir:` target: TS falls back to realpath-of-containing-dir, Rust to raw cwd. Assert the divergence is EXACTLY this and nothing else (the one enumerated accepted asymmetry, ADR-007 §4).

## No-daemon UX bounds (R-13, FR-16)

- `test_no_remote_no_daemon_resolves_uds_not_terminal` — config resolution succeeds in UDS mode even with no daemon present (enqueue path, not a terminal breadcrumb). Queue-bound assertions live with the enqueue path (transport-uds.md / parity-corpus-uds.md): 500 files / 5 MiB / 24 h.

## Edge cases
- Malicious `cwd` steering the project-root walk: hash is sha256-derived hex[..16], cannot contain `/` or `..` — confined to `~/.unimatrix/{hash}/` (security surface, asserted via the non-git fallback fixture).
- Home dir unresolved → fail-open (no throw); document expected behavior.
- Windows: UDS unavailable, remote HTTP is the path (NFR-5) — not shimmed; assert config still resolves `http` mode on remote config regardless of platform.
