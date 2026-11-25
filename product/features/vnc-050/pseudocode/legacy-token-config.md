# C5 Legacy-Token Config — `infra/legacy_token_config.rs` (new), `infra/config.rs` (~5 lines)

ADR-001 (#5814), FR-17, AC-19, HD 3c. Target < 150 lines incl. tests.

## Purpose
`http.legacy_token_enabled` (serde default true) with env override `UNIMATRIX_LEGACY_TOKEN_ENABLED` applied after the file merge (step 3c pattern). Unrecognized env value is a boot error.

## `infra/legacy_token_config.rs`
```
pub(crate) const LEGACY_TOKEN_ENABLED_VAR: &str = "UNIMATRIX_LEGACY_TOKEN_ENABLED";

/// Pure; testable without set_var (forbid(unsafe_code)).
pub(crate) fn resolve_legacy_token_override(v: Option<&str>) -> Result<Option<bool>, ConfigError> {
    match v {
        None | Some("") => Ok(None),                  // empty = no override (FR-40), NO trimming
        Some("true") | Some("1")  => Ok(Some(true)),
        Some("false") | Some("0") => Ok(Some(false)),
        Some(_) => Err(ConfigError::InvalidLegacyTokenOverride { var: LEGACY_TOKEN_ENABLED_VAR }),
    }
}
```
Exact, case-sensitive match. Deliberate deviation from `resolve_http_enabled_override` (which trims/lowercases and ignores garbage): RTS R-15.1 requires `TRUE ` (trailing space) to fail boot; `yes`, `2`, `off` fail. Gap flagged: whether bare `TRUE` should be accepted is unspecified; pseudocode rejects it (fail-closed, documented in error message as "expected true|1|false|0").

## `infra/config.rs` touches
1. `HttpConfig` field (struct already has `#[serde(default)]`, so absent key = `Default`):
   ```
   /// Accept the legacy all-slugs `{data_dir}/token`. Default true (vnc-050 ADR-001).
   pub legacy_token_enabled: bool,
   ```
2. `Default for HttpConfig`: `legacy_token_enabled: true,`
3. `ConfigError` new variant `InvalidLegacyTokenOverride { var: &'static str }`; Display: `"{var} must be one of true|1|false|0 (empty = no override)"` — value not echoed. (Gap: architecture names `ConfigError` but not the variant.)
4. After step 3c in `load_config` (line ~3296):
   ```
   // Step 3d: UNIMATRIX_LEGACY_TOKEN_ENABLED override (vnc-050 ADR-001). Bad value = boot error.
   if let Some(v) = legacy_token_config::resolve_legacy_token_override(std::env::var(legacy_token_config::LEGACY_TOKEN_ENABLED_VAR).ok().as_deref())? {
       merged.http.legacy_token_enabled = v;
   }
   ```
`merge_configs` merges `http` as a whole struct (`project.http != default.http`) — no change needed; the new field rides along.
`std::env::var(..).ok()` maps non-UTF-8 to None (no override) — acceptable; documented.

Consumer: C2 boot wiring reads `config.http.legacy_token_enabled`.

## Error Handling
`Err(ConfigError)` propagates from `load_config` → existing boot failure path (non-zero exit, message names the variable).

## Key Test Scenarios
- Parser: `None`, `""` → None; `true|1|false|0` → value; `yes`, `TRUE `, `TRUE`, `2`, `off`, `" true"` → Err naming var.
- Matrix config {absent, true, false} × env {unset, true, false}: env wins; absent → true (AC-19).
- Existing config files with no key parse unchanged (PL-2).
- Boot with `=yes` → non-zero exit, error names `UNIMATRIX_LEGACY_TOKEN_ENABLED`.
