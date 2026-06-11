# Component: `[[projects]]` config + slug validation

> Source file: `crates/unimatrix-server/src/infra/config.rs`
> (the brief/architecture call it `config.rs`; the actual path is `infra/config.rs`).
> Requirements: FR-C2 (slugs operator-declared in `[[projects]]`), FR-C5 / AC-W2-R6
> (slug allowlist at the routing edge; D1 grammar), FR-C6 / AC-W2-R2 (`[[projects]]`
> absent ⇒ unchanged). LOCKED: D1 grammar `^[a-z0-9][a-z0-9-]{0,62}$`; D2 NO overlay;
> D5 reserved-slug refusal (`v1`/`health`/`observe`/`tools`) — a SEPARATE check from D1.

## Purpose

Add a `[[projects]]` array-of-tables section to `UnimatrixConfig` that declares the
operator's slugs. Parsing this section is **structural only** (a list of slug strings);
the **authoritative slug validation is `ProjectSlug::TryFrom`** (already merged in
`http/router/seam.rs`, D1 grammar) — config does NOT re-implement the regex. The
config layer's job is: (1) deserialize the stanzas, (2) convert each raw slug string to
a validated `ProjectSlug` via the EXISTING `TryFrom`, surfacing a clean `ConfigError`
on rejection, (3) reject duplicate slugs. The validated, deduped list is what
`ProjectRouter` (project-router.md) and the listener wiring consume.

## Reserved route segments (D5 — single source of truth for the whole feature)

```rust
/// Route segments that a slug must NEVER equal. A slug equal to any of these would
/// shadow a fixed route. `tools` is the CRITICAL case: `/v1/tools/...` is the
/// default-project alias (ADR-005), so a slug named `tools` would shadow the default
/// project entirely. This is a SEPARATE check from the D1 charset allowlist — every
/// one of these IS charset-valid (`tools`, `health`, `observe`, `v1` all match
/// ^[a-z0-9][a-z0-9-]{0,62}$) and MUST still be rejected. Defined ONCE here; the
/// register CLI (project-registry-cli.md) imports this same constant — no second list.
pub const RESERVED_SLUGS: [&str; 4] = ["v1", "health", "observe", "tools"];

fn is_reserved_slug(slug: &ProjectSlug) -> bool:
    RESERVED_SLUGS.contains(&slug.as_str())
```

## New types

```rust
/// One `[[projects]]` stanza. Raw operator input — `slug` is an UNVALIDATED String
/// at deserialize time; validation happens in `validate_projects_config` via the
/// existing ProjectSlug allowlist (D1). Adding fields here is the future overlay seam,
/// but D2 forbids overlay in Wave 2 — keep this to `slug` ONLY.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ProjectConfigEntry {
    pub slug: String,
}

// On UnimatrixConfig (config.rs:70), add the section. Absent ⇒ empty Vec (serde default),
// which is the backward-compat path (FR-C6 / AC-W2-R2):
//
//   #[serde(default)]
//   pub projects: Vec<ProjectConfigEntry>,
//
// TOML shape:
//   [[projects]]
//   slug = "alpha"
//   [[projects]]
//   slug = "beta"
```

`UnimatrixConfig` already derives `Default`/`Deserialize` with `#[serde(default)]` per
field (config.rs:69–98); `projects: Vec<ProjectConfigEntry>` with `#[serde(default)]`
defaults to empty when the section is absent — no `Default` impl change needed.

## New / modified functions

### `validate_projects_config` (NEW — the config-side gate)

