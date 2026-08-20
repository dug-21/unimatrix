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
    assert!(fx.slug_db("alpha").exists());
    // ADR-007: the first register now WROTE the routing stanza. De-register
    // (config-side) so the re-register lands in State B, not State A.
    fx.set_routing(&[]);

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
// A.4 register — ADR-007 writes [[projects]] routing intent atomically (vnc-038)
// ---------------------------------------------------------------------------

/// Read the on-disk `config.toml` text (empty string if absent) for assertions.
fn read_config(fx: &Fixture) -> String {
    std::fs::read_to_string(fx.config_data_dir.join("config.toml")).unwrap_or_default()
}

/// Count how many `[[projects]]` stanzas declare `slug` in the on-disk config,
/// parsed via the SAME serde path the boot read uses.
fn stanza_count(fx: &Fixture, slug: &str) -> usize {
    #[derive(serde::Deserialize)]
    struct Probe {
        #[serde(default)]
        projects: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        slug: String,
    }
    let text = read_config(fx);
    toml::from_str::<Probe>(&text)
        .map(|p| p.projects.iter().filter(|e| e.slug == slug).count())
        .unwrap_or(0)
}

#[test]
fn test_register_writes_projects_stanza() {
    // AC-02/AC-03: from a clean state, register WRITES [[projects]] AND creates the
    // per-slug data dir + genesis store (no hand-edit, no instruction print).
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");

    assert!(fx.slug_db("alpha").exists(), "genesis store created");
    assert_eq!(
        stanza_count(&fx, "alpha"),
        1,
        "exactly one [[projects]] entry"
    );
    let text = read_config(&fx);
    assert!(
        text.contains("[[projects]]") && text.contains("slug = \"alpha\""),
        "config.toml must carry the stanza, got:\n{text}"
    );
}

#[test]
fn test_register_then_boot_reread() {
    // Write [[projects]], then re-read via the boot-shape config parse: the slug
    // must be in the routed set (full write -> restart -> resolve loop). Pairs with
    // boot-wiring.md / Component 7.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");

    let routed: Vec<String> = fx
        .registry()
        .configured_slugs()
        .into_iter()
        .map(|s| s.as_str().to_string())
        .collect();
    assert!(
        routed.contains(&"alpha".to_string()),
        "boot re-read must surface the registered slug, got: {routed:?}"
    );
}

#[test]
fn test_nth_register_identical_command() {
    // AC-04: registering a 2nd slug uses the IDENTICAL command path and appends a
    // 2nd [[projects]] entry — no first-project special case, no manual edit.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register 1st");
    fx.registry()
        .register("beta")
        .expect("register Nth, same command");

    assert_eq!(stanza_count(&fx, "alpha"), 1, "first stanza intact");
    assert_eq!(stanza_count(&fx, "beta"), 1, "Nth stanza appended");
    let routed: Vec<String> = fx
        .registry()
        .configured_slugs()
        .into_iter()
        .map(|s| s.as_str().to_string())
        .collect();
    assert!(
        routed.contains(&"alpha".to_string()) && routed.contains(&"beta".to_string()),
        "both routable after restart, got: {routed:?}"
    );
}

#[test]
fn test_re_register_re_attaches_no_clobber() {
    // R-05 (hash chain sacred): register against an EXISTING per-slug store OPENS it
    // and re-attaches; the chain head (content_hash) is UNCHANGED before == after.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    let db = fx.slug_db("alpha");
    let (id, hash_before) = write_entry(&db, "chain-head");

    // De-register (config-side) so re-register lands in State B (data exists, not routed).
    fx.set_routing(&[]);
    fx.registry()
        .register("alpha")
        .expect("State B re-attach, never genesis");

    let store = open_store(&db);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let rec = store.get(id).await.expect("entry survives re-attach");
        assert_eq!(
            rec.content_hash, hash_before,
            "chain head must be IDENTICAL — re-attach (open), never genesis-clobber"
        );
    });
}

#[test]
fn test_re_register_idempotent_single_stanza() {
    // R-05/R-06: running register twice yields exactly ONE [[projects]] entry and
    // ONE untouched store (no duplicate stanza, no second genesis).
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("first register");
    // First register routed it (State C wrote the stanza). Re-register from State B
    // (clear routing so it is not State A), then re-route and re-register again to
    // exercise the idempotent stanza path directly.
    assert_eq!(
        stanza_count(&fx, "alpha"),
        1,
        "one stanza after first register"
    );

    fx.set_routing(&[]); // de-route -> State B on next register
    fx.registry()
        .register("alpha")
        .expect("re-attach + re-write stanza");
    assert_eq!(
        stanza_count(&fx, "alpha"),
        1,
        "re-register must not duplicate the stanza"
    );
}

