//! C3 — Public-URL knob: single derivation of `UNIMATRIX_PUBLIC_URL`.
//!
//! `UNIMATRIX_PUBLIC_URL` is the one piece of operator knowledge the cloud
//! cannot auto-derive. [`derive_public_url`] turns it (or a loud placeholder
//! when unset) into a single [`PublicUrl`] value consumed identically by THREE
//! sites — bundle `base_url` (C1), `allowed_hosts` default, and cert SAN
//! (C2/SR-01) — so the cert SAN can never desync from the bundle base-url
//! (R-09). The invariant `bundle.host ∈ cert.sans` (SR-10) holds by
//! construction because all three consumers read this one struct.
//!
//! Socket auto-detect is explicitly REJECTED (FR-A7, R-09): this is a pure
//! function over [`Env`] with no I/O and no socket inspection.
//!
//! The three consumer call sites (cert-provisioner SANs, bundle-codec
//! `base_url`, listener/config `allowed_hosts`) are wired by their owning
//! components in main.rs and sibling modules; `derive_public_url` is consumed
//! by `client_bundle.rs`. The public API is the contract those owners consume.

/// Environment variable carrying the operator-supplied public URL.
const PUBLIC_URL_VAR: &str = "UNIMATRIX_PUBLIC_URL";

/// Loud placeholder base-url emitted when the knob is unset. Intentionally not
/// a valid host, so the operator notices via the `client-bundle` stderr echo
/// (FR-A5b pairing) before distributing the bundle.
const PLACEHOLDER: &str = "https://<EDIT-ME>:8443";

/// Sentinel host emitted when the knob is unset. Yields a localhost-restrictive
/// `allowed_hosts` posture downstream (only the local SANs gate-pass; no public
/// host is admitted) plus a startup WARN; never appended to the SAN list.
const PLACEHOLDER_HOST: &str = "<EDIT-ME>";

/// Local SANs always present regardless of the public URL (loopback + wildcard
/// bind), so a local-UDS / loopback connect always validates against the cert.
const LOCAL_SANS: [&str; 3] = ["localhost", "127.0.0.1", "0.0.0.0"];

/// Default TLS port used when the public URL carries no explicit port.
const DEFAULT_PORT: u16 = 8443;

/// Thin, testable environment accessor.
///
/// Rust 2024 forbids `std::env::set_var` under `#![forbid(unsafe_code)]`, so
/// the function is parameterised over an injectable getter (mirroring
/// `resolve_env_config_path`'s pure-function approach in `config.rs`). Tests
/// exercise every branch without mutating process environment.
pub struct Env<'a> {
    get: &'a dyn Fn(&str) -> Option<String>,
}

impl<'a> Env<'a> {
    /// Construct an `Env` from an arbitrary getter closure.
    pub fn new(get: &'a dyn Fn(&str) -> Option<String>) -> Self {
        Self { get }
    }

    /// Production accessor backed by the real process environment.
    ///
    /// Reading is safe under Rust 2024 — only `set_var`/`remove_var` are unsafe.
    pub fn from_process() -> Env<'static> {
        Env {
            get: &|key| std::env::var(key).ok(),
        }
    }

    fn read(&self, key: &str) -> Option<String> {
        (self.get)(key)
    }
}

/// Single source of truth for every public-facing URL/host/SAN.
///
/// Derived once by [`derive_public_url`] and read verbatim by all three
/// consumers; never reconstructed from a request payload or a bound socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicUrl {
    /// Verbatim bundle base-url, e.g. `"https://cloud.example:8443"`.
    pub base_url: String,
    /// Host with scheme/port/brackets stripped, e.g. `"cloud.example"` or `"::1"`.
    /// Feeds the `allowed_hosts` default and the cert SAN.
    pub host: String,
    /// `["localhost", "127.0.0.1", "0.0.0.0", host]` — `host` omitted when the
    /// placeholder sentinel is in effect (a sentinel is not a real SAN).
    pub sans: Vec<String>,
}

