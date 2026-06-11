# Test Plan — `[[projects]]` config + slug validation (D1 regex)

> Component: `crates/unimatrix-server/src/infra/config.rs`
> Source: FR-C2, FR-C5, FR-C6; AC-W2-R2, AC-W2-R6; R-03 (fix-before-merge), R-13.
> Locked: **D1** allowlist `^[a-z0-9][a-z0-9-]{0,62}$`; **D2** no config-overlay.
>
> This component owns the **SR-09 trust boundary** — the slug allowlist — and the
> `[[projects]]` parse + backward-compat (absent ⇒ Default). The allowlist newtype
> `ProjectSlug::TryFrom` already lives in `http/router/seam.rs` (merged); the
> exhaustive security table below targets THAT newtype, and config-level validation
> must call it (no second, drifting validator).

---

## Unit test expectations

### A. `[[projects]]` config parse (FR-C2)

- `test_projects_config_parses_slug_list` — a `[[projects]]` array with two entries
  (`slug = "alpha"`, `slug = "beta"`) deserializes into the project config vec with
  both slugs, in order. Arrange a TOML string; Act `load_single_config`/parse; Assert
  two entries, correct slug strings.
- `test_projects_config_entry_fields` — each entry carries at minimum a validated
  `slug`; assert the per-slug data dir is derived as `/data/.unimatrix/{slug}/`
  (path-join from the validated slug only — never raw input).
- `test_projects_config_duplicate_slug_rejected` — two identical slugs in
  `[[projects]]` → loud `ConfigError` at load (a duplicate slug would alias two
  registry entries to one dir). Assert error, not last-wins silence.

### B. Backward-compat: `[[projects]]` absent ⇒ Default (FR-C6, AC-W2-R2, R-13)

- `test_projects_absent_yields_empty_registry` — config with NO `[[projects]]`
  section parses to an empty project list (default), NOT an error. Assert
  `projects.is_empty()`.
- `test_projects_absent_default_alias_unchanged` — with an empty project list, the
  resolution path for `/v1/tools/…` is `ProjectKey::Default` (cross-check with
  `parse_project_key` in seam.rs). Assert no slug arm is constructed; behavior is
  byte-identical to current single-project config. (End-to-end backward-compat is
  also covered by infra-001 smoke — OVERVIEW §4.1.)

### C. Slug validation at config load (FR-C5, R-03)

- `test_config_slug_validation_uses_projectslug_newtype` — a `[[projects]]` entry
  with an invalid slug (e.g. `"My_Project"`) FAILS at load with a `ConfigError`
  that names the offending slug — the SAME rejection the router gives. Assert the
  config path delegates to `ProjectSlug::TryFrom`, not a hand-rolled check.
- `test_config_invalid_slug_fails_loud_not_panic` — invalid slug in config → loud
  actionable error, non-zero, no panic, no `.unwrap()` (NFR-03 discipline). Grep:
  no `.unwrap()` in the projects-config parse/validate path.

### D. D2 — NO config-overlay surface introduced (locked out of scope)

- `test_no_per_project_config_overlay_merge` — **negative.** Assert Wave 2 adds NO
  per-project config-overlay merge: a `[[projects]]` entry has no `config` /
  `overlay` / `inherit` sub-table that participates in `merge_configs`. Structural:
  the project-config struct exposes only identity fields (slug + derived dir), not a
  nested `UnimatrixConfig`. Grep the config module for new overlay/precedence
  surface introduced under `[[projects]]`; assert none. (Per D2: config precedence
  is dsn-001 territory, split to a follow-up.)

---

## SECURITY TEST TABLE — slug allowlist (AC-W2-R6 / SR-09 / R-03, fix-before-merge)

**The allowlist is EXACTLY `^[a-z0-9][a-z0-9-]{0,62}$` (D1).** Every row asserts
`ProjectSlug::try_from(input)` returns `Err(RouteError::InvalidSlug(_))` (REJECT) or
`Ok(_)` (ACCEPT). The table is exhaustive over the traversal/encoding corpus and
includes the **two discriminator rows** that catch the drifted issue-body regex
`^[a-z0-9][a-z0-9_-]{0,63}$`.

Test module: extend `http/router/seam.rs`'s slug-parse tests (cumulative). Each row
is one `#[test]` named `test_slug_reject_<case>` / `test_slug_accept_<case>`.

| T-ID | Input | Expected | Why / which threat |
|------|-------|----------|--------------------|
| T-SEC-01 | `../etc` | REJECT | path traversal — `.`/`/` not in charset |
| T-SEC-02 | `..` | REJECT | parent-dir token |
| T-SEC-03 | `a/../b` | REJECT | embedded traversal |
| T-SEC-04 | `%2e%2e%2f` | REJECT | percent-encoded `../` (`%` not in charset) |
| T-SEC-05 | `a%2fb` | REJECT | encoded `/` mid-slug |
| T-SEC-06 | `%2e` | REJECT | encoded `.` |
| T-SEC-07 | `/abs/path` | REJECT | absolute path (leading `/`) |
| T-SEC-08 | `a/b` | REJECT | bare separator |
| T-SEC-09 | `a\\b` | REJECT | backslash separator (Windows) |
| T-SEC-10 | `Alpha` | REJECT | uppercase (charset is lowercase-only) |
| T-SEC-11 | `` (empty) | REJECT | empty — fails the 1-char minimum |
| T-SEC-12 | `-alpha` | REJECT | leading hyphen (must start alnum) |
| T-SEC-13 | `al pha` | REJECT | whitespace |
| T-SEC-14 | `alpha.beta` | REJECT | dot separator |
| **T-SEC-15** | `my_project` (underscore) | **REJECT** | **DISCRIMINATOR** — underscore is NOT in the locked charset; a drifted `[a-z0-9_-]` impl ACCEPTS this and turns the test red |
| **T-SEC-16** | 64-char slug (`"a"*64`) | **REJECT** | **DISCRIMINATOR** — over the 63 (DNS-label) bound; a drifted `{0,63}` impl (max 64) ACCEPTS this |
| T-SEC-17 | 63-char slug (`"a"*63`) | ACCEPT | exact upper bound is valid |
| T-SEC-18 | `a` (single char) | ACCEPT | 1-char minimum is valid |
| T-SEC-19 | `alpha-1` / `a1-b2` | ACCEPT | canonical: lowercase alnum + interior hyphen |
| T-SEC-20 | `1alpha` | ACCEPT | leading digit is allowed (charset start = alnum) |

