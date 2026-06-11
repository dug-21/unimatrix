//! Tests for the project lifecycle registry + CLI (vnc-034 Wave 2).
//!
//! Covers register (FR-C3/FR-C4, D5 reserved, D6 two-state), list (D3 status, no
//! network), delete/--purge/re-attach (D4 integrity), and fail-loud provisioning.
//! Each test drives a [`ProjectRegistry`] over a temp base dir via `with_dirs`, so
//! no env mutation and no `~/.unimatrix` leakage.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use unimatrix_core::Store;
use unimatrix_store::{NewEntry, PoolConfig, Status};

use super::*;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A test harness owning the temp base dir (per-slug trees) and config data dir
/// (where the routing `config.toml` lives).
struct Fixture {
    _base: TempDir,
    _cfg: TempDir,
    base_dir: PathBuf,
    config_data_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let base = TempDir::new().expect("base temp dir");
        let cfg = TempDir::new().expect("cfg temp dir");
        let base_dir = base.path().to_path_buf();
        let config_data_dir = cfg.path().to_path_buf();
        Fixture {
            _base: base,
            _cfg: cfg,
            base_dir,
            config_data_dir,
        }
    }

    fn registry(&self) -> ProjectRegistry {
        ProjectRegistry::with_dirs(self.base_dir.clone(), self.config_data_dir.clone())
    }

    /// Per-slug data dir under the base (mirrors `per_slug_data_dir`).
    fn slug_dir(&self, slug: &str) -> PathBuf {
        self.base_dir.join(slug)
    }

    fn slug_db(&self, slug: &str) -> PathBuf {
        self.slug_dir(slug).join(PROJECT_DB_NAME)
    }

    /// Write a `config.toml` declaring `slugs` as `[[projects]]` routing intent.
    fn set_routing(&self, slugs: &[&str]) {
        let mut text = String::new();
        for s in slugs {
            text.push_str(&format!("[[projects]]\nslug = \"{s}\"\n\n"));
        }
        std::fs::write(self.config_data_dir.join("config.toml"), text).expect("write config.toml");
    }
}

/// Synchronously open a slug's store (current-thread runtime) for assertions.
fn open_store(db: &Path) -> Store {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        Store::open(db, PoolConfig::default())
            .await
            .expect("open store")
    })
}

/// Insert one entry into a slug's store and return (entry_id, content_hash).
fn write_entry(db: &Path, title: &str) -> (u64, String) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = Store::open(db, PoolConfig::default())
            .await
            .expect("open store");
        let id = store
            .insert(NewEntry {
                title: title.to_string(),
                content: format!("content for {title}"),
                topic: "test".to_string(),
                category: "pattern".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "vnc-034".to_string(),
                trust_source: "test".to_string(),
            })
            .await
            .expect("insert entry");
        let rec = store.get(id).await.expect("read entry back");
        (id, rec.content_hash)
    })
}

// ---------------------------------------------------------------------------
// A. register — happy path (FR-C3, FR-C4, AC-W2-R4)
// ---------------------------------------------------------------------------

#[test]
fn test_register_creates_per_slug_store_dir() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");

    assert!(fx.slug_dir("alpha").is_dir(), "per-slug dir must exist");
    assert!(fx.slug_db("alpha").exists(), "per-slug db must exist");
    assert!(
        fx.slug_dir("alpha").join(PROJECT_VECTOR_DIR).is_dir(),
        "per-slug vector dir must exist"
    );
}

#[test]
fn test_register_adds_slug_to_registry() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");
    // `list` is config-driven (the routing source of truth): after the operator
    // adds the printed [[projects]] stanza, the slug appears with [store: ok].
    fx.set_routing(&["alpha"]);

    let statuses = fx.registry().scan_registered().expect("scan");
    let alpha = statuses
        .iter()
        .find(|s| s.slug.as_str() == "alpha")
        .expect("list/scan must include the registered+routed slug");
    assert_eq!(alpha.store_open, Some(true), "store present => ok");
}

#[test]
fn test_register_validates_slug_via_newtype() {
    let fx = Fixture::new();
    for bad in ["My_Project", "../etc", "a/b", "a%2fb", "Alpha", "a_b", ""] {
        let err = fx
            .registry()
            .register(bad)
            .expect_err("invalid slug must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("invalid project slug"),
            "expected charset rejection for '{bad}', got: {msg}"
        );
    }
    // No directory may be created on rejection (AC-W2-R6: reject before any FS use).
    let count = std::fs::read_dir(&fx.base_dir)
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(count, 0, "no dir may be created on a rejected slug");
}

#[test]
fn test_register_rejects_overlength_slug() {
    let fx = Fixture::new();
    // 64 chars — over the 63-char DNS bound (D1). MUST reject.
    let slug = "a".repeat(64);
    let err = fx
        .registry()
        .register(&slug)
        .expect_err("over-length rejects");
    assert!(err.to_string().contains("invalid project slug"));
    assert!(!fx.slug_dir(&slug).exists(), "no dir on rejection");
}

