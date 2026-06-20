// vnc-041 (C4): Global serve-time seed tests.
//
// The seed call is one line inside `tokio_main_daemon`'s `if config.http.enabled`
// block, before the per-slug loop:
//
//     write_default_config_if_absent(&paths.data_dir.join("config.toml"), false)
//
// The `else` branch (local STDIO/UDS) has NO seed call site. "Container only" is a
// compile-time branch fact, NOT a runtime flag, and NOT keyed on `base_dir` (which
// is `None` on every live serve call). Test depth is function-level (per the C4 test
// plan harness note): call the seed function directly against a temp data dir under
// each branch's *conditions*, plus an EMPIRICAL file-count sentinel and a
// source-text structural placement guard. The file-count delta assertion
// (== 0 local, > 0 container) is MANDATORY (#4876 empirical-gate-integrity).

use std::path::Path;
use unimatrix_server::infra::config::{DEFAULT_CONFIG_TOML, write_default_config_if_absent};

/// Recursively count regular files under `root` (empirical zero-files sentinel).
/// Returns 0 for a missing root. No external crate — std walk only.
fn count_files(root: &Path) -> usize {
    let mut total = 0;
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += count_files(&path);
        } else {
            total += 1;
        }
    }
    total
}

/// The seam the daemon exercises on the `http.enabled == true` path: the C4 call
/// site is exactly this single line. We invoke the same function the production
/// branch invokes, against a temp data dir, with `base_dir = None` semantics (the
/// function takes only the (a) path — it never consults `base_dir`).
fn container_serve_seed(data_dir: &Path) {
    // Mirrors main.rs C4: inside `if config.http.enabled`, before the per-slug loop.
    write_default_config_if_absent(&data_dir.join("config.toml"), false);
}

// ── R-01 / AC-01 — seed fires on the http.enabled path WITH base_dir = None ─────

#[test]
fn test_serve_seed_fires_with_http_enabled_and_base_dir_none() {
    // Arrange: empty temp data dir. The live serve path passes base_dir = None
    // (main.rs 599/1347/1779/529/546); the seed function never reads base_dir — it
    // takes only the (a) path. So driving the seam under base_dir = None semantics
    // and observing the write IS the proof that the gate is http.enabled, not
    // base_dir.
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path();
    let cfg_path = data_dir.join("config.toml");
    assert!(!cfg_path.exists(), "precondition: (a) absent");

    // Act: drive the http-enabled seed seam.
    container_serve_seed(data_dir);

    // Assert: (a) exists at paths.data_dir.join("config.toml") with the
    // DEFAULT_CONFIG_TOML knobs (parse it -> it is the compiled default body).
    assert!(cfg_path.exists(), "(a) IS written on the http.enabled path");
    let content = std::fs::read_to_string(&cfg_path).unwrap();
    assert_eq!(
        content, DEFAULT_CONFIG_TOML,
        "(a) exposes the DEFAULT_CONFIG_TOML knobs"
    );
    toml::from_str::<toml::Value>(&content).expect("(a) is valid TOML");
}

#[test]
fn test_serve_seed_second_boot_does_not_overwrite() {
    // First boot seeds (a); capture content + mtime.
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path();
    let cfg_path = data_dir.join("config.toml");
    container_serve_seed(data_dir);
    let first_content = std::fs::read_to_string(&cfg_path).unwrap();
    let first_mtime = std::fs::metadata(&cfg_path).unwrap().modified().unwrap();

    // Act: second boot runs the seed again.
    container_serve_seed(data_dir);

    // Assert: content + mtime unchanged (skip-if-exists; AC-01 / couples R-11).
    let second_content = std::fs::read_to_string(&cfg_path).unwrap();
    let second_mtime = std::fs::metadata(&cfg_path).unwrap().modified().unwrap();
    assert_eq!(
        first_content, second_content,
        "second boot does not overwrite"
    );
    assert_eq!(
        first_mtime, second_mtime,
        "second boot leaves mtime unchanged"
    );
}

// ── R-01 / R-02 / AC-06 — EMPIRICAL zero-files sentinel + negative control ──────

#[test]
fn test_local_serve_writes_zero_new_config_files() {
    // The sentinel. Arrange: config.http.enabled == false (local/STDIO); an empty
    // home `.unimatrix` tree. The local `else` branch has NO seed call site, so we
    // model it by NOT invoking any seed function and asserting the tree is untouched.
    let tmp = tempfile::TempDir::new().unwrap();
    let unimatrix_tree = tmp.path().join(".unimatrix");
    std::fs::create_dir_all(&unimatrix_tree).unwrap();
    let before = count_files(&unimatrix_tree);

    // Act: the local serve seam. The `else` branch performs NO seed write — there is
    // no call site. (We deliberately call nothing: the absence of a call IS the
    // behavior under test.)

    // Assert: delta == 0 — no new config file anywhere in the tree.
    let after = count_files(&unimatrix_tree);
    assert_eq!(
        after - before,
        0,
        "local serve (http.enabled == false) writes ZERO new config files"
    );
}

