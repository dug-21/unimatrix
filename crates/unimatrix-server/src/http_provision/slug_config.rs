//! Per-slug config overlay resolution (vnc-040 C6, ADR-001/002/003 #5209).
//!
//! Split out of `http_provision.rs` (vnc-046 Wave 2, 500-line module cap) as a focused,
//! single-responsibility unit: overlay `{base_dir}/{slug}/config.toml` onto the daemon's
//! already-resolved global config. `build_project_server` (the sibling in `http_provision.rs`)
//! stays the server-assembly responsibility; this stays the config-resolution one.
//!
//! `resolve_slug_config` is re-exported from `http_provision` so the `main.rs` per-slug loop
//! call site (`http_provision::resolve_slug_config`) is unchanged by the split.

use std::borrow::Cow;
use std::path::Path;

use unimatrix_server::error::ServerError;
use unimatrix_server::http::ProjectSlug;
use unimatrix_server::infra::config::{
    UnimatrixConfig, is_per_slug_overlayable, load_single_config, merge_configs, validate_config,
};

/// Per-slug config file name within a slug's data dir (`{base_dir}/{slug}/config.toml`).
/// Shared with Feature B (seeding, #785) — operator hand-places it for Feature A.
pub(super) const PROJECT_CONFIG_NAME: &str = "config.toml";

/// Resolve the per-slug [`UnimatrixConfig`] by overlaying `{base_dir}/{slug}/config.toml`
/// onto the daemon's already-resolved `global` config (vnc-040 C6, ADR-001 #5209).
///
/// Sole owner of the per-slug overlay decision. The THIRD precedence layer atop the
/// established global → project layering (`load_config`), using the IDENTICAL field-level
/// replace discipline (dsn-001 #2286): reuses [`load_single_config`], [`validate_config`],
/// and [`merge_configs`] UNCHANGED — introduces no new load/merge/validate logic.
///
/// - **No file** → [`Cow::Borrowed`]`(global)`: byte-for-byte fallthrough (ADR-002 §4,
///   AC-02, R-03). NO merge, NO load, NO re-derivation — the global config itself is
///   returned, so the single-project / local-UDS majority sees zero behavior change.
/// - **File present** → load → per-file validate (AC-08a) → merge → **post-merge validate**
///   (ADR-003 #5199, SR-01, AC-08b, the #3905 third-layer fix) → [`Cow::Owned`]`(merged)`.
///
/// The post-merge [`validate_config`] is MANDATORY: it runs after [`merge_configs`] and
/// before the merged config is returned, catching cross-field invariants (the
/// `InferenceConfig` sum-of-six fusion-weight constraint, PPR/confidence/custom-preset/size
/// bounds) that EACH file passes alone but the field-by-field merge violates (#3905).
/// Per-file validation alone is provably insufficient for these.
///
/// The reused [`load_single_config`] carries the 64 KiB size cap (#2395) and the
/// `#[cfg(unix)]` `mode() & 0o022` permission check (R-10), now EXERCISED on the new,
/// untrusted per-slug file surface — not assumed. The hash-pin divergence `tracing::warn`
/// (AC-05) is emitted INSIDE [`merge_configs`] unchanged; this helper neither adds nor
/// suppresses it.
///
/// # Errors
///
/// Any load / per-file-validate / post-merge-validate failure returns a
/// [`ServerError::Config`] NAMING the offending slug file — startup fails loud, never a
/// silent request-time fallback (#4583, R-11). No `.unwrap()` / `.expect()` / panic on any
/// path. A missing file is NOT an error (it is the fallthrough sentinel).
pub fn resolve_slug_config<'a>(
    base_dir: &Path,
    slug: &ProjectSlug,
    global: &'a UnimatrixConfig,
) -> Result<Cow<'a, UnimatrixConfig>, ServerError> {
    // (1) Probe path — single-site derivation; `slug` is allowlist-validated, so this
    //     CANNOT escape `{base_dir}/{slug}/` (AC-W2-R6, same join as build_project_server).
    let path = base_dir.join(slug.as_str()).join(PROJECT_CONFIG_NAME);

    // (2) NO-FILE ARM — fallthrough sentinel (ADR-002 §4, FR-08, AC-02, R-03).
    //     A metadata probe that is_file (not a bare .exists() that would also accept a
    //     directory). NotFound is NOT an error — it is the global-only path.
    let is_file = std::fs::metadata(&path)
        .map(|m| m.is_file())
        .unwrap_or(false);
    if !is_file {
        // The global config itself — NO merge, NO re-derivation.
        return Ok(Cow::Borrowed(global));
    }

    // (3) FILE-PRESENT ARM — load → per-file validate → merge → post-merge validate.
    tracing::debug!(slug = %slug, path = %path.display(), "resolving per-slug config overlay");

    // 3a-WARN. Locked-key seam WARN pass (vnc-041 C5, ADR-005 #5239). PURE OBSERVATION:
    //     read the text and raw-parse it to enumerate which keys the file actually SETS,
    //     then WARN (key + slug, content-free) for each key that is NOT per-slug overlayable.
    //     WARN-ONLY (SR-06): this changes the resolution output by exactly nothing — the
    //     locked value is already ignored by `merge_configs`. The raw read/parse NEVER adds a
    //     failure mode: a read or parse error here is swallowed (no WARN), and the canonical,
    //     loud, slug-named error is left to `load_single_config` below (it re-reads the path).
    if let Ok(text) = std::fs::read_to_string(&path) {
        warn_locked_keys(&text, slug);
    }

    // 3a. Parse + hardening (REUSE — 64 KiB cap #2395 + #[cfg(unix)] 0o022 check, R-10).
    let slug_file =
        load_single_config(&path).map_err(|e| config_err(slug, &path, &e.to_string()))?;

    // 3b. Per-file validation (FR-01, AC-08a).
    validate_config(&slug_file, &path).map_err(|e| config_err(slug, &path, &e.to_string()))?;

    // 3c. Merge — THIRD precedence layer (FR-01, FR-02). REUSE merge_configs UNCHANGED.
    //     The LIVE signature takes OWNED values; `global` is borrowed, so clone it once to
    //     feed the merge (one clone per slug-with-a-file, startup-only, negligible).
    //     hash-pin global-wins (#4655) + instructions project-wins (config.rs:3863) ride
    //     INSIDE merge_configs.
    let merged = merge_configs(global.clone(), slug_file);

    // 3d. POST-MERGE re-validation (ADR-003, SR-01, FR-07, AC-08b, R-01) — MANDATORY, after
    //     the merge, before return. Catches cross-field violations (fusion-weight sum-of-six,
    //     PPR, confidence, custom-preset, size bounds) each file passes alone (#3905).
    validate_config(&merged, &path).map_err(|e| config_err(slug, &path, &e.to_string()))?;

    // 3e. Return the owned merged config.
    Ok(Cow::Owned(merged))
}

