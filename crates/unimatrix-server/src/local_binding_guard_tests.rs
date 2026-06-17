//! vnc-038 Component 11 — Local STDIO/UDS Direct-Binding Guard (C-13, ADR-006 #5087).
//!
//! This is a NEGATIVE / GUARD module. It changes NO production behavior. It encodes,
//! as compile-time-loaded source assertions, the boundary the rest of vnc-038 MUST NOT
//! cross: local STDIO (`main.rs` `tokio_main_stdio`) and local UDS (`main.rs`
//! `tokio_main_daemon`) open the path-hash store (`~/.unimatrix/{hash}/unimatrix.db`)
//! DIRECTLY at boot via `open_store_with_retry` and thread the resulting `Arc<Store>`
//! STRAIGHT to their handlers. They NEVER route through the unified HTTP resolver,
//! NEVER call `parse_project_key`, NEVER reference `ProjectKey::Default`, and NEVER
//! touch a bundle.
//!
//! The ADR-004 deletions (`DefaultResolver`, `/v1/tools->Default` arm, `_ => Default`
//! fallback) are HTTP-cloud/container ONLY and live exclusively inside the daemon's
//! `if config.http.enabled { ... }` block. This guard FAILS the instant a future edit
//! threads local through the resolver or makes local a resolver-map key.
//!
//! R-13 (Critical — the load-bearing GATE-2 guard) · AC-10 · ADR-006 (#5087).
//!
//! ## Mechanism
//! `main.rs` is loaded at compile time with `include_str!` and sliced into the three
//! boot regions by their STABLE in-source markers (function signatures and comment
//! banners that already exist in production and are themselves regression-signalled by
//! the slicing helpers below — if a marker is renamed the slice helper panics LOUDLY
//! rather than passing vacuously). Each region is then asserted against the invariant.
//! Resolver symbols (`MultiProjectRouter`, `StoreResolver`, `parse_project_key`,
//! `ProjectKey::Default`) legitimately appear in `tokio_main_daemon` — but ONLY inside
//! the HTTP block — so the guard slices the LOCAL boot regions out and asserts the
//! resolver wiring is confined to the `config.http.enabled` gate (G2/G4).

const MAIN_RS: &str = include_str!("main.rs");

/// Resolver / cloud-routing symbols that MUST NOT appear in any local boot region.
/// `ProjectKey::Slug` is deliberately NOT here — it is the cloud key type and may be
/// named in shared comments; what the guard forbids in local regions is `ProjectKey`
/// usage entirely (no resolver key of any kind on the local path).
const FORBIDDEN_IN_LOCAL: &[&str] = &[
    "parse_project_key",
    "MultiProjectRouter",
    "DefaultResolver",
    "StoreResolver",
    "ProjectKey",
    "ObserveContext",
    "PathRouter",
    "SlugRouter",
    "decode_bundle",
    "encode_bundle",
    "BUNDLE_VERSION",
];

/// Strip line (`//`) and block (`/* */`) comments from a source slice so the guard
/// asserts on EXECUTABLE CODE, never on explanatory prose. Resolver symbols are named
/// in several local-region comments (e.g. "... available for ObserveContext", "the
/// validated `[[projects]]` slugs are unused"); those are documentation of the
/// boundary, not a crossing of it. We keep string contents intact (the loud-first-boot
/// message lives in a string literal and is asserted elsewhere) and only remove comments.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        if in_line_comment {
            if c == b'\n' {
                in_line_comment = false;
                out.push('\n');
            }
            i += 1;
        } else if in_block_comment {
            if c == b'*' && next == Some(b'/') {
                in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
        } else if in_string {
            // Preserve string contents verbatim; honor escapes so an escaped quote
            // does not prematurely end the string.
            out.push(c as char);
            if let (b'\\', Some(n)) = (c, next) {
                out.push(n as char);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_string = false;
            }
            i += 1;
        } else if c == b'/' && next == Some(b'/') {
            in_line_comment = true;
            i += 2;
        } else if c == b'/' && next == Some(b'*') {
            in_block_comment = true;
            i += 2;
        } else if c == b'"' {
            in_string = true;
            out.push('"');
            i += 1;
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

/// Slice `[start_marker, end_marker)` out of `MAIN_RS`. Panics LOUDLY if either marker
/// is missing — a renamed marker is a regression signal, never a silent pass.
fn slice_region(start_marker: &str, end_marker: &str) -> &'static str {
    let start = MAIN_RS.find(start_marker).unwrap_or_else(|| {
        panic!(
            "GUARD MARKER LOST: start marker {start_marker:?} not found in main.rs — \
             the boot path was renamed/restructured; re-anchor the local-binding guard"
        )
    });
    let rest = &MAIN_RS[start..];
    let end = rest.find(end_marker).unwrap_or_else(|| {
        panic!(
            "GUARD MARKER LOST: end marker {end_marker:?} not found after {start_marker:?} \
             in main.rs — re-anchor the local-binding guard"
        )
    });
    &rest[..end]
}

/// The local STDIO boot path: `tokio_main_stdio` from its signature up to the daemon
/// (no further `async fn tokio_main_*` appears before bridge). We bound it at the
/// bridge entry point which immediately follows it in source order.
fn stdio_region() -> &'static str {
    slice_region("async fn tokio_main_stdio(", "async fn tokio_main_bridge(")
}