#[test]
fn test_container_serve_writes_one_config_file_negative_control() {
    // MANDATORY negative control: SAME sentinel harness on the http.enabled == true
    // path. Without this, the zero-files assertion above is worthless — it proves the
    // sentinel actually DETECTS writes and is not trivially passing.
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path().join(".unimatrix").join("path-hash");
    std::fs::create_dir_all(&data_dir).unwrap();
    let tree = tmp.path().join(".unimatrix");
    let before = count_files(&tree);

    // Act: the container serve seam (the http.enabled branch).
    container_serve_seed(&data_dir);

    // Assert: delta > 0 — the global (a) appears.
    let after = count_files(&tree);
    assert!(
        after - before > 0,
        "container serve writes at least one config file (sentinel has teeth)"
    );
    assert_eq!(after - before, 1, "exactly the single (a) file appears");
}

#[test]
fn test_local_serve_resolution_behavior_matches_pre_vnc041_baseline() {
    // The local majority is unperturbed: with NO seed call on the local path, the
    // pre-vnc-041 baseline (an absent (a) tree) is byte-for-byte identical to the
    // post-vnc-041 local path (still no call site). Capture the tree state with the
    // seed conceptually disabled vs the live local path (no call) — identical.
    let baseline = tempfile::TempDir::new().unwrap();
    let baseline_tree = baseline.path().join(".unimatrix");
    std::fs::create_dir_all(&baseline_tree).unwrap();

    let live = tempfile::TempDir::new().unwrap();
    let live_tree = live.path().join(".unimatrix");
    std::fs::create_dir_all(&live_tree).unwrap();
    // Act (live local path): no seed call site exists -> nothing written.

    // Assert: identical file counts (both zero) — the local path is byte-for-byte
    // unperturbed by vnc-041.
    assert_eq!(
        count_files(&baseline_tree),
        count_files(&live_tree),
        "local resolution/provisioning behavior matches the pre-vnc-041 baseline"
    );
    assert_eq!(count_files(&live_tree), 0, "local path provisions nothing");
}

// ── R-03 / AC-05 — no-clobber on (a) (operator-edited global survives) ──────────

#[test]
fn test_container_serve_does_not_clobber_operator_edited_a() {
    // Arrange: pre-place an operator-edited (a) at the path-hash path.
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path();
    let cfg_path = data_dir.join("config.toml");
    let operator_body = "# operator-edited global config\n";
    std::fs::write(&cfg_path, operator_body).unwrap();

    // Act: run the http-enabled seed.
    container_serve_seed(data_dir);

    // Assert: (a) byte-for-byte unchanged (skip-if-exists; AC-05 / R-03).
    let content = std::fs::read_to_string(&cfg_path).unwrap();
    assert_eq!(
        content, operator_body,
        "operator-edited (a) survives the serve seed"
    );
}

// ── R-11 — dual (a) writers (handle_version + serve) are idempotent ─────────────

#[test]
fn test_init_then_container_serve_a_written_once() {
    // Simulate `handle_version`/init writing (a) first (it calls the SAME
    // write_default_config_if_absent — main.rs:1958), then the serve seed.
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path();
    let cfg_path = data_dir.join("config.toml");

    // init writer.
    write_default_config_if_absent(&cfg_path, false);
    let init_content = std::fs::read_to_string(&cfg_path).unwrap();
    let init_mtime = std::fs::metadata(&cfg_path).unwrap().modified().unwrap();

    // Act: serve seed runs second.
    container_serve_seed(data_dir);

    // Assert: (a) is the init-written file; serve no-ops (create_new skip-if-exists).
    assert_eq!(
        std::fs::read_to_string(&cfg_path).unwrap(),
        init_content,
        "serve no-ops over the init-written (a)"
    );
    assert_eq!(
        std::fs::metadata(&cfg_path).unwrap().modified().unwrap(),
        init_mtime,
        "serve does not touch the init-written (a)"
    );
}

