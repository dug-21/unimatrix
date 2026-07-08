# Component: boot-assertion (`assert_per_slug_isolation` + field census)

**Source:** `crates/unimatrix-server/src/main.rs` (extends `assert_wave_b_precondition:81`);
census destructures `UnimatrixServer` (`server.rs:197-289`)
**ADR:** ADR-003 · **FR:** FR-13 · **AC:** AC-08 · **Risks:** R-03, R-05, R-07
**NFR:** NFR-2 (real runtime check, NOT `debug_assert` — compiled out of release)

## Purpose

Two complementary white-box guards, both mandatory, neither a `debug_assert`:
1. **Runtime boot assertion** per built slug — `Arc::ptr_eq` convergence + `has_transcript_hold`
   pairing + P3 sentinels + global-handle checks; `Err` aborts boot loud.
2. **Compile-time field census** — exhaustive `UnimatrixServer` destructure (no `..`) so a future
   field is a compile error until classified.

These are complements to the behavioral suite (ADR-004), never substitutes.

## Guard 1 — `assert_per_slug_isolation`

### Sequencing (OPEN QUESTION 1 — read first)

ADR-003 signature is `assert_per_slug_isolation(input: &ProjectServerInput, resolver: &dyn StoreResolver,
config: &UnimatrixConfig) -> Result<(), ServerError>`. But `MultiProjectRouter::from_servers` **consumes**
`Vec<ProjectServerInput>`, and the resolver does not exist until after that consume — while the
`Arc::ptr_eq` check needs both the resolver-returned handle and the server-held handle at once.

**Recommended flow** (flag for architect sign-off — refines the param type, not behavior):
```
// In the existing pre-move loop (main.rs:1229) that already clones input.server.session_registry
// for tick_contexts, ALSO capture a probe per slug:
probes: Vec<IsolationProbe> = for input in &slug_servers:
    IsolationProbe {
        slug:             input.slug.clone(),
        session_registry: Arc::clone(input.server.session_registry),
        pending:          Arc::clone(input.server.pending_entries_analysis),
        services:         input.server.service_layer(),
        has_hold:         input.server.session_registry.has_transcript_hold(),
        // P3 sentinels captured off input.server before the move:
        signal_class_names: Arc::clone(input.server.transcript_signal_class_names),  // via accessor/pub
        // (retention/store snapshots as needed for sentinel checks)
    }

router = MultiProjectRouter::from_servers(slug_servers, ...)   // consumes inputs

// After the router exists, assert per slug against the probe + the built resolver:
for probe in &probes:
    assert_per_slug_isolation(probe, &*router, &config)?       // Err aborts boot
```
`IsolationProbe` is a small local struct in `main.rs` (Arc clones only). This replaces the literal
`input: &ProjectServerInput` param with `probe: &IsolationProbe`. If the architect prefers the ADR
signature verbatim, the alternative is to expose an accessor on `MultiProjectRouter` returning the
entry's server-side handle — heavier; the probe is preferred.

### Assertion body