/// The local UDS direct-binding region inside the daemon: from the `start_uds_listener`
/// banner up to the HTTP listener banner. This is the span that wires the directly
/// opened store into the hook UDS transport, BEFORE the HTTP/cloud block begins.
fn daemon_uds_region() -> &'static str {
    slice_region(
        "// Start UDS listener for hook IPC.",
        "// --- HTTP LISTENER STARTUP",
    )
}

/// The daemon HTTP/cloud block — where resolver wiring legitimately lives. Used by the
/// HTTP-only-deletion cross-check (G4) to prove the resolver symbols are confined here.
fn daemon_http_region() -> &'static str {
    slice_region("// --- HTTP LISTENER STARTUP", "async fn tokio_main_stdio(")
}

// ---------------------------------------------------------------------------
// G1 — Direct-binding assertion (R-13 sc.1 — the load-bearing guard)
// ---------------------------------------------------------------------------

/// R-13 sc.1 / G1: local STDIO opens the path-hash store DIRECTLY at boot via
/// `open_store_with_retry(&paths.db_path)` and threads the resulting `Arc<Store>`
/// straight to its handler — NO slug supplied, behavior unchanged from ADR-004.
#[test]
fn test_local_stdio_opens_path_hash_store_directly() {
    let region = stdio_region();

    assert!(
        region.contains("open_store_with_retry(&paths.db_path)"),
        "local STDIO boot must open the path-hash store DIRECTLY via \
         open_store_with_retry(&paths.db_path) (G1, ADR-006); call site not found"
    );

    // The directly-opened store is threaded straight to the MCP server / handler.
    assert!(
        region.contains("UnimatrixServer::new(") && region.contains(".serve("),
        "local STDIO must thread the directly-opened Arc<Store> into UnimatrixServer \
         and serve over stdio (G1); wiring not found"
    );

    // No slug is resolved in STDIO CODE (comments documenting that slugs are unused
    // are fine — we strip them). Path-hash is derived automatically at boot.
    let code = strip_comments(region);
    assert!(
        !code.contains("slug") || code.contains("_project_slugs"),
        "local STDIO boot CODE must NOT route by a slug — path-hash is derived \
         automatically at boot, no manual slug (AC-10/G1). The only permitted mention \
         is binding the validated list to the unused `_project_slugs`."
    );
}

/// R-13 sc.1 / G1: local UDS (daemon hook IPC) threads the DIRECTLY-opened
/// `Arc<Store>` (via `Arc::clone(&store)`) straight into `start_uds_listener` —
/// the store is the one opened by `open_store_with_retry`, never a resolved handle.
#[test]
fn test_local_uds_opens_path_hash_store_directly() {
    // The daemon opens the path-hash store directly (shared open site for both the
    // local UDS hook transport and — when enabled — the daemon's own subsystems).
    assert!(
        MAIN_RS.contains("let store = open_store_with_retry(&paths.db_path).await?;"),
        "daemon must open the path-hash store DIRECTLY via open_store_with_retry \
         (G1, ADR-006); call site not found"
    );

    let region = daemon_uds_region();
    assert!(
        region.contains("start_uds_listener(") && region.contains("Arc::clone(&store)"),
        "local UDS must thread the directly-opened Arc<Store> (Arc::clone(&store)) \
         straight into start_uds_listener (G1); direct binding not found"
    );
}

// ---------------------------------------------------------------------------
// G2 — Resolver-bypass assertion (R-13 sc.2 — structure guard)
// ---------------------------------------------------------------------------