```
fn validate_projects_config(
    entries: &[ProjectConfigEntry],
    path: &Path,                  // config file path, for error context (mirrors validate_http_config)
) -> Result<Vec<ProjectSlug>, ConfigError>:

    result = Vec::new()
    seen   = HashSet::<String>::new()           // dedupe on the validated string form

    for entry in entries:
        # 1. AUTHORITATIVE validation = the merged Wave-1 allowlist (D1). Do NOT
        #    re-implement the regex here. ProjectSlug::try_from enforces
        #    ^[a-z0-9][a-z0-9-]{0,62}$ at the parse edge (seam.rs:71-104).
        slug = match ProjectSlug::try_from(entry.slug.as_str()):
            Ok(s)  => s
            Err(_) => return Err(ConfigError::ProjectSlugInvalid {
                          path: path.to_path_buf(),
                          value: entry.slug.clone(),     # rejected input, diagnostics only
                      })

        # 2. RESERVED-SLUG refusal (D5) — SEPARATE from the D1 charset check above. A
        #    charset-valid slug equal to a reserved route segment (v1/health/observe/
        #    tools) MUST still be rejected. `tools` is the critical case: it shadows the
        #    /v1/tools/... default-project alias (ADR-005). Uses the SHARED RESERVED_SLUGS
        #    constant (defined above) — the CLI register path imports the same list.
        if is_reserved_slug(&slug):
            return Err(ConfigError::ProjectSlugReserved {
                path:  path.to_path_buf(),
                value: slug.as_str().to_owned(),
            })

        # 3. Reject duplicate slugs — two [[projects]] with the same slug is an
        #    operator error (ambiguous registry → ambiguous store). Fail loud.
        if !seen.insert(slug.as_str().to_owned()):
            return Err(ConfigError::ProjectSlugDuplicate {
                path: path.to_path_buf(),
                value: slug.as_str().to_owned(),
            })

        result.push(slug)

    Ok(result)
```

Notes:
- Returns `Vec<ProjectSlug>` (validated), never raw strings — downstream never re-validates.
- No filesystem access here. This is parse-edge validation BEFORE any path use (R-03):
  a rejected slug never reaches `per_slug_data_dir`. The escape-is-unrepresentable
  guarantee (AC-W2-R6) is the allowlist's, exercised here at config-load time.
- `ProjectSlug` is imported from the seam: `use crate::http::{ProjectSlug, RouteError};`
  (re-exported at `http/router.rs:319` → `http/mod.rs`). If `infra/config.rs` cannot
  depend on `http` without a cycle, see "Open questions" — the fallback is to keep
  `validate_projects_config` in a small `projects.rs`-adjacent module that both `http`
  and the CLI import, NOT to duplicate the regex.

### `ConfigError` additions (modify the existing enum, config.rs:2283)

```
enum ConfigError {
    ... existing variants (HttpFieldInvalid, etc.) ...

    /// A [[projects]] slug failed the allowlist (D1 / FR-C5). Carries the rejected
    /// input for the operator's diagnostics only — never used to build a path.
    ProjectSlugInvalid { path: PathBuf, value: String },

    /// A [[projects]] slug equals a reserved route segment (D5). Charset-valid but
    /// forbidden because it would shadow a fixed route (`tools` shadows the
    /// default-project alias). SEPARATE from ProjectSlugInvalid.
    ProjectSlugReserved { path: PathBuf, value: String },

    /// Two [[projects]] stanzas declared the same slug.
    ProjectSlugDuplicate { path: PathBuf, value: String },
}
```
Add matching `Display` arms (mirror the existing `HttpFieldInvalid` formatting style):
- `ProjectSlugInvalid` → `"invalid project slug '{value}' in {path}: must match ^[a-z0-9][a-z0-9-]{{0,62}}$ (lowercase alphanumeric and hyphen, 1-63 chars, no underscore)"`
- `ProjectSlugReserved` → `"project slug '{value}' in {path} is reserved (v1, health, observe, tools); 'tools' would shadow the default-project alias /v1/tools/..."`
- `ProjectSlugDuplicate` → `"duplicate project slug '{value}' in {path}"`

The `Display` text MUST name the exact D1 grammar so the gate/PR-review can assert the
canonical regex (the brief: "the gate and PR review MUST assert this exact value").

### `load_config` wiring (modify, config.rs:2806)

`load_config` already deserializes `UnimatrixConfig` from TOML and runs section
validators (e.g. `validate_http_config`). Add ONE call after the config is parsed and
before `Ok(ConfigLoadResult { .. })`:

```
# inside load_config, after the existing validate_http_config(...) call:
let _validated_slugs = validate_projects_config(&config.projects, &config_path)?;
```

`load_config` keeps returning `ConfigLoadResult { config, .. }` — `config.projects`
carries the raw `Vec<ProjectConfigEntry>`. The *validated* `Vec<ProjectSlug>` is
re-derived by the listener wiring (and the CLI) by calling `validate_projects_config`
again, OR `ConfigLoadResult` gains an optional `pub projects: Vec<ProjectSlug>` field
carrying the validated list (preferred — validate once). See "Open questions".