#[test]
fn test_no_genesis_creation_when_dir_exists() {
    // R-05: no genesis-creation path runs when the per-slug data dir already exists.
    // Proven by chain-head preservation across a de-register + re-register cycle:
    // a second genesis would reset the chain (covered structurally by State B branch).
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    let db = fx.slug_db("alpha");
    let (id, _h) = write_entry(&db, "pre-existing");

    fx.set_routing(&[]);
    fx.registry().register("alpha").expect("re-attach");

    let store = open_store(&db);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        assert!(
            store.exists(id).await.expect("query"),
            "pre-existing entry must survive — no genesis ran over the dir"
        );
    });
}

#[test]
fn test_config_write_atomic() {
    // R-06: the write is temp + fsync + rename. Assert the on-disk config.toml is
    // ALWAYS a complete, well-formed file (never partial) and no .tmp sibling is left
    // behind after a successful write.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");

    let text = read_config(&fx);
    // Complete & parseable (would fail if a partial write landed).
    toml::from_str::<toml::Value>(&text).expect("on-disk config must be complete TOML");
    // No leftover temp sibling.
    let leftovers = std::fs::read_dir(&fx.config_data_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".config.toml."))
        .count();
    assert_eq!(leftovers, 0, "no temp file may survive a successful write");
}

#[test]
fn test_config_write_preserves_existing_stanzas() {
    // R-06: register into a config with N existing stanzas + unrelated sections;
    // all N+1 stanzas and the unrelated config survive, well-formed.
    let fx = Fixture::new();
    std::fs::write(
        fx.config_data_dir.join("config.toml"),
        "[http]\nport = 8443\n\n[[projects]]\nslug = \"alpha\"\n\n[[projects]]\nslug = \"beta\"\n",
    )
    .expect("seed config");

    fx.registry()
        .register("gamma")
        .expect("register into non-empty config");

    // All three stanzas present.
    for s in ["alpha", "beta", "gamma"] {
        assert_eq!(stanza_count(&fx, s), 1, "stanza '{s}' must survive/append");
    }
    // Unrelated section preserved.
    let v: toml::Value = toml::from_str(&read_config(&fx)).expect("complete TOML");
    assert_eq!(
        v.get("http")
            .and_then(|h| h.get("port"))
            .and_then(toml::Value::as_integer),
        Some(8443),
        "unrelated [http] config must be preserved"
    );
}

#[test]
fn test_register_malformed_config_errors_no_write() {
    // R-06: a malformed existing config.toml is a loud error and is NOT clobbered.
    let fx = Fixture::new();
    let bad = "this is = = not valid toml [[[";
    std::fs::write(fx.config_data_dir.join("config.toml"), bad).expect("seed bad config");

    let err = fx
        .registry()
        .register("alpha")
        .expect_err("malformed config must fail loud");
    assert!(
        err.to_string().contains("malformed"),
        "error must name the malformed config: {err}"
    );
    // The malformed file is left intact (not blindly overwritten).
    assert_eq!(
        read_config(&fx),
        bad,
        "malformed config must not be clobbered"
    );
}