// ---------------------------------------------------------------------------
// A.2 register — D5 reserved-slug refusal (separate from charset)
// ---------------------------------------------------------------------------

#[test]
fn test_register_rejects_reserved_tools_shadowing() {
    let fx = Fixture::new();
    let err = fx
        .registry()
        .register("tools")
        .expect_err("'tools' must reject as reserved");
    let msg = err.to_string();
    assert!(msg.contains("reserved"), "must name reserved: {msg}");
    assert!(
        msg.contains("/v1/tools/..."),
        "tools message must name the default-project shadow: {msg}"
    );
    assert!(!fx.slug_dir("tools").exists(), "no dir for reserved slug");
}

#[test]
fn test_register_rejects_reserved_route_segments() {
    let fx = Fixture::new();
    for reserved in ["v1", "health", "observe", "tools"] {
        let err = fx
            .registry()
            .register(reserved)
            .expect_err("reserved must reject");
        assert!(err.to_string().contains("reserved"));
        assert!(
            !fx.slug_dir(reserved).exists(),
            "no dir created for reserved '{reserved}'"
        );
    }
}

#[test]
fn test_register_reserved_is_separate_from_charset() {
    // The discriminator: 'tools' is charset-valid (ProjectSlug::try_from Ok) yet
    // register rejects it as reserved — proving the reserved check is a SEPARATE
    // layer, not folded into the charset regex.
    assert!(
        ProjectSlug::try_from("tools").is_ok(),
        "'tools' must be charset-valid"
    );
    let fx = Fixture::new();
    let err = fx.registry().register("tools").expect_err("reserved");
    assert!(
        err.to_string().contains("reserved"),
        "rejection must be the reserved layer, not charset"
    );
}

#[test]
fn test_register_reserved_exact_match_only() {
    // Only the four EXACT segments are reserved — near-misses must succeed,
    // guarding against an over-broad starts_with/contains check.
    let fx = Fixture::new();
    for ok in ["toolsx", "v1-prod", "healthcheck", "observer"] {
        fx.registry()
            .register(ok)
            .unwrap_or_else(|e| panic!("'{ok}' should register (not reserved): {e}"));
        assert!(fx.slug_db(ok).exists(), "'{ok}' store must be created");
    }
}

// ---------------------------------------------------------------------------
// A.3 register — D6 two-state idempotence
// ---------------------------------------------------------------------------

#[test]
fn test_register_already_routing_errors_loud() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("first register");
    // Simulate routing: alpha is in [[projects]] AND its data dir exists.
    fx.set_routing(&["alpha"]);

    let err = fx
        .registry()
        .register("alpha")
        .expect_err("State A must be a loud error");
    let msg = err.to_string();
    assert!(
        msg.contains("already registered and routing"),
        "State A message must be the routing-collision one: {msg}"
    );
}

#[test]
fn test_register_dir_exists_deregistered_reattaches() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("first register");
    // No routing config => de-registered. Data dir survives (State B).
    assert!(fx.slug_db("alpha").exists());

    // Re-register => re-attach, NOT an error.
    fx.registry()
        .register("alpha")
        .expect("State B re-attach must succeed (not an error)");
    assert!(fx.slug_db("alpha").exists(), "store preserved on re-attach");
}

#[test]
fn test_register_two_states_distinct_messages() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");

    // State B (de-registered) succeeds — no error to compare; capture State A msg.
    fx.set_routing(&["alpha"]);
    let state_a = fx
        .registry()
        .register("alpha")
        .expect_err("State A errors")
        .to_string();
    assert!(
        state_a.contains("already registered and routing"),
        "State A must be distinguishable, got: {state_a}"
    );
    // State B (clear routing) must NOT emit the same error — it succeeds.
    fx.set_routing(&[]);
    fx.registry()
        .register("alpha")
        .expect("State B must succeed, distinct from State A");
}

// ---------------------------------------------------------------------------
// B. list (AC-W2-R4; D3 status field)
// ---------------------------------------------------------------------------

#[test]
fn test_list_returns_registered_slugs() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");
    fx.registry().register("beta").expect("register beta");
    // `list` is config-driven: both must be routed for `list` to surface them.
    fx.set_routing(&["alpha", "beta"]);

    let slugs: Vec<String> = fx
        .registry()
        .scan_registered()
        .expect("scan")
        .into_iter()
        .map(|s| s.slug.as_str().to_string())
        .collect();
    assert_eq!(slugs, vec!["alpha".to_string(), "beta".to_string()]);
}