### No-filesystem-escape assertion (AC-W2-R6 closing clause)

- `test_no_accepted_slug_escapes_data_dir` — for every ACCEPT-ed slug in the table
  (plus a small property/fuzz sweep of random charset-valid slugs), assert the
  derived path `/data/.unimatrix/{slug}` **canonicalizes within** the
  `/data/.unimatrix/` root — escape is unrepresentable, not merely rejected. Since
  `.`, `/`, `\`, `%` cannot pass the charset, a valid slug has no path component.
  Assert `joined.starts_with(root)` and contains no `..`/separator component.
- `test_rejected_slug_never_reaches_path_join` — assert validation occurs at the
  parse edge BEFORE any `Path::join`: a rejected slug returns `Err` from
  `try_from` and the registry/config code never constructs a path from raw input.
  (Structural: the only path-builder takes a `ProjectSlug`, not `&str`.)

### Charset/parity cross-check with register CLI

- `test_config_and_register_reject_identical_corpus` — drive the full reject corpus
  (T-SEC-01..16) through BOTH the config-load path and `register <slug>`; assert
  identical rejection. Guards against a second validator drifting from the seam
  newtype (cross-component dependency, OVERVIEW §3).

---

## RESERVED-SLUG TEST TABLE — D5 (route-grammar refusal; SEPARATE from charset)

**D5 (locked):** `register` MUST reject any slug equal to a reserved route segment:
**`v1`, `health`, `observe`, `tools`.** This is a check the **register CLI** owns
(see `project-registry-cli.md` §A.2), but the reserved SET is route-grammar ground
truth, mirrored here so config-load that materializes registry entries refuses the
same set. The critical property: the reserved check is **separate from and additional
to** the D1 charset allowlist — a charset-VALID slug can still be reserved.

| T-ID | Input | Charset (D1) | Reserved (D5) | Net result | Why |
|------|-------|--------------|---------------|-----------|-----|
| **T-RSV-01** | `tools` | **valid** | **reserved** | **REJECT** | **THE shadowing test** — `/v1/tools/…` is the default-project alias (ADR-005); a slug `tools` would shadow the default project entirely. A charset-only impl wrongly ACCEPTS this. |
| T-RSV-02 | `v1` | valid | reserved | REJECT | reserved route segment (`/v1/…` prefix) |
| T-RSV-03 | `health` | valid | reserved | REJECT | reserved route segment (`GET /health`) |
| T-RSV-04 | `observe` | valid | reserved | REJECT | reserved route segment (`/observe/…`) |
| T-RSV-05 | `toolsx` / `v1x` / `healthy` | valid | NOT reserved | ACCEPT | discriminator: only EXACT matches are reserved — no over-broad prefix/substring rejection |

- `test_reserved_check_is_separate_from_charset` — **the discriminator.** Assert
  `tools` PASSES `ProjectSlug::try_from` (charset-valid: lowercase alnum, 5 chars)
  yet is REJECTED by the register reserved-slug guard. A charset-only implementation
  (no reserved set) would wrongly accept `tools` — this test turns that impl red.
  Proves the two checks are independent layers, not one combined regex.
- `test_reserved_set_exact_match_only` — `toolsx`, `v1-prod`, `healthcheck`,
  `observer` are NOT reserved (only the four exact segments are); assert they pass
  the reserved guard (subject to normal charset + registration). Guards against an
  over-broad `starts_with`/`contains` reserved check.

---

## Edge cases (from RISK-TEST-STRATEGY §Edge Cases)

- Reserved-word slugs reaching the slug position: `tools` (never reaches the slug
  arm — `/v1/tools/…` is matched first), `health`, `observe`, `v1`. Assert `health`/
  `observe`/`v1` PASS the charset but, unregistered, resolve to `UnknownProject`;
  refusing to REGISTER reserved slugs is the **D5** reserved-slug guard (separate
  from charset) — see the RESERVED-SLUG TEST TABLE above and
  `project-registry-cli.md` §A.2. `tools` is the critical shadowing case (T-RSV-01).
- Max-length (63) and single-char already in the table (T-SEC-17/18).
- Unicode lookalikes (e.g. fullwidth `ａ`) → REJECT (not ASCII lowercase).

## Notes
- No `.unwrap()` in non-test config/slug code.
- The discriminator rows (T-SEC-15, T-SEC-16) are the gate's proof that D1 (not the
  drifted issue body) was implemented — PR review MUST see both green.