/// Derive the single [`PublicUrl`] from `UNIMATRIX_PUBLIC_URL`.
///
/// Total function — never errors, never panics. An unset, empty, or
/// un-parseable value degrades to the loud placeholder + WARN. A non-https
/// scheme is coerced to https + WARN so the bundle-schema invariant
/// (ADR-001: `base_url` must be https) cannot be violated downstream. Socket
/// auto-detect is not implemented (FR-A7, R-09).
pub fn derive_public_url(env: &Env) -> PublicUrl {
    match env.read(PUBLIC_URL_VAR) {
        None => placeholder(),
        Some(raw) if raw.trim().is_empty() => placeholder(),
        Some(raw) => match parse_public_url(raw.trim()) {
            Some((base_url, host)) => {
                let mut sans: Vec<String> = LOCAL_SANS.iter().map(|s| (*s).to_string()).collect();
                if !sans.iter().any(|s| s == &host) {
                    sans.push(host.clone());
                }
                PublicUrl {
                    base_url,
                    host,
                    sans,
                }
            }
            None => {
                tracing::warn!(
                    value = %raw,
                    "UNIMATRIX_PUBLIC_URL is set but un-parseable — emitting placeholder \
                     base-url. Fix it before distributing the bundle."
                );
                placeholder()
            }
        },
    }
}

/// Build the unset/garbage placeholder result with a startup WARN.
fn placeholder() -> PublicUrl {
    tracing::warn!(
        "UNIMATRIX_PUBLIC_URL unset — emitting placeholder base-url ({PLACEHOLDER}) and \
         localhost-restrictive allowed_hosts (no public host admitted). Set it before \
         distributing the bundle."
    );
    PublicUrl {
        base_url: PLACEHOLDER.to_string(),
        host: PLACEHOLDER_HOST.to_string(),
        sans: LOCAL_SANS.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Tolerantly parse an operator-supplied public URL into `(base_url, host)`.
///
/// Returns `None` only on un-parseable garbage (no host). Tolerances:
/// - missing scheme (`cloud.example:8443`) -> `https://` prepended;
/// - non-https scheme (`http://...`) -> coerced to https + WARN;
/// - trailing path / query / fragment -> discarded;
/// - IPv6 literal (`[::1]`) -> brackets kept in `base_url`, stripped in `host`;
/// - absent port -> [`DEFAULT_PORT`].
fn parse_public_url(input: &str) -> Option<(String, String)> {
    // Split scheme (if any) from the authority.
    let (scheme_present_non_https, authority_and_rest) = match input.split_once("://") {
        Some((scheme, rest)) => {
            let scheme_lc = scheme.to_ascii_lowercase();
            if scheme_lc.is_empty() {
                return None;
            }
            if scheme_lc != "https" {
                (true, rest)
            } else {
                (false, rest)
            }
        }
        None => (false, input),
    };

    if scheme_present_non_https {
        tracing::warn!(
            value = %input,
            "UNIMATRIX_PUBLIC_URL has a non-https scheme — coercing to https (ADR-001 \
             requires the bundle base_url to be https)."
        );
    }

    // Authority ends at the first '/', '?' or '#'; the rest (path/query/frag) is discarded.
    let authority_end = authority_and_rest
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_rest.len());
    let authority = &authority_and_rest[..authority_end];
    if authority.is_empty() {
        return None;
    }

    // Strip userinfo if present (defensive — not expected in this knob).
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);

    let (host, host_in_base, port) = parse_authority(authority)?;
    if host.is_empty() {
        return None;
    }

    let base_url = format!("https://{host_in_base}:{port}");
    Some((base_url, host))
}

