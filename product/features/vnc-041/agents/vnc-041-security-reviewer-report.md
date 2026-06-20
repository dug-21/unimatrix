# Security Review: vnc-041-security-reviewer

## Risk Level: low

## Summary
vnc-041 adds config seeding (global file (a) on container serve, per-slug file (b) on register) plus a content-free seam-level WARN for global-locked keys. Reviewed cold against the full `main...HEAD` diff and surrounding source. All five scrutinized surfaces (slug path-join, no-clobber seed write, C5 WARN parse, best-effort isolation, local-path unchanged) are safe. Two non-blocking findings: an unbounded raw read in the WARN pass (low — trusted operator file, once-per-boot, mirrors a pre-existing uncapped read) and an observability-completeness note on one-level key flattening.

## Findings

### F1 — C5 WARN raw read+parse is unbounded (no 64 KiB cap)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/http_provision.rs:341` (`std::fs::read_to_string` + `toml::from_str::<toml::Value>` in `warn_locked_keys`), before the cap at `infra/config.rs:3790`.
- **Description**: The WARN pass reads + parses the entire per-slug file with no size limit, BEFORE `load_single_config` applies the `CONFIG_MAX_BYTES` (64 KiB) check. Blast radius is bounded: file (b) is operator-authored (trusted tier, requires data-volume write access), read once per slug per boot, not per request. The existing `load_single_config` in the same arm already does an uncapped `std::fs::read` (the 64 KiB check is post-read), so the WARN adds a second full read+parse of an already-unprotected file rather than a categorically new surface. `toml` 0.8.23 returns `Err` (caught) on deep nesting — no parse-time panic.
- **Recommendation**: Optional hardening — gate the WARN read on `metadata().len() <= CONFIG_MAX_BYTES`, or cap the read. Defense-in-depth; the threat actor is already the data-volume owner.
- **Blocking**: no

### F2 — `flatten_present_keys` one-level flatten (observability completeness)
- **Severity**: info
- **Location**: `crates/unimatrix-server/src/http_provision.rs` (`flatten_present_keys`)
- **Description**: Only top-level leaves and one level of sub-table are enumerated. Correct-by-conservatism for the registry surface (`tls.<field>`/`http.<field>` are unknown → conservative WARN fires). A locked key buried >1 level deep would not warn, but is outside the per-slug seam and its value is still merge-ignored. No security impact.
- **Recommendation**: None required; note for future seam expansion.
- **Blocking**: no

## Surfaces verified clear
- **Path traversal via slug**: `ProjectSlug` newtype enforces `^[a-z0-9][a-z0-9-]{0,62}$` at `TryFrom` edge (seam.rs:83); rejects `.`/`/`/`\`/`%`/encodings. Seed reuses the single `per_slug_data_dir` join the resolver reads. No escape.
- **No-clobber/TOCTOU/symlink**: `create_new(true)` (`O_EXCL`) is the sole guard — no `exists()` precheck, refuses final-symlink follow. Operator files survive byte-for-byte. `force=true` overwrite reachable only from `handle_version --force`, never a seed.
- **Best-effort isolation**: both seeds return `()`, warn-and-continue, no `.unwrap()`/panic; cannot gate register hash-chain or boot.
- **Local STDIO unchanged**: global seed lexically inside `if config.http.enabled`; `else` branch has no seed call. AC-06 sentinel asserts zero files empirically.
- **WARN content-free**: logs `slug` + `key` only, never the operator's set value (#4749).
- **A→B drift**: render + WARN both bind to the runtime registry; `OverlayDisposition` match exhaustive (compile-break forcing function). No hand-list in B.
- **Dependencies**: none added; `toml` 0.8.23 pre-existing; `cargo audit` gate covers CVEs.

## Blast Radius Assessment
Worst case if the seed code has a subtle bug: a missing or duplicated *convenience* config file. `create_new` cannot overwrite, so no destructive write of operator config or arbitrary paths. Seed failures are swallowed, so they cannot corrupt the hash chain, the `[[projects]]` routing stanza (written by the unchanged `ensure_project_stanza`), or daemon boot. The WARN pass changes resolution output by exactly nothing (locked value already merge-ignored). No privilege escalation, no information disclosure (content-free logs), no DoS beyond a once-per-boot read of a trusted operator file.

## Regression Risk
Low. Additive call sites (`register` State B + C, the `if http.enabled` block) plus a delegation refactor of `write_default_config_if_absent` (force=false delegates to `write_if_absent`; force=true preserved verbatim). Existing config-write tests retained; resolution output byte/value-identical with/without the WARN.

## PR Comments
- Posted 1 review comment on PR #806 (state: COMMENTED, non-blocking).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — the F1 "validate at the boundary before the size cap" and "O_EXCL no-clobber seed" patterns are feature-specific applications of already-stored lessons (#665 TOCTOU class, #4749 content-free logging, #4876 empirical gate verification); no new cross-feature (2+) anti-pattern emerged.
