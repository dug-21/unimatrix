# Component: project-provisioner (`build_project_server`)

**Source:** `crates/unimatrix-server/src/http_provision.rs:136` + call site `main.rs:1204`
**ADR:** ADR-002 · **FR:** FR-1…FR-4, FR-8…FR-10 · **Risks:** R-05, R-08 · **Constraint:** F1/SR-03

## Purpose

Reach full per-slug construction parity with the daemon path (`main.rs:830-994`): build the per-slug
`SessionRegistry`+`TranscriptHold`+`SignatureScanner` **triple** and `PendingEntriesAnalysis`, set them
plus the 5 config-snapshot fields on the per-slug `UnimatrixServer`. Today `build_project_server` sets
**none** of these, so `UnimatrixServer::new` test-defaults are read at runtime — the #930 split-brain +
P3 config gap, and an empty `SignatureScanner` that yields all-zero `signal_class_counts` (hollow FR-9,
AC-07 parity break — ADR-002 OQ-2). Thread 3 new params-at-end so a missing field is a **compile error
at the call site**, never a silent default (crt-056 anti-Defect-1).

## Signature — 3 new params-at-end (`http_provision.rs:136`)

```
pub async fn build_project_server(
    base_dir, slug, embed_handle, permissive, instructions,
    rayon_pool, nli_handle, nli_top_k, nli_enabled,
    inference_config, confidence_params, categories, observation_registry, boosted_categories,
    // NEW (ADR-002), params-at-end:
    store_config: &Arc<StoreConfig>,
    retention_config: &Arc<RetentionConfig>,
    signal_class_names: &Arc<Vec<String>>,
) -> Result<ProjectServerInput, ServerError>
```
Imports to add: `StoreConfig`, `RetentionConfig` (`crate::infra::config`); `TranscriptHold`,
`AuditLogPurgeSink` (`crate::infra::transcript_hold`); `SessionRegistry`, `HeldBufferScan`
(`crate::infra::session`); `SignatureScanner` (`crate::infra::transcript_activity`); `Mutex`,
`PendingEntriesAnalysis` (`crate::server`).

## Body Changes — construct + set (mirror daemon `main.rs:830-994`)

The function currently ends by building `service_layer`, then `let server = UnimatrixServer::new(...)`,
then `Ok(ProjectServerInput { slug, store, server, vector_dir })`. Change `server` to `let mut server`
and insert the P1/P3 construction + field-sets between `UnimatrixServer::new` and the `Ok(...)`.

### P1 — registry+hold+scanner TRIPLE + pending (F1/SR-03 — the load-bearing pairing; ADR-002 OQ-2)

```
// audit already built at http_provision.rs:224 as `Arc<AuditLog>` (`audit`). Reuse it.
// (1) Build this slug's TranscriptHold over the slug's audit (mirror main.rs:830-838).
//     max_sessions from the slug's resolved retention (retention_config).
transcript_hold = Arc::new(TranscriptHold::new(
    retention_config.transcript_hold_max_sessions,
    Arc::new(AuditLogPurgeSink::new(Arc::clone(audit))),
))

// (2) Build this slug's SignatureScanner from THIS slug's resolved signals (mirror main.rs:820/852).
//     ADR-002: per-slug (NOT the daemon's shared Arc<SignatureScanner>) — compiled from
//     r.transcript_signals so counts match the per-slug class NAMES P3 sets (see P3 below).
//     SignatureScanner::compile is fallible (transcript_activity.rs:172, Result<_, ScannerError>);
//     mirror the daemon's map_err into ServerError::Config (main.rs:70-72) since there is no
//     From<ScannerError> for ServerError. r.transcript_signals is already the validated output of
//     resolve_slug_config (vnc-040) — do NOT re-run transcript_signals.validate(path) here (that is
//     the daemon's file-anchored startup validate; per-slug re-validation is the wrong level).
signature_scanner = Arc::new(
    SignatureScanner::compile(&r.transcript_signals.enabled_patterns())
        .map_err(|e| ServerError::Config(e.to_string()))?   // fallible; build_project_server → Result
)

// (3) Build this slug's SessionRegistry PAIRED with the hold + scanner (mirror main.rs:846-853).
//     transcript cap from the slug's retention (the cap now actually reaches buffers, N1 gap).
//     cap + hold + scanner — the full daemon triple, not a two-of-three subset.
session_registry = Arc::new(
    SessionRegistry::with_transcript_cap(retention_config.transcript_buffer_max_bytes)
        .with_transcript_hold(Arc::clone(transcript_hold) as Arc<dyn HeldBufferScan>)
        .with_signature_scanner(Arc::clone(signature_scanner))   // OQ-2 closed (ADR-002)
)

// (4) Fresh per-slug pending accumulator (mirror main.rs:861).
pending_entries_analysis = Arc::new(Mutex::new(PendingEntriesAnalysis::new()))

// (5) Set on the server — the object the MCP read path holds (mirror main.rs:975-976, 994).
server.session_registry         = Arc::clone(session_registry)   // carries the wired scanner
server.pending_entries_analysis = Arc::clone(pending_entries_analysis)
server.transcript_hold          = Arc::clone(transcript_hold)   // PAIR — never omit (F1/SR-03)
```
The registry is the constructed unit — cap+hold+**scanner** compiled and chained in **before** the
`server.session_registry` set — so the scanner travels with the registry the tick loop clones. All
set **inside** `build_project_server`, so they land **before** the `main.rs:1229` tick-context loop
clones `input.server.session_registry` (FR-3) — no reorder needed at the call site.