/// Build a slug-named, startup-fatal [`ServerError::Config`] for a per-slug overlay failure
/// (NFR-05, R-11). Every failure path names the offending slug AND its file path so the
/// operator can locate and fix it.
fn config_err(slug: &ProjectSlug, path: &Path, detail: &str) -> ServerError {
    ServerError::Config(format!(
        "per-slug config for slug '{}' at {}: {detail}",
        slug.as_str(),
        path.display()
    ))
}

/// Emit one `tracing::warn` per global-locked key the per-slug file SETS (vnc-041 C5,
/// ADR-005 #5239). The locked surface DERIVES from the Feature A registry at runtime
/// (`is_per_slug_overlayable == false`) — NO hand-list in B (SR-02/SR-07).
///
/// PURE OBSERVATION — infallible, emits only logs, NEVER errors (SR-06, R-07):
/// - The raw parse is INDEPENDENT of the typed `load_single_config`. On a raw-parse
///   failure this returns with no WARN; the canonical loud, slug-named `ServerError::Config`
///   is left to `load_single_config`. The WARN pass never converts a parseable file into an
///   error, nor pre-empts the typed parse's error.
/// - CONTENT-FREE (C-11, #4749): the WARN names the bounded key + the validated slug newtype
///   ONLY — never the operator's set VALUE.
///
/// Dedup (OQ-C / R-08): the resolver runs once per slug per boot (main.rs per-slug loop), so
/// once-per-resolution IS once-per-boot. A key appears once in the raw table, so the loop
/// visits it once — naturally one WARN per (slug, key) per boot with no dedup structure (and
/// no cross-boot / cross-slug shared state, which R-08 forbids: the WARN is keyed on the
/// `slug` argument so each slug warns independently).
fn warn_locked_keys(text: &str, slug: &ProjectSlug) {
    // Raw parse — degrade silently on failure (no WARN, no error).
    let raw: toml::Value = match toml::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    for key in flatten_present_keys(&raw) {
        // `is_per_slug_overlayable(key) == false` ⇒ GlobalLocked OR unknown/non-seam key.
        // The conservative default also returns false, so a typo'd / unknown key warns too
        // (it is silently ineffective otherwise — desirable signal, ADR-005).
        if !is_per_slug_overlayable(&key) {
            tracing::warn!(
                slug = %slug,
                key = %key,
                "per-slug config sets a global-locked key; value is ignored (managed globally)"
            );
        }
    }
}

/// Flatten the PRESENT keys of a raw per-slug TOML table into dotted identifiers matching
/// the `PER_SLUG_CONFIG_CLASSIFICATION` `key` strings (vnc-041 C5).
///
/// One level of nesting covers the entire registry surface: top-level leaves render as
/// `"key"` (e.g. `permissive`); top-level sub-tables render as `"section.subkey"`
/// (e.g. `inference.embedding_model_sha256`). For table-shaped locks (`tls` / `http`),
/// the sub-keys flatten to `tls.<field>` / `http.<field>`, which are NOT in the registry —
/// so the conservative-unknown default (`is_per_slug_overlayable == false`) still fires the
/// WARN correctly (Gate 3a observation). Deeper nesting is not part of the per-slug seam.
fn flatten_present_keys(raw: &toml::Value) -> Vec<String> {
    let mut keys = Vec::new();
    if let toml::Value::Table(top) = raw {
        for (name, value) in top {
            match value {
                toml::Value::Table(sub) => {
                    for sub_name in sub.keys() {
                        keys.push(format!("{name}.{sub_name}"));
                    }
                }
                _ => keys.push(name.clone()),
            }
        }
    }
    keys
}

#[cfg(test)]
mod slug_config_tests;
