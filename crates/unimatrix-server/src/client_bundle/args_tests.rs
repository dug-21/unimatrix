//! Flag-matrix tests for [`super::ClientBundleArgs`] (vnc-050 C7, R-14).
//! Parses through clap exactly as `main.rs` does, then re-checks `action()`.

use clap::Parser;

use super::*;

#[derive(Parser, Debug)]
struct Harness {
    #[command(flatten)]
    args: ClientBundleArgs,
}

fn parse(argv: &[&str]) -> Result<Action, String> {
    let mut full = vec!["client-bundle"];
    full.extend_from_slice(argv);
    let h = Harness::try_parse_from(full).map_err(|e| format!("clap: {e}"))?;
    h.args.action().map_err(|UsageError(m)| m)
}

#[test]
fn test_action_invalid_combos_rejected() {
    let bad: &[&[&str]] = &[
        &["alpha", "--mode", "read"],
        &["alpha", "--label", "ci"],
        &["alpha", "--headless"],
        &["alpha", "--headless", "--mode", "read"],
        &["alpha", "--headless", "--label", "ci"],
        &["alpha", "--list", "--trust", "ca"],
        &["alpha", "--revoke", "ci", "--trust", "pin"],
        &["alpha", "--list", "--revoke", "ci"],
        &[
            "alpha",
            "--list",
            "--headless",
            "--mode",
            "read",
            "--label",
            "ci",
        ],
        &[
            "alpha",
            "--revoke",
            "ci",
            "--headless",
            "--mode",
            "read",
            "--label",
            "ci",
        ],
        &["alpha", "--headless", "--mode", "admin", "--label", "ci"],
        &[
            "alpha",
            "--headless",
            "--mode",
            "read",
            "--label",
            "Bad_Label",
        ],
        &["alpha", "--trust", "none"],
        &[],
    ];
    for argv in bad {
        assert!(parse(argv).is_err(), "must reject {argv:?}");
    }
}

#[test]
fn test_action_valid_shapes_parse() {
    assert_eq!(parse(&["alpha"]), Ok(Action::Legacy { trust: Trust::Pin }));
    assert_eq!(
        parse(&["alpha", "--trust", "ca"]),
        Ok(Action::Legacy { trust: Trust::Ca })
    );
    assert_eq!(
        parse(&[
            "alpha",
            "--headless",
            "--mode",
            "read-write",
            "--label",
            "ci-1"
        ]),
        Ok(Action::Mint {
            mode: TokenMode::ReadWrite,
            label: "ci-1".to_string(),
            trust: Trust::Pin
        })
    );
    assert_eq!(
        parse(&[
            "alpha",
            "--headless",
            "--mode",
            "read",
            "--label",
            "ci",
            "--trust",
            "ca"
        ]),
        Ok(Action::Mint {
            mode: TokenMode::Read,
            label: "ci".to_string(),
            trust: Trust::Ca
        })
    );
    assert_eq!(parse(&["alpha", "--list"]), Ok(Action::List));
    assert_eq!(
        parse(&["alpha", "--revoke", "ci"]),
        Ok(Action::Revoke {
            label: "ci".to_string()
        })
    );
}

#[test]
fn test_action_belt_and_braces_without_clap() {
    // action() re-checks even if clap was bypassed.
    let base = ClientBundleArgs {
        slug: "alpha".to_string(),
        headless: false,
        mode: None,
        label: None,
        trust: None,
        list: false,
        revoke: None,
    };
    let mut a = base.clone();
    a.mode = Some("read".to_string());
    assert!(a.action().is_err());
    let mut a = base.clone();
    a.list = true;
    a.revoke = Some("x".to_string());
    assert!(a.action().is_err());
    let mut a = base;
    a.list = true;
    a.trust = Some("ca".to_string());
    assert!(a.action().is_err());
}