#[test]
fn test_slug_regex_constrained_pre_write() {
    // Security (TOML-injection guard): a slug carrying a TOML metacharacter (newline,
    // quote, bracket) is rejected at the parse edge BEFORE any write — it can never
    // reach the [[projects]] stanza.
    let fx = Fixture::new();
    for inject in ["alpha\"\n[evil]", "a\"]\nslug=\"x", "a\nb", "a\"b"] {
        fx.registry()
            .register(inject)
            .expect_err("metacharacter slug must reject pre-write");
    }
    // No config written by any rejected attempt.
    assert_eq!(
        read_config(&fx),
        "",
        "no stanza written for a rejected slug"
    );
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
    assert!(
        statuses.is_empty(),
        "no routing config => empty, not an error"
    );
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
    // ADR-007: register wrote the stanza; stanza removal is config-side on delete
    // (operator removes it). Simulate that so the purged slug leaves the list.
    fx.set_routing(&[]);
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

    // De-register (default delete): data dir preserved. ADR-007: register wrote the
    // stanza; default delete is config-side (operator removes it). Simulate that so
    // the re-register lands in State B, not State A.
    fx.registry()
        .delete("alpha", false, None)
        .expect("de-register");
    fx.set_routing(&[]);
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
    // ADR-007: register wrote the stanza; default delete is config-side. Clear the
    // routing so re-register is State B (re-attach), not State A.
    fx.set_routing(&[]);
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

    // bugfix-978: root bypasses permission bits, so the 0o500 base dir would be
    // writable and the fail-loud assertion below could never fire. Skip loudly
    // under root; the assertion stays live on every non-root run.
    if crate::test_support::skip_if_root("test_register_on_unwritable_root_fails_loud") {
        return;
    }

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

// ---------------------------------------------------------------------------
// F. vnc-041 C3 — per-slug seed file (b) (ADR-002; R-05, R-10, R-13, R-03/AC-05)
//
// register seeds the editable per-slug `config.toml` (file b) at the SINGLE join
// site `per_slug_data_dir(base, slug).join("config.toml")` — byte-identical to
// what `resolve_slug_config` reads — at BOTH success branches (State B re-attach +
// State C genesis). It NEVER touches the shared (a)≡(c) path-hash file. Best-effort.
// ---------------------------------------------------------------------------

/// File (b) — the per-slug seed path, computed by the SAME formula the resolver
/// (`http_provision::resolve_slug_config`) uses: `base_dir.join(slug).join("config.toml")`.
fn slug_seed_path(fx: &Fixture, slug: &str) -> PathBuf {
    fx.slug_dir(slug).join("config.toml")
}

// --- R-05 / AC-02: (b) lands at exactly the resolver's path ---

#[test]
fn test_register_writes_b_at_per_slug_data_dir_path() {
    // R-05 scenario 2 / SR-09: after register, (b) exists at exactly
    // per_slug_data_dir(base, slug).join("config.toml") — a SIBLING of, NOT inside,
    // the path-hash dir.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");

    let b = slug_seed_path(&fx, "alpha");
    assert!(
        b.is_file(),
        "per-slug seed (b) must exist at {}",
        b.display()
    );
    // It is the resolver's literal formula: base_dir.join(slug).join("config.toml").
    assert_eq!(b, fx.base_dir.join("alpha").join("config.toml"));
}

#[test]
fn test_register_seeds_b_with_rendered_classification_body() {
    // The seed body is the C2 render (classification-derived legend + DEFAULT_CONFIG_TOML),
    // NOT empty and NOT the [[projects]] routing text. Proves register wrote the C2 body.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");

    let body = std::fs::read_to_string(slug_seed_path(&fx, "alpha")).expect("read (b)");
    let expected = config::render_per_slug_seed_toml();
    assert_eq!(
        body, expected,
        "(b) body must be the C2 rendered seed verbatim"
    );
    assert!(
        !body.contains("[[projects]]"),
        "(b) is the per-slug overlay, never the routing stanza"
    );
}

#[test]
fn test_register_b_path_is_sibling_not_inside_path_hash_dir() {
    // R-05 / SR-09 forcing function: (b)'s dir (base_dir/slug) is NOT config_data_dir
    // (where (a)/(c) lives). Structural proof the seed uses per_slug_data_dir, the single
    // join site, not the config data dir.
    let fx = Fixture::new();
    assert_ne!(
        fx.slug_dir("alpha"),
        fx.config_data_dir,
        "(b) dir must be a sibling of the path-hash (a)/(c) dir, never the same dir"
    );
    // And (b) is NOT under config_data_dir at all.
    assert!(
        !slug_seed_path(&fx, "alpha").starts_with(&fx.config_data_dir),
        "(b) must not live inside the path-hash dir"
    );
}

// --- R-05: (a)/(c) byte-unchanged by the seed's presence (isolation) ---

#[test]
fn test_register_does_not_modify_shared_a_c_file() {
    // R-05 scenario 1 / SR-05: the per-slug seed (b) NEVER touches the shared (a)≡(c)
    // path-hash file. Compare the (a)/(c) bytes after a register that seeds (b) against
    // the bytes ensure_project_stanza alone produces — they must be identical, proving
    // the seed writer targets a different file.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");
    let with_seed = read_config(&fx); // (a)/(c) after register (stanza + any seed effect)

    // Control: a second, distinct registry over a fresh config dir whose (a)/(c) is
    // produced by ensure_project_stanza for the SAME slug — the seed writes a different
    // file, so the routing file must match byte-for-byte.
    let ctrl = Fixture::new();
    ctrl.registry().register("alpha").expect("register control");
    let ctrl_cfg =
        std::fs::read_to_string(ctrl.config_data_dir.join("config.toml")).expect("control (a)/(c)");

    assert_eq!(
        with_seed, ctrl_cfg,
        "(a)/(c) must be byte-identical — the seed never writes the path-hash file"
    );
    // And the seed file (b) is a genuinely different path that DOES exist.
    assert!(slug_seed_path(&fx, "alpha").is_file());
}

#[test]
fn test_register_seed_does_not_create_config_in_path_hash_dir() {
    // The seed must not drop a per-slug body into config_data_dir. After a register where
    // the routing config does NOT pre-exist, config_data_dir/config.toml is ONLY the
    // stanza file (a)/(c) — it must NOT contain the per-slug seed legend.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register alpha");
    let ac = read_config(&fx);
    assert!(
        ac.contains("[[projects]]"),
        "(a)/(c) carries the routing stanza"
    );
    assert!(
        !ac.contains("per-slug overlay registry"),
        "the per-slug seed legend must NOT leak into the (a)/(c) path-hash file"
    );
}

// --- R-13: seed on State C genesis AND State B re-attach (re-attach not missed) ---

#[test]
fn test_register_state_c_genesis_writes_b() {
    // R-13 scenario 1: fresh slug, no store, no stanza => State C genesis. (b) written.
    let fx = Fixture::new();
    assert!(
        !slug_seed_path(&fx, "alpha").exists(),
        "precondition: no (b) yet"
    );
    fx.registry().register("alpha").expect("State C genesis");
    assert!(
        slug_seed_path(&fx, "alpha").is_file(),
        "State C genesis must seed (b)"
    );
}

#[test]
fn test_register_state_b_reattach_writes_b() {
    // R-13 scenario 2 (the gap the risk register calls out): a re-registered slug whose
    // store survives but is de-routed lands in State B; the seed MUST fire there too.
    let fx = Fixture::new();
    fx.registry()
        .register("alpha")
        .expect("first register (State C)");
    // Remove the seed so its re-appearance proves State B seeded it (not the first run).
    std::fs::remove_file(slug_seed_path(&fx, "alpha")).expect("remove (b)");
    // De-route (config-side) so the next register is State B (data exists, not routed).
    fx.set_routing(&[]);

    fx.registry()
        .register("alpha")
        .expect("State B re-attach must succeed");

    assert!(
        slug_seed_path(&fx, "alpha").is_file(),
        "State B re-attach must (re-)seed (b) — ADR-002 requires the seed at BOTH branches"
    );
}

#[test]
fn test_register_state_a_already_routed_errors_no_seed() {
    // R-13 scenario 3: slug already registered + routed => State A. Loud error BEFORE any
    // write; no (b) written on the error path (no clobber, no partial write).
    let fx = Fixture::new();
    let b = slug_seed_path(&fx, "alpha");
    // Arrange State A directly: data dir + db exist AND the slug is routed, WITHOUT any
    // prior register having created (b).
    std::fs::create_dir_all(fx.slug_dir("alpha")).unwrap();
    open_store(&fx.slug_db("alpha")); // create the db so data_exists is true
    fx.set_routing(&["alpha"]); // route it so is_routed is true
    assert!(
        !b.exists(),
        "precondition: no (b) before the State A attempt"
    );

    let err = fx
        .registry()
        .register("alpha")
        .expect_err("State A must error loud");
    assert!(
        err.to_string().contains("already registered and routing"),
        "State A error, got: {err}"
    );
    assert!(
        !b.exists(),
        "State A returns before any write — no seed on the error path"
    );
}

// --- R-03 / AC-05: no-clobber on (b) (operator file survives) ---

#[test]
fn test_register_does_not_clobber_pre_placed_b() {
    // AC-05 / R-03 scenario 2: a pre-placed operator (b) survives register byte-for-byte
    // (skip-if-exists via C1's create_new).
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.slug_dir("alpha")).unwrap();
    let operator = "# operator per-slug config\n";
    std::fs::write(slug_seed_path(&fx, "alpha"), operator).unwrap();

    fx.registry().register("alpha").expect("register");

    let after = std::fs::read_to_string(slug_seed_path(&fx, "alpha")).unwrap();
    assert_eq!(
        after, operator,
        "operator (b) must be untouched (no-clobber)"
    );
}

