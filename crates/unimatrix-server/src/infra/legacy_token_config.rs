//! `http.legacy_token_enabled` env override (vnc-050 C5, ADR-001, FR-17).
//!
//! `UNIMATRIX_LEGACY_TOKEN_ENABLED` is applied after the config-file merge
//! (`load_config` step 3d). Accepted values are exact and case-sensitive:
//! `true|1|false|0`; empty or unset means no override. Anything else is a boot
//! error — deliberately stricter than `UNIMATRIX_HTTP_ENABLED` (no trimming,
//! no lowercasing) so a typo cannot silently re-enable the legacy token.

use super::config::ConfigError;

/// Env var that overrides `http.legacy_token_enabled`.
pub const LEGACY_TOKEN_ENABLED_VAR: &str = "UNIMATRIX_LEGACY_TOKEN_ENABLED";

/// Pure parser for the override value; testable without `set_var`.
/// Non-UTF-8 values are mapped to `None` by the caller (`env::var(..).ok()`).
pub fn resolve_legacy_token_override(v: Option<&str>) -> Result<Option<bool>, ConfigError> {
    match v {
        None | Some("") => Ok(None),
        Some("true") | Some("1") => Ok(Some(true)),
        Some("false") | Some("0") => Ok(Some(false)),
        Some(_) => Err(ConfigError::InvalidLegacyTokenOverride {
            var: LEGACY_TOKEN_ENABLED_VAR,
        }),
    }
}

/// Apply the env override to an already-resolved config. Used by the boot
/// fallback path so a config-load failure can never bypass the override
/// (compiled defaults would otherwise re-enable the legacy token).
pub fn apply_legacy_token_env(
    config: &mut super::config::UnimatrixConfig,
) -> Result<(), ConfigError> {
    let raw = std::env::var(LEGACY_TOKEN_ENABLED_VAR).ok();
    if let Some(v) = resolve_legacy_token_override(raw.as_deref())? {
        config.http.legacy_token_enabled = v;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::config::UnimatrixConfig;

    #[test]
    fn override_accepts_true_1_false_0() {
        assert_eq!(
            resolve_legacy_token_override(Some("true")).unwrap(),
            Some(true)
        );
        assert_eq!(
            resolve_legacy_token_override(Some("1")).unwrap(),
            Some(true)
        );
        assert_eq!(
            resolve_legacy_token_override(Some("false")).unwrap(),
            Some(false)
        );
        assert_eq!(
            resolve_legacy_token_override(Some("0")).unwrap(),
            Some(false)
        );
    }

    #[test]
    fn override_empty_or_unset_is_none() {
        assert_eq!(resolve_legacy_token_override(None).unwrap(), None);
        assert_eq!(resolve_legacy_token_override(Some("")).unwrap(), None);
    }

    #[test]
    #[allow(non_snake_case)]
    fn override_rejects_yes_TRUE_space_2_off() {
        for bad in ["yes", "TRUE ", "TRUE", "2", "off", " true", "False"] {
            let err = resolve_legacy_token_override(Some(bad)).expect_err(bad);
            let msg = err.to_string();
            assert!(msg.contains(LEGACY_TOKEN_ENABLED_VAR), "{msg}");
            assert!(msg.contains("true|1|false|0"), "{msg}");
            if !bad.trim().is_empty() && bad.trim() != "true" && bad.trim() != "TRUE" {
                assert!(!msg.contains(bad.trim()), "value not echoed: {msg}");
            }
        }
    }

    fn parse(body: &str) -> UnimatrixConfig {
        toml::from_str(body).expect("config parses")
    }

    #[test]
    fn config_missing_key_defaults_true() {
        assert!(parse("").http.legacy_token_enabled);
        assert!(
            parse("[http]\nenabled = true\ncontent_port = 9000\n")
                .http
                .legacy_token_enabled
        );
        assert!(
            !parse("[http]\nlegacy_token_enabled = false\n")
                .http
                .legacy_token_enabled
        );
    }

    #[test]
    fn config_env_matrix() {
        let configs: [(&str, bool); 3] = [
            ("", true),
            ("[http]\nlegacy_token_enabled = true\n", true),
            ("[http]\nlegacy_token_enabled = false\n", false),
        ];
        let envs: [(Option<&str>, Option<bool>); 6] = [
            (None, None),
            (Some(""), None),
            (Some("true"), Some(true)),
            (Some("false"), Some(false)),
            (Some("1"), Some(true)),
            (Some("0"), Some(false)),
        ];
        for (body, from_file) in configs {
            for (env, env_val) in envs {
                // Mirrors load_config step 3d.
                let mut cfg = parse(body);
                if let Some(v) = resolve_legacy_token_override(env).unwrap() {
                    cfg.http.legacy_token_enabled = v;
                }
                let want = env_val.unwrap_or(from_file);
                assert_eq!(cfg.http.legacy_token_enabled, want, "{body:?} x {env:?}");
            }
        }
    }
}
