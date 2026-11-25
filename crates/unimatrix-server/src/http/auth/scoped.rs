//! `TokenTableValidator` (vnc-050 C2, ADR-001).
//!
//! Legacy constant-time compare plus a constant-time full scan of the scoped
//! token table. Both always run (no short-circuit between them). A tombstone
//! match is indistinguishable from an unknown token.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use subtle::ConstantTimeEq;

use super::{AuthError, BearerValidator, PrincipalFuture};
use crate::http::principal::{BearerPrincipal, auth_identity};
use crate::http::token_table::TokenTable;
use crate::mcp::identity::ResolvedIdentity;

/// Validator over the legacy token and `tokens.json`.
#[derive(Debug)]
pub(crate) struct TokenTableValidator {
    legacy: Option<[u8; 32]>,
    legacy_enabled: bool,
    table: Arc<TokenTable>,
}

impl TokenTableValidator {
    pub(crate) fn new(legacy: Option<[u8; 32]>, table: Arc<TokenTable>) -> Self {
        Self {
            legacy,
            legacy_enabled: true,
            table,
        }
    }

    /// Disable the legacy token (FR-11); a match then yields `LegacyDisabled`.
    pub(crate) fn with_legacy_enabled(mut self, enabled: bool) -> Self {
        self.legacy_enabled = enabled;
        self
    }

    fn validate_sync(&self, token: &str) -> Result<(ResolvedIdentity, BearerPrincipal), AuthError> {
        // Early reject on non-hex / wrong length reveals nothing (ADR-001).
        let bytes = hex::decode(token).map_err(|_| AuthError::InvalidToken)?;
        let presented: [u8; 32] = bytes.try_into().map_err(|_| AuthError::InvalidToken)?;

        // (1) legacy compare — always computed, even when disabled.
        let legacy_match: bool = match &self.legacy {
            Some(l) => l.ct_eq(&presented).into(),
            None => false,
        };
        // (2) full scan — always run regardless of (1).
        let scoped = self.table.lookup_ct(&presented);

        let principal = if legacy_match {
            if !self.legacy_enabled {
                return Err(AuthError::LegacyDisabled);
            }
            BearerPrincipal::Legacy
        } else {
            match scoped {
                Some(e) => BearerPrincipal::Scoped {
                    label: e.label,
                    mode: e.mode,
                    allowed_slugs: e.allowed_slugs,
                },
                None => return Err(AuthError::InvalidToken),
            }
        };
        Ok((auth_identity(&principal), principal))
    }
}

impl BearerValidator for TokenTableValidator {
    fn validate(
        &self,
        token: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedIdentity, AuthError>> + Send + '_>> {
        let r = self.validate_sync(token).map(|(id, _)| id);
        Box::pin(async move { r })
    }

    fn validate_principal(&self, token: &str) -> PrincipalFuture<'_> {
        let r = self.validate_sync(token);
        Box::pin(async move { r })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::principal::TokenMode;
    use crate::http::token_table::tests::write_table;

    const LEGACY: [u8; 32] = [0xAA; 32];

    fn fixture() -> (tempfile::TempDir, Arc<TokenTable>) {
        let dir = tempfile::tempdir().unwrap();
        write_table(
            dir.path(),
            &[
                ("first", [1; 32], "read", &["alpha"], false),
                ("gone", [2; 32], "read-write", &["alpha"], true),
                ("last", [3; 32], "read-write", &["alpha", "beta"], false),
            ],
        );
        let t = Arc::new(TokenTable::open(dir.path()));
        (dir, t)
    }

    #[test]
    fn legacy_token_matches_returns_legacy_principal() {
        let (_d, t) = fixture();
        let v = TokenTableValidator::new(Some(LEGACY), t);
        let (id, p) = v.validate_sync(&hex::encode(LEGACY)).expect("legacy ok");
        assert_eq!(p, BearerPrincipal::Legacy);
        assert_eq!(id.agent_id, "http-bearer");
    }

    #[test]
    fn legacy_disabled_yields_legacy_disabled() {
        let (_d, t) = fixture();
        let v = TokenTableValidator::new(Some(LEGACY), t).with_legacy_enabled(false);
        assert!(matches!(
            v.validate_sync(&hex::encode(LEGACY)),
            Err(AuthError::LegacyDisabled)
        ));
        assert!(
            v.validate_sync(&hex::encode([1u8; 32])).is_ok(),
            "scoped unaffected"
        );
    }

    #[test]
    fn scoped_match_index_0_and_n_minus_1() {
        let (_d, t) = fixture();
        let v = TokenTableValidator::new(Some(LEGACY), t);
        let (id, p) = v.validate_sync(&hex::encode([1u8; 32])).unwrap();
        assert_eq!(id.agent_id, "token:first");
        assert!(matches!(
            p,
            BearerPrincipal::Scoped {
                mode: TokenMode::Read,
                ..
            }
        ));
        let (_, p) = v.validate_sync(&hex::encode([3u8; 32])).unwrap();
        match p {
            BearerPrincipal::Scoped {
                label,
                mode,
                allowed_slugs,
            } => {
                assert_eq!(label, "last");
                assert_eq!(mode, TokenMode::ReadWrite);
                assert_eq!(allowed_slugs.len(), 2);
            }
            other => panic!("expected scoped, got {other:?}"),
        }
    }

    #[test]
    fn tombstone_match_rejected_after_full_scan() {
        let (_d, t) = fixture();
        let (hit, visits) = t.lookup_ct_counted(&[2; 32]);
        assert_eq!(hit, None);
        assert_eq!(visits, 3);
        let v = TokenTableValidator::new(Some(LEGACY), t);
        assert!(matches!(
            v.validate_sync(&hex::encode([2u8; 32])),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn presented_token_wrong_length_nonhex_uppercase() {
        let (_d, t) = fixture();
        let v = TokenTableValidator::new(Some(LEGACY), t);
        assert!(v.validate_sync("abcd").is_err());
        assert!(v.validate_sync(&"zz".repeat(32)).is_err());
        assert!(v.validate_sync("").is_err());
        // Uppercase hex decodes (legacy behavior); hash compare is exact.
        assert!(v.validate_sync(&hex::encode_upper([1u8; 32])).is_ok());
        assert!(v.validate_sync(&hex::encode([9u8; 32])).is_err());
    }

    #[test]
    fn no_early_exit_scan_visits_all_entries() {
        let (_d, t) = fixture();
        for probe in [[1u8; 32], [9u8; 32], [2u8; 32]] {
            assert_eq!(t.lookup_ct_counted(&probe).1, 3);
        }
    }

    #[test]
    fn static_no_hashmap_no_break() {
        let table_src = include_str!("../token_table.rs");
        let scan = table_src
            .split("fn scan(")
            .nth(1)
            .and_then(|s| s.split("pub(crate) fn lookup_ct(").next())
            .expect("scan body present");
        let loop_body = scan
            .split("for (i, pe)")
            .nth(1)
            .and_then(|s| s.split("if !bool::from(found)").next())
            .expect("scan loop present");
        assert!(!loop_body.contains("break"), "no break in scan loop");
        assert!(!loop_body.contains("return"), "no return in scan loop");
        let me = include_str!("scoped.rs");
        let prod = me.split("#[cfg(test)]").next().unwrap();
        assert!(!prod.contains("HashMap<"));
        assert!(!table_src.contains("HashMap<"));
    }
}