/// Parse `host[:port]` / `[ipv6][:port]` into `(bare_host, host_for_base_url, port)`.
///
/// `bare_host` is bracket-free (SAN/allowed_hosts form); `host_for_base_url`
/// keeps IPv6 brackets so `base_url` stays a valid authority.
fn parse_authority(authority: &str) -> Option<(String, String, u16)> {
    if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 literal: [host] or [host]:port
        let (inner, after) = rest.split_once(']')?;
        if inner.is_empty() {
            return None;
        }
        let port = match after.strip_prefix(':') {
            Some(p) if !p.is_empty() => p.parse::<u16>().ok()?,
            Some(_) => return None, // bracket followed by bare ':' with no port
            None if after.is_empty() => DEFAULT_PORT,
            None => return None, // junk after ']'
        };
        Some((inner.to_string(), format!("[{inner}]"), port))
    } else {
        // host or host:port (host has no ':' of its own here)
        match authority.rsplit_once(':') {
            Some((host, port_str)) => {
                let port = port_str.parse::<u16>().ok()?;
                Some((host.to_string(), host.to_string(), port))
            }
            None => Some((authority.to_string(), authority.to_string(), DEFAULT_PORT)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Build a testable `Env` from a fixed key/value map (no `set_var`).
    fn env_with(map: HashMap<String, String>) -> impl Fn(&str) -> Option<String> {
        move |k: &str| map.get(k).cloned()
    }

    fn set(value: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(PUBLIC_URL_VAR.to_string(), value.to_string());
        m
    }

    fn derive(value: Option<&str>) -> PublicUrl {
        let map = match value {
            Some(v) => set(v),
            None => HashMap::new(),
        };
        let getter = env_with(map);
        derive_public_url(&Env::new(&getter))
    }

    // --- Single derivation, three consumers (R-09) ---

    #[test]
    fn test_derive_public_url_base_url_verbatim() {
        let pu = derive(Some("https://cloud.example:8443"));
        assert_eq!(pu.base_url, "https://cloud.example:8443");
    }

    #[test]
    fn test_derive_public_url_host_extracted() {
        let pu = derive(Some("https://cloud.example:8443"));
        assert_eq!(pu.host, "cloud.example");
    }

    #[test]
    fn test_derive_public_url_sans_set() {
        let pu = derive(Some("https://cloud.example:8443"));
        assert_eq!(
            pu.sans,
            vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "cloud.example".to_string(),
            ]
        );
    }

    #[test]
    fn test_three_consumers_read_one_derivation() {
        // Mutating the single input changes host, base_url, and sans together —
        // there is no second/independent host parse in any consumer path.
        let a = derive(Some("https://a.example:8443"));
        let b = derive(Some("https://b.example:8443"));
        assert_ne!(a.base_url, b.base_url);
        assert_ne!(a.host, b.host);
        assert_ne!(a.sans, b.sans);
        // host feeds both base_url and sans from the one parse:
        assert!(a.base_url.contains(&a.host));
        assert!(a.sans.contains(&a.host));
    }

    // --- host ∈ SAN invariant (AC-W1-S9, SR-10) ---

    #[test]
    fn test_bundle_host_in_cert_sans() {
        for url in [
            "https://cloud.example:8443",
            "host.internal:9000",
            "https://[2001:db8::1]:8443",
            "https://only-host",
        ] {
            let pu = derive(Some(url));
            assert!(
                pu.sans.iter().any(|s| s == &pu.host),
                "host {} not in sans {:?} for {url}",
                pu.host,
                pu.sans
            );
        }
    }

    // --- Unset / placeholder behavior ---

    #[test]
    fn test_unset_public_url_yields_edit_me_placeholder() {
        let pu = derive(None);
        assert_eq!(pu.base_url, "https://<EDIT-ME>:8443");
        assert!(pu.base_url.contains("<EDIT-ME>"));
    }

    #[test]
    fn test_unset_public_url_allowed_hosts_localhost_restrictive() {
        // Localhost-restrictive posture: the sentinel host is NOT appended to the
        // SAN list, so allowed_hosts carries only the local SANs (no public host).
        let pu = derive(None);
        assert_eq!(pu.host, "<EDIT-ME>");
        assert_eq!(
            pu.sans,
            vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
            ]
        );
        assert!(!pu.sans.iter().any(|s| s == "<EDIT-ME>"));
    }

    #[test]
    fn test_empty_public_url_yields_placeholder() {
        let pu = derive(Some("   "));
        assert_eq!(pu.base_url, "https://<EDIT-ME>:8443");
    }

    // --- Edge cases (RISK-TEST §Edge Cases) ---

    #[test]
    fn test_explicit_port_carried_into_base_url() {
        let pu = derive(Some("https://cloud.example:9000"));
        assert_eq!(pu.base_url, "https://cloud.example:9000");
        // SAN host has no port.
        assert!(pu.sans.iter().any(|s| s == "cloud.example"));
        assert!(!pu.sans.iter().any(|s| s.contains(':')));
    }

    #[test]
    fn test_path_is_discarded() {
        let pu = derive(Some("https://cloud.example:8443/v1/tools"));
        assert_eq!(pu.base_url, "https://cloud.example:8443");
        assert_eq!(pu.host, "cloud.example");
    }

    #[test]
    fn test_query_and_fragment_discarded() {
        let pu = derive(Some("https://cloud.example:8443/p?q=1#frag"));
        assert_eq!(pu.base_url, "https://cloud.example:8443");
        assert_eq!(pu.host, "cloud.example");
    }

    #[test]
    fn test_ipv6_literal() {
        let pu = derive(Some("https://[::1]:8443"));
        assert_eq!(pu.host, "::1");
        assert_eq!(pu.base_url, "https://[::1]:8443");
        assert!(pu.sans.iter().any(|s| s == "::1"));
    }

    #[test]
    fn test_ipv6_literal_default_port() {
        let pu = derive(Some("https://[2001:db8::1]"));
        assert_eq!(pu.host, "2001:db8::1");
        assert_eq!(pu.base_url, "https://[2001:db8::1]:8443");
    }

    #[test]
    fn test_no_scheme_prepends_https() {
        let pu = derive(Some("cloud.example:8443"));
        assert_eq!(pu.base_url, "https://cloud.example:8443");
        assert_eq!(pu.host, "cloud.example");
    }

    #[test]
    fn test_no_port_uses_default() {
        let pu = derive(Some("https://cloud.example"));
        assert_eq!(pu.base_url, "https://cloud.example:8443");
        assert_eq!(pu.host, "cloud.example");
    }

    #[test]
    fn test_http_scheme_coerced_to_https() {
        let pu = derive(Some("http://cloud.example:8443"));
        assert_eq!(pu.base_url, "https://cloud.example:8443");
        assert_eq!(pu.host, "cloud.example");
    }

    #[test]
    fn test_trailing_slash_normalized() {
        let pu = derive(Some("https://cloud.example:8443/"));
        assert_eq!(pu.base_url, "https://cloud.example:8443");
        assert_eq!(pu.host, "cloud.example");
    }

    #[test]
    fn test_host_equal_to_local_san_not_duplicated() {
        let pu = derive(Some("https://localhost:8443"));
        assert_eq!(pu.host, "localhost");
        // "localhost" already present in LOCAL_SANS — not appended twice.
        let count = pu.sans.iter().filter(|s| *s == "localhost").count();
        assert_eq!(count, 1);
        assert_eq!(pu.sans.len(), 3);
    }

    #[test]
    fn test_garbage_degrades_to_placeholder() {
        // No host -> placeholder, never panic.
        let pu = derive(Some("https://"));
        assert_eq!(pu.base_url, "https://<EDIT-ME>:8443");
    }

    #[test]
    fn test_bad_port_degrades_to_placeholder() {
        let pu = derive(Some("https://cloud.example:not-a-port"));
        assert_eq!(pu.base_url, "https://<EDIT-ME>:8443");
    }

    // --- No socket auto-detect (R-09 negative) ---

    #[test]
    fn test_no_socket_autodetect() {
        // Structural: derive_public_url takes only &Env. Distinct env inputs
        // fully determine output; nothing reads a bound/peer socket address.
        let a = derive(Some("https://from-env-a:8443"));
        let b = derive(Some("https://from-env-b:9000"));
        assert_eq!(a.host, "from-env-a");
        assert_eq!(b.host, "from-env-b");
        assert_eq!(b.base_url, "https://from-env-b:9000");
    }

    #[test]
    fn test_env_is_injectable_without_set_var() {
        // The whole suite proves this; assert explicitly that a custom getter
        // drives the result with no process-env mutation.
        let getter = |k: &str| {
            if k == PUBLIC_URL_VAR {
                Some("https://injected.example:8443".to_string())
            } else {
                None
            }
        };
        let pu = derive_public_url(&Env::new(&getter));
        assert_eq!(pu.host, "injected.example");
    }
}