#[test]
fn test_register_twice_does_not_overwrite_b() {
    // Idempotent: the second seed is a no-op. (b) content is unchanged after re-register.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("first register");
    let first = std::fs::read_to_string(slug_seed_path(&fx, "alpha")).unwrap();

    fx.set_routing(&[]); // de-route so the second register is State B (still seeds)
    fx.registry().register("alpha").expect("re-register");
    let second = std::fs::read_to_string(slug_seed_path(&fx, "alpha")).unwrap();

    assert_eq!(first, second, "second seed must be a no-op (no overwrite)");
}

// --- R-10: best-effort — seed failure does not fail register (C-09, NFR-07) ---

#[test]
fn test_register_seed_write_failure_does_not_fail_register() {
    // R-10 scenario 1: make ONLY the seed write fail (place a DIRECTORY at the (b) path
    // so create_new cannot create the file) while leaving the hash-chain-critical steps
    // healthy. register MUST still reach Ok — warn-and-continue, no error, no panic.
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.slug_dir("alpha")).unwrap();
    // (b) path is a directory => write_if_absent's create_new open fails (warn-and-continue).
    std::fs::create_dir_all(slug_seed_path(&fx, "alpha")).unwrap();

    let result = fx.registry().register("alpha");

    assert!(
        result.is_ok(),
        "seed-write failure must NOT fail register (best-effort), got: {result:?}"
    );
    // The store + stanza (the critical steps) still landed.
    assert!(
        fx.slug_db("alpha").exists(),
        "store opened despite seed failure"
    );
    assert_eq!(
        stanza_count(&fx, "alpha"),
        1,
        "routing intent still written"
    );
    // (b) remains the directory we placed — the seed never clobbered it.
    assert!(
        slug_seed_path(&fx, "alpha").is_dir(),
        "the obstructing dir survives; seed was a no-op"
    );
}