Rationale for validating inside `load_config`: a malformed slug in `config.toml` must
fail server startup **loud and actionable** (NFR-03), at config-load, NOT at first
request. This is the FR-C6 backward-compat guarantee's mirror: absent section ⇒ empty
⇒ zero change; present-but-bad ⇒ clean startup error.

## Data flow

```
config.toml [[projects]] stanzas
   │  serde::Deserialize
   ▼
UnimatrixConfig.projects: Vec<ProjectConfigEntry>   (raw strings)
   │  validate_projects_config (load_config)
   ▼
Vec<ProjectSlug>  (validated, deduped)  ──► listener wiring builds ProjectRouter
                                         └─► register/list/delete CLI cross-checks
```

## Error handling

- Invalid slug → `ConfigError::ProjectSlugInvalid` → propagates from `load_config` →
  server startup fails loud (no panic, no `.unwrap()`; matches existing `ConfigError`
  propagation). NFR-03.
- Reserved slug (D5) → `ConfigError::ProjectSlugReserved` → same loud-fail path. Distinct
  variant so the message names the shadowing risk (not a generic "invalid").
- Duplicate slug → `ConfigError::ProjectSlugDuplicate` → same loud-fail path.
- Absent `[[projects]]` → empty Vec → `Ok` → backward-compat path (AC-W2-R2). NOT an error.

## Key test scenarios (hints for the tester — not the test plan)

1. `[[projects]]` absent ⇒ `config.projects` empty, `load_config` Ok, behavior
   unchanged (AC-W2-R2 / FR-C6).
2. Valid stanzas `slug="alpha"`, `slug="beta"` ⇒ `validate_projects_config` returns
   `[alpha, beta]` as `ProjectSlug`.
3. D1 grammar rejects (each ⇒ `ProjectSlugInvalid`, at config load, no path touched):
   `"Alpha"` (uppercase), `"a_b"` (underscore — the drifted-issue charset MUST reject),
   a 64-char slug (over the 63 bound — the drifted 64 bound MUST reject), `"-lead"`
   (leading hyphen), `""` (empty), `"../etc"`, `"a/b"`, `"a%2fb"`, `"a.b"`.
4. **D5 reserved-slug refusal:** `slug="tools"` ⇒ `ProjectSlugReserved` (NOT
   `ProjectSlugInvalid` — `tools` is charset-valid; assert the distinct variant and that
   the message names the `/v1/tools/...` shadow). Same for `slug="v1"`, `slug="health"`,
   `slug="observe"`. Assert each is charset-valid (`ProjectSlug::try_from` Ok) yet
   `validate_projects_config` rejects it — proving the two checks are independent.
5. Duplicate `slug="alpha"` twice ⇒ `ProjectSlugDuplicate`.
6. The `ProjectSlugInvalid` Display string contains the literal
   `^[a-z0-9][a-z0-9-]{0,62}$` (gate asserts the canonical D1 value, not the drift).
7. Round-trip: a `ProjectSlug` produced by config equals one produced by
   `parse_project_key("/v1/alpha/tools")` (single grammar, no second validator).

## Out of scope (D2)

NO per-project config-overlay: no per-`[[projects]]` fields beyond `slug`, no merge/
precedence semantics. If an overlay need surfaces, file a follow-up issue (per D2).

## Open questions / gaps

- **OQ-CFG-1 (module dependency direction):** `infra/config.rs` importing `ProjectSlug`
  from `crate::http` may create an unwanted `config → http` dependency (today `http`
  depends on `config`, e.g. `HttpConfig`). Confirm the import direction at build time.
  If it cycles: extract `ProjectSlug`/its `TryFrom` into a tiny leaf module both `http`
  and `config` import (the grammar still lives in exactly ONE place — D1 preserved).
  **Do NOT** resolve a cycle by duplicating the regex into config.
- **OQ-CFG-2 (carry validated slugs):** prefer adding `pub projects: Vec<ProjectSlug>`
  to `ConfigLoadResult` so validation runs once at load and both the listener and the
  CLI consume the validated list. Confirm with the synthesizer; the fallback
  (re-call `validate_projects_config`) is correct but validates twice.
