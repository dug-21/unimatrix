//! vnc-046 Wave 4 (ADR-003, FR-13, AC-08) Guard 2: compile-time field census.
//!
//! Destructures [`UnimatrixServer`] with an **exhaustive** pattern and **no `..`
//! rest**, so ADDING a field to the struct is a COMPILE ERROR here until the author
//! classifies it PER-SLUG / CORRECTLY-GLOBAL / CORRECTLY-PER-INSTANCE. This closes
//! the whole "constructor-default never overwritten" bug class (SR-02): a future
//! field cannot ship unclassified, and a PER-SLUG classification is the reminder to
//! wire it into the runtime boot assertion (`assert_per_slug_isolation`, main.rs
//! Guard 1).
//!
//! This is a REAL compile-time guard in EVERY build (deliberately NOT `#[cfg(test)]`),
//! so the shipped release binary carries the class closure — the complement to the
//! runtime boot assertion. The census function is never called: the exhaustive
//! destructure is checked by the compiler regardless. Rust has no runtime field
//! reflection, so "assert every field was overwritten" is impossible generically at
//! runtime; the class is closed HERE at compile time by forcing classification.
//!
//! Guard 2 is a COMPLEMENT to the behavioral suite (ADR-004), never a substitute:
//! the census proves a field is *classified* (and PER-SLUG fields are routed into
//! Guard 1), but it is blind to whether the resolved per-slug handle is actually
//! *used* on the write path (#5427) — the behavioral back-stop enforces that.

use super::UnimatrixServer;

/// Compile-time census. NEVER called — the exhaustive destructure (no `..`) is the
/// guard. A new `UnimatrixServer` field breaks compilation HERE until the author
/// adds it to one of the three classification groups below.
///
/// **REVIEW-ENFORCED (no trybuild harness in this repo, test-plan OQ-3):** do NOT
/// "fix" a compile error here by adding `..` — that silently re-opens the whole
/// class. Classify the new field instead. A PER-SLUG classification additionally
/// requires wiring the field into `assert_per_slug_isolation` (main.rs Guard 1).
#[allow(dead_code)]
fn field_census(server: UnimatrixServer) {
    let UnimatrixServer {
        // ---- PER-SLUG: per-slug store / subsystems, constructor-wired ----
        entry_store,
        vector_store,
        registry,
        audit,
        store,
        vector_index,
        usage_dedup,
        adapt_service,
        services,
        effectiveness_state,
        // ---- PER-SLUG: vnc-046 new wiring, boot-asserted by Guard 1 ----
        session_registry,
        transcript_hold,
        pending_entries_analysis,
        observation_registry,
        inference_config,
        store_config,
        retention_config,
        transcript_signal_class_names,
        // ---- PER-SLUG: config-snapshot, per-slug config-driven (ADR-003 OQ-3) ----
        // `categories` is threaded per-slug as `slug_categories` in shipped code
        // (main.rs:1183 → build_project_server); NFR-5's "global operator allowlist"
        // prose is STALE relative to the code (ADR-003 correction). It is a
        // config-snapshot field (set at the constructor from the threaded param,
        // like store_config/retention_config), so it needs no Arc::ptr_eq handle
        // convergence in Guard 1 — covered by this census + the AC-06 exception.
        categories,
        // ---- CORRECTLY-GLOBAL: one shared ONNX model across every slug ----
        embed_service,
        // ---- CORRECTLY-PER-INSTANCE: per server instance, never per-slug-shared ----
        tick_metadata,
        tool_router,
        server_info,
        client_type_map,
        // NO `..` — a NEW field is a COMPILE ERROR until classified above (SR-02).
    } = server;

    // Route every binding to its classification group. Consuming each binding here
    // (a) documents the isolation class of every field as compiler-enforced living
    // documentation, and (b) leaves no unused-variable warning.
    let _per_slug = (
        entry_store,
        vector_store,
        registry,
        audit,
        store,
        vector_index,
        usage_dedup,
        adapt_service,
        services,
        effectiveness_state,
        session_registry,
        transcript_hold,
        pending_entries_analysis,
        observation_registry,
        inference_config,
        store_config,
        retention_config,
        transcript_signal_class_names,
        categories,
    );
    let _correctly_global = (embed_service,);
    let _correctly_per_instance = (tick_metadata, tool_router, server_info, client_type_map);
}