### P2 — no construction change

The slug's config-driven `ServiceLayer` is already built here (`http_provision.rs:240`) and handed to
`UnimatrixServer::new(..., Some(service_layer))`. P2 is realized entirely in the resolver/handler
(`services_for` returns this layer; `ObserveContext` stops reading the global). Nothing to add here —
confirm `server.service_layer()` returns this per-slug layer for `from_server` to clone.

### P3 — set the 5 config-snapshot fields (mirror daemon `main.rs:978-990`)

```
server.observation_registry           = Arc::clone(observation_registry)   // already a param
server.inference_config               = Arc::clone(inference_config)       // already a param
server.store_config                   = Arc::clone(store_config)           // NEW param
server.retention_config               = Arc::clone(retention_config)       // NEW param
server.transcript_signal_class_names  = Arc::clone(signal_class_names)     // NEW param
```
`observation_registry` and `inference_config` are already threaded (for the ServiceLayer) — also
assign them to the server fields (they were only used for the layer before).

## Call Site Update (`main.rs:1204`)

Append the 3 resolved values from `resolve_slug_config` (`resolved`/`r`), sourcing the SAME
expressions the daemon uses (`main.rs:982,985,989`) but from the slug's `r`:

```
slug_store_config     = Arc::new(r.store.clone())                       // daemon: config.store.clone()
slug_retention_config = Arc::new(r.retention.clone())                   // daemon: config.retention.clone()
slug_signal_class_names = Arc::new(r.transcript_signals.enabled_class_names())  // daemon main.rs:989

input = build_project_server(
    base_dir, slug, &embed, permissive, instructions, &pool, &nli, nli_top_k, nli_enabled,
    &slug_inference_config, &slug_confidence_params, &slug_categories,
    &slug_observation_registry, &slug_boosted_categories,
    &slug_store_config, &slug_retention_config, &slug_signal_class_names,   // NEW
).await?
```
`slug_inference_config` already exists (`main.rs:1173`); pass it to the server field too (it is
already threaded to the ServiceLayer). Derive over the wire from `r` — **never** seed a server field
in a test (R-08); the INV-C proof drives this call.

## Data Flow

- **In:** `base_dir`, `slug`, resolved config (`r` via `resolve_slug_config`), shared handles.
- **Out:** `ProjectServerInput { slug, store, server, vector_dir }` where `server` now carries wired
  per-slug registry+hold+pending and 5 config snapshots — consumed by `from_server` (clones off it)
  and the tick loop.

## Error Handling

Existing (`ServerError::Config` on missing store, vector/registry init). `TranscriptHold::new` and
the `SessionRegistry` `with_*` builders are infallible. **New fallible step:**
`SignatureScanner::compile(&r.transcript_signals.enabled_patterns())` returns
`Result<_, ScannerError>` — map it to `ServerError::Config(e.to_string())` and propagate with `?`
(mirror daemon `main.rs:70-72`; `build_project_server` already returns `Result<_, ServerError>`, so no
new failure mode is introduced at the caller). An invalid per-slug regex now aborts **that slug's**
provision loudly rather than silently degrading to no-scanning (R-10, ADR-002). No `.unwrap()` (NFR-7).

## Key Test Scenarios (hints)

- Built slug server: `has_transcript_hold()` true; `session_registry`/`pending` are non-default
  instances (pinned by boot assertion + `Arc::ptr_eq` unit).
- **Non-zero signal counts (OQ-2 regression guard, FR-9):** a slug that declares `[transcript_signals]`
  with a matching pattern, driven a **signal-bearing** delta, yields **non-zero** `signal_class_counts`
  for the matched class in `cycle_review`'s `signal_class_counts_json` — not merely the class *names*
  with zero counts. This fails against an empty per-slug `SignatureScanner` and passes only once the
  per-slug scanner is compiled from `r.transcript_signals` and chained into the registry. Distinct from
  the empty-config edge below.
- INV-C1/C2 (R-08): register A and B with **different** declared config over the #800 fixture; assert
  `signal_class_counts` / observation categories / purge behavior reflect each slug's own config —
  derived via `resolve_slug_config` → `build_project_server`, never seeded. A's counts accumulate only
  against A's declared classes (drive a delta matching A's pattern but not B's) and vice-versa.
- Edge: slug with **empty** `[transcript_signals]` → `signal_class_counts_json == "{}"` legitimately
  (empty scanner is correct here; distinguish from the #930 default-fallback symptom, which is a
  **declared-but-zero** class set — the non-zero test above catches that).
- Edge: slug with **no** config file → `r == &config`, fields equal the daemon's own (fidelity via
  reuse) — the "declared vs not-declared" pair (RISK-TEST Edge Cases).
- Compile: omitting any of the 3 new params at `main.rs:1204` is a compile error (anti-Defect-1).