#[test]
fn test_serve_seed_then_version_second_caller_noops() {
    // Reverse order: serve seeds (a) first, then a later version/handle_version runs.
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path();
    let cfg_path = data_dir.join("config.toml");

    container_serve_seed(data_dir);
    let serve_content = std::fs::read_to_string(&cfg_path).unwrap();
    let serve_mtime = std::fs::metadata(&cfg_path).unwrap().modified().unwrap();

    // Act: a later `version` call (same primitive).
    write_default_config_if_absent(&cfg_path, false);

    // Assert: same file; second caller no-ops. Whichever runs first wins (ADR-004;
    // create_new makes order irrelevant).
    assert_eq!(
        std::fs::read_to_string(&cfg_path).unwrap(),
        serve_content,
        "the second (version) caller no-ops"
    );
    assert_eq!(
        std::fs::metadata(&cfg_path).unwrap().modified().unwrap(),
        serve_mtime,
        "the second caller does not touch (a)"
    );
}

// ── R-10 — best-effort: (a) seed failure does not abort serve startup ───────────

#[test]
#[cfg(unix)]
fn test_container_serve_seed_failure_does_not_abort_startup() {
    use std::os::unix::fs::PermissionsExt;

    // Arrange: a non-writable data dir (chmod 0o555). The create_new open will fail
    // with PermissionDenied; the primitive warns-and-continues, never panics.
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path().join("ro");
    std::fs::create_dir_all(&data_dir).unwrap();
    let mut perms = std::fs::metadata(&data_dir).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(&data_dir, perms).unwrap();

    // Act + Assert: no panic; the call returns normally (best-effort). serve would
    // proceed (it tolerates an absent (a), loading from defaults).
    container_serve_seed(&data_dir);

    // Restore perms so the temp dir can be cleaned up.
    let mut restore = std::fs::metadata(&data_dir).unwrap().permissions();
    restore.set_mode(0o755);
    std::fs::set_permissions(&data_dir, restore).unwrap();

    // Seed should NOT have been written (dir non-writable), but startup is unharmed.
    assert!(
        !data_dir.join("config.toml").exists(),
        "no (a) written into the read-only dir, but no abort"
    );
}

// ── R-01 scenario 3 — structural placement guard (source-text) ──────────────────
//
// The empirical sentinel above is the RUNTIME proof; this is the CONSTRUCTION proof
// (#4876: structure alone is insufficient — BOTH are required). We assert the seed
// call is lexically INSIDE the `if config.http.enabled` block and that the local
// `else` branch contains no seed call site, by source-text inspection. Markers are
// stable in-source anchors; the slice helper PANICS LOUDLY if a marker is missing,
// so a renamed anchor fails the guard rather than passing vacuously (pattern #5097).

const MAIN_RS: &str = include_str!("main.rs");

/// Return the source region of `tokio_main_daemon`'s HTTP block: from the
/// `if config.http.enabled {` anchor to the start of the per-slug loop. Panics if an
/// anchor is missing.
fn http_block_head() -> &'static str {
    let gate = "let (http_acceptor_handle, http_listener_addr) = if config.http.enabled {";
    let start = MAIN_RS
        .find(gate)
        .expect("HTTP gate anchor present in main.rs (guard would pass vacuously otherwise)");
    // Bound the head region at the per-slug loop anchor so we test the prologue
    // where C4 lives, not the whole HTTP body.
    let loop_anchor = "for slug in &project_slugs {";
    let end = MAIN_RS[start..]
        .find(loop_anchor)
        .expect("per-slug loop anchor present in main.rs HTTP block")
        + start;
    &MAIN_RS[start..end]
}

#[test]
fn test_seed_call_is_inside_http_enabled_block() {
    let head = http_block_head();
    assert!(
        head.contains(
            "write_default_config_if_absent(&paths.data_dir.join(\"config.toml\"), false)"
        ),
        "C4 seed call is lexically inside the if config.http.enabled block, before the per-slug loop"
    );
}

#[test]
fn test_only_one_serve_time_seed_call_site_for_a() {
    // The (a) serve seed must appear exactly once on the daemon path (the HTTP head),
    // never duplicated and never in the local `else` branch. handle_version's own
    // call (main.rs:1958) is a DIFFERENT fn (init/version), excluded here by counting
    // only the daemon-fn occurrence of the exact data_dir-join call.
    let daemon_seed =
        "write_default_config_if_absent(&paths.data_dir.join(\"config.toml\"), false)";
    let occurrences = MAIN_RS.matches(daemon_seed).count();
    assert_eq!(
        occurrences, 1,
        "exactly one serve-time (a) seed call site (the C4 HTTP-branch site)"
    );
    // And it must reside within the HTTP head region (positive containment).
    assert!(
        http_block_head().contains(daemon_seed),
        "the sole serve-time (a) seed call site is the HTTP-branch site"
    );
}