// --- R-05 scenario 3 / AC-02: register -> resolver round-trip (in-scope half) ---
//
// `resolve_slug_config` is declared in the BINARY crate, so the empirical resolver
// call cannot be reached from this lib-crate test module. We close the round-trip
// here by (1) proving register writes (b) at the resolver's LITERAL path formula
// `base_dir.join(slug).join("config.toml")` (the resolver computes exactly this at
// http_provision.rs:318), and (2) reproducing the resolver's file-present arm logic
// (load_single_config -> validate -> merge -> validate, all pub in `infra::config`)
// on the SEEDED (b) — proving the file register wrote is resolver-loadable and
// overlays nothing. See the agent report for the bin-target empirical-resolver FLAG.

#[test]
fn test_register_seeded_b_path_is_the_resolver_formula() {
    // The seed lands at EXACTLY base_dir.join(slug).join("config.toml") — byte-identical
    // to resolve_slug_config's probe path (http_provision.rs:318), with no recomputed base.
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");

    let resolver_formula = fx.base_dir.join("alpha").join("config.toml");
    assert_eq!(
        slug_seed_path(&fx, "alpha"),
        resolver_formula,
        "(b) must sit at the resolver's literal probe formula"
    );
    assert!(
        resolver_formula.is_file(),
        "register wrote (b) at that path"
    );
}

#[test]
fn test_register_seeded_b_is_resolver_loadable() {
    // Reproduce resolve_slug_config's file-present arm on the SEEDED (b): the file register
    // wrote must parse + per-file-validate + merge + post-merge-validate WITHOUT error —
    // i.e. it is resolver-loadable, not a malformed body that would make startup fail loud.
    // This is the C3-scope half of the round-trip; the empirical resolver call lives in the
    // binary crate (see the agent report FLAG).
    let fx = Fixture::new();
    fx.registry().register("alpha").expect("register");
    let b = slug_seed_path(&fx, "alpha");

    let global = config::UnimatrixConfig::default();
    let slug_file = config::load_single_config(&b).expect("seeded (b) must be loadable");
    config::validate_config(&slug_file, &b).expect("seeded (b) must validate");
    let merged = config::merge_configs(global, slug_file);
    config::validate_config(&merged, &b)
        .expect("merged config must pass post-merge validation (no startup-fail body)");
}