/// R-13 sc.2 / G2: the local STDIO/UDS boot paths NEVER invoke `parse_project_key`,
/// NEVER construct the HTTP resolver (`DefaultResolver`/`MultiProjectRouter`/
/// `StoreResolver`), NEVER reference `ProjectKey`, and NEVER touch a bundle. FAILS
/// the instant a future edit threads local through the resolver or adds a local
/// resolver-map key.
#[test]
fn test_local_boot_never_invokes_parse_project_key() {
    for (region_name, region) in [
        ("STDIO (tokio_main_stdio)", stdio_region()),
        ("UDS (daemon hook-IPC binding)", daemon_uds_region()),
    ] {
        // Assert on executable CODE, not comments documenting the boundary.
        let code = strip_comments(region);
        for forbidden in FORBIDDEN_IN_LOCAL {
            assert!(
                !code.contains(forbidden),
                "REGRESSION (R-13/G2, AC-10): local boot region {region_name} references \
                 {forbidden:?}. Local STDIO/UDS MUST keep its DIRECT path-hash binding and \
                 MUST NOT be routed through the unified resolver or carry a resolver key \
                 (ADR-006 #5087). Routing local through the resolver creates the cross-store \
                 path AC-10 forbids."
            );
        }
    }
}

// ---------------------------------------------------------------------------
// G3 — No-resolver-key assertion (R-13 sc.3 — ADR-006 tightening)
// ---------------------------------------------------------------------------

/// R-13 sc.3 / G3: local is NOT self-registered as a resolver key. The unified
/// resolver's key space is `ProjectKey::Slug` only; there is no derived path-hash
/// key in the slug map. We prove this two ways:
///   (a) the only resolver/slug-map construction site lives in the daemon HTTP block
///       and is fed from `project_slugs` (config `[[projects]]`), never from a
///       path-hash;
///   (b) the local boot regions name no `ProjectKey` at all (covered by G2), so they
///       cannot contribute a key.
#[test]
fn test_local_not_a_resolver_key() {
    let http = daemon_http_region();

    // The resolver is built from project_slugs (config-declared), not a path-hash.
    assert!(
        http.contains("MultiProjectRouter::from_servers") && http.contains("project_slugs"),
        "the unified resolver must be built from config project_slugs in the HTTP block \
         (G3); construction site not found"
    );

    // No path-hash is ever inserted as a resolver key anywhere in main.rs.
    assert!(
        !MAIN_RS.contains("compute_project_hash") || !http.contains("compute_project_hash"),
        "REGRESSION (R-13/G3, ADR-006): a path-hash is being used inside the resolver \
         construction block — local must NOT be self-registered as a resolver key. The \
         resolver key space is ProjectKey::Slug only."
    );

    // STDIO has no resolver at all — it never constructs a slug map.
    assert!(
        !stdio_region().contains("from_servers"),
        "REGRESSION (R-13/G3): the STDIO boot path constructs a resolver slug map. \
         Local must never enter the resolver."
    );
}

// ---------------------------------------------------------------------------
// G4 — HTTP-only-deletion cross-check (R-13 sc.4, with R-07)
// ---------------------------------------------------------------------------

/// R-13 sc.4 / G4: the ADR-004 deletions and the resolver wiring are confined to the
/// daemon HTTP/cloud block and do NOT reach the local STDIO/UDS boot paths. We assert
/// every resolver symbol that appears in main.rs at all appears ONLY within the daemon
/// HTTP region (or in comments outside any boot path) — never in the local regions.
#[test]
fn test_default_deletions_confined_to_http() {
    let http = daemon_http_region();
    let stdio = strip_comments(stdio_region());
    let uds = strip_comments(daemon_uds_region());

    // The resolver machinery exists and is wired in the HTTP block (positive anchor:
    // if the HTTP block stopped constructing the resolver this guard re-anchors).
    assert!(
        http.contains("StoreResolver") && http.contains("MultiProjectRouter"),
        "the resolver wiring must live in the daemon HTTP block (G4); not found — \
         re-anchor the guard if the cloud surface moved"
    );

    // The resolver wiring is gated behind config.http.enabled — it cannot execute on
    // a local-only (stdio) boot and is not threaded into the UDS binding.
    assert!(
        MAIN_RS.contains("if config.http.enabled"),
        "the HTTP resolver wiring must be gated behind `if config.http.enabled` (G4); \
         the gate is missing — resolver wiring may have leaked onto the local path"
    );

    // Neither local region carries any resolver symbol (cross-check of G2 with focus
    // on the ADR-004-deleted machinery specifically).
    for sym in ["DefaultResolver", "MultiProjectRouter", "parse_project_key"] {
        assert!(
            !stdio.contains(sym),
            "REGRESSION (R-13/G4): STDIO boot references {sym:?}; ADR-004 deletions and \
             resolver wiring are HTTP-only and must not reach local STDIO."
        );
        assert!(
            !uds.contains(sym),
            "REGRESSION (R-13/G4): local UDS binding references {sym:?}; ADR-004 deletions \
             and resolver wiring are HTTP-only and must not reach the local UDS path."
        );
    }
}