```
FUNCTION assert_per_slug_isolation(probe, resolver, config) -> Result<(), ServerError>:
    key = ProjectKey::Slug(probe.slug.clone())

    // P1 convergence — resolver returns the SAME instance the server holds (ADR-003).
    reg = resolver.registry_for(&key).map_err(|_| ServerError::Config(
        "slug {slug}: registry_for failed at boot — wiring contradiction"))?
    IF NOT Arc::ptr_eq(&reg, &probe.session_registry):
        RETURN Err(ServerError::Config("slug {slug}: session_registry not converged (write≠read instance)"))
    pend = resolver.pending_for(&key).map_err(...)?
    IF NOT Arc::ptr_eq(&pend, &probe.pending):
        RETURN Err(ServerError::Config("slug {slug}: pending_entries_analysis not converged"))

    // P1 pairing (F1/SR-03) — registry carries a wired hold, purge gate cannot split.
    IF NOT probe.has_hold:
        RETURN Err(ServerError::Config("slug {slug}: transcript_hold not wired (purge gate would split)"))

    // P2 convergence — services_for resolves the slug's layer (reachable = wired).
    resolver.services_for(&key).map_err(|_| ServerError::Config("slug {slug}: services_for failed"))?

    // P3 non-default sentinels (where checkable) — config fields are the slug's resolved values,
    // not UnimatrixServer::new defaults. E.g. when the slug declares signals, class names non-empty:
    IF slug_declares_signals(config, probe.slug) AND probe.signal_class_names.is_empty():
        RETURN Err(ServerError::Config("slug {slug}: transcript_signal_class_names empty despite declared signals"))
    //   store_config / inference_config lack a clean sentinel → covered by guard 2 + wiring-pin unit
    //   (documented AC-06 exception; R-04).

    // Correctly-global — embed_service is the ONE shared model across every slug (catch accidental reload).
    //   Assert Arc::ptr_eq(resolver-side embed / probe embed, the daemon's single embed_handle) if surfaced.

    RETURN Ok(())
```
Return `Result<(), ServerError>` and `?`-propagate at the call site so any failure aborts daemon boot
(exactly like `assert_wave_b_precondition`, `main.rs:858`). Not a `debug_assert` (NFR-2/SR-06).

`assert_wave_b_precondition` (`main.rs:81`) stays for the daemon/stdio global registry; it can be
called by `assert_per_slug_isolation` for the hold+retention re-check, or its logic inlined into the
pairing clause. Generalize, do not duplicate.

## Guard 2 — Compile-time field census (exhaustive, no `..`)

A helper (or `#[cfg(test)]` fn) destructures `UnimatrixServer` fully; adding a field breaks
compilation until classified. Enumerate ALL fields from `server.rs:197-289`:

```
let UnimatrixServer {
    // PER-SLUG (constructor-wired, per-slug store/subsystems):
    entry_store, vector_store, registry, audit, store, vector_index, usage_dedup,
    adapt_service, services, effectiveness_state,
    // PER-SLUG (NEW wiring, boot-asserted by guard 1):
    session_registry, transcript_hold, pending_entries_analysis,
    observation_registry, inference_config, store_config, retention_config,
    transcript_signal_class_names,
    // CORRECTLY-GLOBAL:
    embed_service,
    // CORRECTLY-PER-INSTANCE:
    tick_metadata, tool_router, server_info, client_type_map,
    // `categories`: SEE OPEN QUESTION 3 (OVERVIEW) — classify consistent with the code
    //   (per-slug config-driven `slug_categories` today), not with NFR-5's "global" prose.
    categories,
    // NO `..` — a new field forces a compile error here until the author classifies it (SR-02).
} = server;
```
Route each binding to its class (e.g. `let _per_slug = (session_registry, ...); let _global =
(embed_service,); let _per_instance = (tool_router, ...);` or an explicit match arm per field). A
new field cannot ship unclassified; a PER-SLUG classification is a reminder to wire it into guard 1.

## Data Flow

- **In:** built per-slug servers (via probes), the built resolver, the daemon `config`.
- **Out:** `Ok(())` (boot proceeds) or `Err(ServerError)` (boot aborts, naming the slug + field).

## Error Handling

Every failure → `Err(ServerError::Config(...))` naming the slug and the unwired field. Aborts boot
loud (AC-08). No panic, no `debug_assert`, no silent read-zero at review time.

## Key Test Scenarios (hints)

- Build a slug server with `session_registry` left as the constructor default → boot assertion `Err`
  (R-03 §1). Same for an unpaired hold (R-05 §1) and a default P3 sentinel.
- Wiring-pin unit: `Arc::ptr_eq(resolver.registry_for(&slug)?, slug_server.session_registry)` and
  same for `pending` — per built slug (R-03 §2, R-06 §2). Covers `store_config`/`inference_config`
  value-pins (documented AC-06 exception, R-04).
- Census compile-fail: add a throwaway field to `UnimatrixServer` → census fails to compile (R-07 §1).
- Ordering: the pair is set before the `main.rs:1229` tick clone → per-slug `PerSlugTickContext` reads
  the wired instance, not the default (R-05 §2, FR-3).