#[test]
fn test_list_empty_when_none_registered() {
    let fx = Fixture::new();
    let statuses = fx.registry().scan_registered().expect("scan empty");
    assert!(statuses.is_empty(), "no routing config => empty, not an error");
}

#[test]
fn test_list_may_carry_store_open_status() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    fx.set_routing(&["alpha"]);

    // Present + readable db => store_open Some(true).
    let statuses = fx.registry().scan_registered().expect("scan");
    let alpha = statuses
        .iter()
        .find(|s| s.slug.as_str() == "alpha")
        .expect("alpha present");
    assert_eq!(alpha.store_open, Some(true), "present db => open");

    // Remove the db out-of-band => still listed (config-registered) but the local
    // status reflects "unavailable" (D3: operator-side status reflects reality).
    std::fs::remove_file(fx.slug_db("alpha")).expect("remove db");
    let statuses = fx.registry().scan_registered().expect("scan after rm");
    let alpha = statuses
        .iter()
        .find(|s| s.slug.as_str() == "alpha")
        .expect("still registered in config");
    assert_eq!(
        alpha.store_open,
        Some(false),
        "missing db => status reflects unavailable (local fs only)"
    );
}

#[test]
fn test_list_is_config_driven_not_dir_scan() {
    // Path-hash data_dirs are siblings of slug dirs under `.unimatrix` and HAVE
    // charset-valid names (16-hex). `list` MUST NOT surface them as projects — it
    // is config-driven, not a directory scan. Only the routed slug appears.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");
    fx.set_routing(&["alpha"]);
    // A sibling path-hash-style dir with a db, NOT in [[projects]].
    let hashish = fx.base_dir.join("00131535b7a00d6b");
    std::fs::create_dir_all(&hashish).unwrap();
    std::fs::write(hashish.join(PROJECT_DB_NAME), b"x").unwrap();

    let slugs: Vec<String> = fx
        .registry()
        .scan_registered()
        .expect("scan")
        .into_iter()
        .map(|s| s.slug.as_str().to_string())
        .collect();
    assert_eq!(
        slugs,
        vec!["alpha".to_string()],
        "only config-registered slugs are listed; path-hash dirs are not"
    );
}

// ---------------------------------------------------------------------------
// C. delete / --purge / re-attach — D4 integrity discriminators
// ---------------------------------------------------------------------------

#[test]
fn test_delete_deregisters_and_preserves_data_dir() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    write_entry(&fx.slug_db("alpha"), "entry-1");

    fx.registry()
        .delete("alpha", false, None)
        .expect("default delete = de-register");

    // D4: data dir + db PRESERVED (delete must NOT touch disk).
    assert!(fx.slug_dir("alpha").is_dir(), "data dir preserved");
    assert!(fx.slug_db("alpha").exists(), "db preserved on de-register");
}

#[test]
fn test_purge_requires_slug_confirmation_or_no_destroy() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");

    // Bare --purge (no confirm) => REFUSED, nothing destroyed.
    let err = fx
        .registry()
        .delete("alpha", true, None)
        .expect_err("bare --purge must be refused");
    assert!(err.to_string().contains("re-type the slug"));
    assert!(
        fx.slug_db("alpha").exists(),
        "bare --purge must not destroy"
    );

    // Mismatched confirm => REFUSED, nothing destroyed.
    let err = fx
        .registry()
        .delete("alpha", true, Some("beta"))
        .expect_err("mismatched confirm must be refused");
    assert!(err.to_string().contains("re-type the slug"));
    assert!(fx.slug_db("alpha").exists(), "mismatch must not destroy");

    // Correct confirm => destroyed.
    fx.registry()
        .delete("alpha", true, Some("alpha"))
        .expect("matching confirm purges");
    assert!(
        !fx.slug_dir("alpha").exists(),
        "confirmed purge removes the dir"
    );
}

#[test]
fn test_purge_with_confirmation_removes_dir_and_deregisters() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    fx.registry()
        .delete("alpha", true, Some("alpha"))
        .expect("purge");
    assert!(!fx.slug_dir("alpha").exists(), "dir removed");
    let statuses = fx.registry().scan_registered().expect("scan");
    assert!(
        !statuses.iter().any(|s| s.slug.as_str() == "alpha"),
        "purged slug excluded from list"
    );
}