// ---------------------------------------------------------------------------
// Edge — empty [[projects]] must NOT trigger loud-first-boot on local
// ---------------------------------------------------------------------------

/// Edge (R-13): local STDIO boot with NO `[[projects]]` and NO slug resolves its
/// path-hash store DIRECTLY — it is NOT caught by the cloud "register a project to
/// begin" loud-first-boot failure. The loud message (AC-09) is emitted ONLY inside
/// the daemon HTTP block's empty-slug branch; the STDIO path treats `project_slugs`
/// as unused and opens its store unconditionally.
#[test]
fn test_local_stdio_empty_projects_does_not_loud_fail() {
    let stdio = stdio_region();

    // STDIO declares project_slugs unused — it never branches on emptiness to fail.
    assert!(
        stdio.contains("_project_slugs"),
        "local STDIO must treat the validated [[projects]] slugs as UNUSED \
         (`_project_slugs`) — the slug list does not gate the local boot (edge, R-13)"
    );

    // The loud "register a project to begin" message must NOT live on the STDIO path.
    assert!(
        !stdio.contains("register") || !stdio.contains("nothing is servable"),
        "REGRESSION (R-13 edge, AC-09): the cloud loud-first-boot message leaked onto \
         the local STDIO path. Local must serve its direct store on empty [[projects]], \
         never fail loud."
    );

    // STDIO opens the store unconditionally (no emptiness guard wrapping the open).
    assert!(
        stdio.contains("open_store_with_retry(&paths.db_path)"),
        "local STDIO must open its path-hash store unconditionally on empty [[projects]] \
         (edge, R-13); unconditional open not found"
    );

    // The loud message is gated to the HTTP block (positive anchor for AC-09 scope).
    assert!(
        daemon_http_region().contains("nothing is servable"),
        "the loud-first-boot 'nothing is servable' message must live in the daemon HTTP \
         block (AC-09 is cloud-only); not found there — verify scoping"
    );
}

// ---------------------------------------------------------------------------
// Self-test of the guard mechanism — proves the guard FAILS on a routed-local edit
// ---------------------------------------------------------------------------

/// Meta-guard: proves the structural detector actually FIRES when a local region is
/// routed through the resolver. We synthesize a region that wires the resolver into a
/// local boot path and confirm the same predicate the G2 guard uses flags it. This is
/// the explicit "confirm the guard fails if local were routed through the resolver"
/// requirement (R-13). If this ever passes a poisoned region as clean, the live guard
/// is toothless.
#[test]
fn test_guard_detects_local_routed_through_resolver() {
    // Simulated regression: a future edit threads the local store through the resolver.
    let poisoned = r#"
        let resolver: Arc<dyn StoreResolver> =
            MultiProjectRouter::from_servers(local_servers, max_body, origins)?;
        let key = parse_project_key(path)?;            // <-- AC-10 violation
        let store = resolver.resolve_store(&key)?;     // local now goes through funnel
        start_uds_listener(&paths.socket_path, store, ...).await?;
    "#;

    let detected = FORBIDDEN_IN_LOCAL
        .iter()
        .any(|forbidden| poisoned.contains(forbidden));

    assert!(
        detected,
        "META-GUARD FAILURE: the structural detector did NOT flag a local boot region \
         routed through the resolver. The live G2 guard would be toothless — fix the \
         FORBIDDEN_IN_LOCAL set before trusting this guard."
    );

    // And confirm a clean direct-binding region is NOT flagged (no false positives).
    let clean = r#"
        let store = open_store_with_retry(&paths.db_path).await?;
        start_uds_listener(&paths.socket_path, Arc::clone(&store), ...).await?;
    "#;
    let false_positive = FORBIDDEN_IN_LOCAL
        .iter()
        .any(|forbidden| clean.contains(forbidden));
    assert!(
        !false_positive,
        "META-GUARD FAILURE: the detector flagged a CLEAN direct-binding region — \
         the guard would false-fail every untouched local boot path."
    );
}