#[test]
fn test_deregister_reregister_reattaches_to_preserved_chain() {
    // THE highest-value integrity test (D4). register -> write >=2 entries (capture
    // hash-chain head H_last) -> delete (de-register) -> register again -> assert
    // the prior entries survive and the chain head is IDENTICAL (continued, not a
    // fresh genesis).
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    let db = fx.slug_db("alpha");

    let (id1, _h1) = write_entry(&db, "entry-1");
    let (id2, h2) = write_entry(&db, "entry-2");

    // De-register (default delete): data dir preserved.
    fx.registry()
        .delete("alpha", false, None)
        .expect("de-register");
    assert!(db.exists(), "de-register preserves the store");

    // Re-register => re-attach to the PRESERVED store/chain (State B).
    fx.registry()
        .register("alpha")
        .expect("re-register re-attaches");

    // Assert prior entries survive and the chain head is the SAME (no new genesis).
    let store = open_store(&db);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let r1 = store.get(id1).await.expect("entry-1 preserved");
        assert_eq!(r1.title, "entry-1");
        let r2 = store.get(id2).await.expect("entry-2 preserved");
        assert_eq!(
            r2.content_hash, h2,
            "chain head must be preserved (re-attach, not fresh genesis)"
        );
    });
}

#[test]
fn test_purge_then_register_is_fresh_store() {
    // Contrast/guard: after a CONFIRMED purge, the old chain is severed. A
    // subsequent register creates a FRESH store — the prior entry id is gone.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    let db = fx.slug_db("alpha");
    let (id1, _h1) = write_entry(&db, "entry-1");

    fx.registry()
        .delete("alpha", true, Some("alpha"))
        .expect("purge");
    assert!(!fx.slug_dir("alpha").exists(), "purge removed the dir");

    fx.registry()
        .register("alpha")
        .expect("re-register after purge = fresh store");

    let store = open_store(&fx.slug_db("alpha"));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let exists = store.exists(id1).await.expect("query exists");
        assert!(
            !exists,
            "purge severs the old chain — prior entry must be gone"
        );
    });
}

#[test]
fn test_delete_unregistered_slug_errors_loud() {
    let fx = Fixture::new();
    let err = fx
        .registry()
        .delete("ghost", false, None)
        .expect_err("deleting a never-registered slug must error");
    assert!(err.to_string().contains("no on-disk data"));
}

#[test]
fn test_delete_validates_slug() {
    let fx = Fixture::new();
    for bad in ["../etc", "a/b", "Alpha"] {
        let err = fx
            .registry()
            .delete(bad, false, None)
            .expect_err("invalid slug must reject at parse edge");
        assert!(err.to_string().contains("invalid project slug"));
        // And the same for the purge path.
        let err = fx
            .registry()
            .delete(bad, true, Some(bad))
            .expect_err("invalid slug must reject for --purge too");
        assert!(err.to_string().contains("invalid project slug"));
    }
}

// ---------------------------------------------------------------------------
// D. Lifecycle round-trip (AC-W2-R4)
// ---------------------------------------------------------------------------

#[test]
fn test_register_list_delete_roundtrip() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    assert!(fx.slug_db("alpha").exists());
    // Operator adds the printed [[projects]] stanza => routed => appears in list.
    fx.set_routing(&["alpha"]);
    assert!(
        fx.registry()
            .scan_registered()
            .unwrap()
            .iter()
            .any(|s| s.slug.as_str() == "alpha"),
        "routed slug appears in list"
    );

    fx.registry().delete("alpha", false, None).expect("delete");
    // D4: de-register is config-side (operator removes the stanza). Simulate that;
    // `list` then excludes the slug, but the on-disk dir is PRESERVED.
    fx.set_routing(&[]);
    assert!(
        !fx.registry()
            .scan_registered()
            .unwrap()
            .iter()
            .any(|s| s.slug.as_str() == "alpha"),
        "de-registered slug excluded from list"
    );
    assert!(
        fx.slug_dir("alpha").is_dir(),
        "de-register keeps data on disk"
    );
}

#[test]
fn test_register_delete_reregister_roundtrip() {
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    let db = fx.slug_db("alpha");
    let (id, hash) = write_entry(&db, "keep-me");

    fx.registry().delete("alpha", false, None).expect("delete");
    fx.registry().register("alpha").expect("re-register");

    let store = open_store(&db);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let rec = store.get(id).await.expect("entry survived round-trip");
        assert_eq!(rec.content_hash, hash, "chain continued across round-trip");
    });
}

// ---------------------------------------------------------------------------
// E. Fail-loud provisioning (R-11, NFR-03)
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn test_register_on_unwritable_root_fails_loud() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::new();
    // Make the base dir read-only so create_dir_all under it fails.
    let mut perms = std::fs::metadata(&fx.base_dir).unwrap().permissions();
    perms.set_mode(0o500);
    std::fs::set_permissions(&fx.base_dir, perms).unwrap();

    let result = fx.registry().register("alpha");

    // Restore perms before asserting so TempDir cleanup succeeds.
    let mut restore = std::fs::metadata(&fx.base_dir).unwrap().permissions();
    restore.set_mode(0o700);
    std::fs::set_permissions(&fx.base_dir, restore).unwrap();

    let err = result.expect_err("unwritable root must fail loud, not panic");
    assert!(
        err.to_string().contains("failed to create data dir"),
        "error must be actionable: {err}"
    );
}
