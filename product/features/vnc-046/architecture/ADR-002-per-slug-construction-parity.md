## ADR-002: Full Per-Slug Construction Parity in `build_project_server` — P1 (registry+hold+scanner triple, pending), P2 (services), P3 (5 config-snapshots), Delete 2 Vestigial Fields

### Context

`UnimatrixServer::new` (`server.rs:403-446`) mints **test-only defaults** for a set of fields;
each doc-comment says "overwritten in main.rs daemon/stdio paths." The daemon (`main.rs:975-994`)
and stdio (`main.rs:1694-1709`) paths overwrite ~8 of them. `build_project_server`
(`http_provision.rs:136-283`) overwrites **none** — it threads config only into the per-slug
`ServiceLayer`. So every per-slug cloud server silently reads: an empty `SessionRegistry` and an
unshared default `TranscriptHold` (#930), a default `PendingEntriesAnalysis`, builtin domain
packs, and default byte-limit/inference/retention/empty-signal-class-names (the empty
`transcript_signal_class_names` also makes `cycle_review`'s `signal_class_counts_json == "{}"`).

The audit split the 9 items into 3 fix patterns. This ADR ratifies constructing all of them
per-slug, at the seam, in one pass — refusing the inconsistent half-isolation of fixing the
registry while leaving services and config global (SR-04, uni-zero "last mile"). Two constraints
are load-bearing:

- **F1 / SR-03:** `transcript_hold` must be a constructed **pair** with `session_registry`.
  Registry-alone splits the purge gate (`purge_held_for_feature` / `sweep_expired` act on the
  default hold while drains route through the shared store) → held buffers never purge →
  unbounded memory growth. They move together, and land before the tick-context loop.
- **crt-056 anti-Defect-1:** config parity is threaded as **required params at the end** of
  `build_project_server`, so a missing field is a **compile error at the call site**, never a
  silent test-default fallback.

crt-056 already builds a config-correct per-slug `ServiceLayer` inside `build_project_server`
(verified: `inference_config`, `observation_registry`, `categories`, `boosted_categories`,
`nli_*` all threaded). So P2 is genuinely mechanical — the slug's `ServiceLayer` exists; only
`ObserveContext` was reading the global one (fixed in ADR-001). The SR-assumption that
crt-056's per-slug ServiceLayer is config-driven is **confirmed against code**.

### Decision

In `build_project_server`, reach full construction parity with the daemon path:

- **P1 — construct per-slug + set on the server (registry+hold+scanner triple + pending):**
  build the slug's `SessionRegistry` with its per-slug transcript cap (from the slug's
  `[retention]`, vnc-040 — the cap now actually reaches buffers), wire its own `TranscriptHold`
  (slug's `AuditLogPurgeSink` over the slug's `audit`), **wire its own per-slug
  `SignatureScanner`** (below), and a fresh `PendingEntriesAnalysis`. Set
  `server.session_registry`, `server.transcript_hold`, `server.pending_entries_analysis`. The
  registry+hold are one unit (F1); ordering is inside `build_project_server`, so they are set
  **before** the `main.rs:1229` tick loop clones them (SR-03). This mirrors the daemon registry
  build at `main.rs:846-853` in full — cap **and** hold **and** scanner — not a two-of-three
  subset.

- **P1 signature scanner (per-slug, fallible) — closes OQ-2:** the daemon compiles a
  `SignatureScanner` once (`main.rs:820`, via `build_signature_scanner`) and chains it into the
  registry with `.with_signature_scanner(Arc::clone(&signature_scanner))` (`main.rs:852`). Without
  this, `SessionRegistry` falls back to `SignatureScanner::empty()` (`session.rs:305`), whose
  `RegexSet` matches nothing — so `class_counts` accumulated during delta apply (`session.rs:350`)
  stay all-zero. A per-slug server that declared `[transcript_signals]` would then emit class
  **names** (from P3's `signal_class_names`) with all-**zero** counts in
  `signal_class_counts_json` (hollow FR-9), and diverge from the UDS/daemon path on any
  signal-bearing transcript (AC-07 parity break, R-16). `build_project_server` therefore builds
  the scanner per-slug from the slug's already-resolved `r.transcript_signals` and chains it into
  the registry:

  ```rust
  // mirror main.rs:846-853; scanner compiled from THIS slug's resolved signals
  SessionRegistry::with_transcript_cap(retention_config.transcript_buffer_max_bytes)
      .with_transcript_hold(Arc::clone(&transcript_hold) as Arc<dyn HeldBufferScan>)
      .with_signature_scanner(Arc::new(
          SignatureScanner::compile(&r.transcript_signals.enabled_patterns())?,
      ))
  ```

  `SignatureScanner::compile` is fallible (`transcript_activity.rs:172`, `Result<_, ScannerError>`)
  → propagate with `?`; `build_project_server` already returns `Result`, so no new failure mode
  is introduced. The daemon additionally calls `transcript_signals.validate(path)` inside
  `build_signature_scanner` (`main.rs:66-69`); the per-slug path does **not** re-validate here
  because `r.transcript_signals` is already the validated output of `resolve_slug_config`
  (vnc-040), so `compile` on the resolved, enabled patterns is the correct per-slug level — no
  second file-anchored validate.

  **Per-slug vs operator-global signatures — DECIDED per-slug.** The scanner is compiled from
  `r.transcript_signals`, **not** shared as the daemon's global `Arc<SignatureScanner>`.
  Rationale: (1) `[transcript_signals]` is per-slug-overlayable config, resolved per slug by
  `resolve_slug_config` (vnc-040 ADR-004, the canonical per-slug-vs-global key classifier,
  #5217) — the same funnel that yields `r`. (2) The class **names** side is already per-slug:
  P3 threads `signal_class_names = r.transcript_signals.enabled_class_names()` onto
  `server.transcript_signal_class_names`. A **global** scanner would count against a different
  class set than the per-slug names the JSON reports — re-creating exactly the names-vs-counts
  split-brain #930 exists to close. Per-slug names therefore *require* a per-slug scanner
  compiled from the same `r.transcript_signals`; they are one unit. `embed_service` (one ONNX
  model) is correctly-global because it is not config-derived per slug — the scanner is,
  so the two are classified differently and deliberately.
- **P2 — no server change; `ObserveContext` stops reading the global `ServiceLayer`** (ADR-001
  `services_for` returns the slug's already-built layer). The slug's `ServiceLayer` is the same
  one its `UnimatrixServer` holds → observe-path briefing/search/compact read the slug's store.
- **P3 — set the 5 config-snapshot server fields** (mirror `main.rs:978-990`):
  `observation_registry` and `inference_config` are **already available** as params (threaded
  for the ServiceLayer) — also assign them to the server fields. Thread **3 new params**
  (params-at-end): `store_config: &Arc<StoreConfig>`, `retention_config: &Arc<RetentionConfig>`,
  `signal_class_names: &Arc<Vec<String>>` (from the slug's resolved config via
  `resolve_slug_config`, vnc-040), and set `server.store_config`, `server.retention_config`,
  `server.transcript_signal_class_names`.
- **Delete vestigial fields (AC-09):** remove `ObserveContext.vector_store` and
  `ObserveContext.adapt_service` (dormant `_`-prefixed split-brains) in the same pass — else
  they become the next split-brain when a caller starts reading them.

`ProjectEntry::from_server` `Arc::clone`s the per-slug registry/pending/services off the
assembled `server` into the entry (ADR-001), so the resolver's `*_for` methods return the same
instances the server holds — convergence by construction, pinned by ADR-003.

Correctly-global handles stay global: `embed_service` (one loaded ONNX model, crt-056 C-3).
`categories` is **per-slug config-driven** (`slug_categories` from `r.knowledge.categories`,
threaded at `main.rs:1183` → `http_provision.rs`, crt-031/vnc-040) — a config-snapshot field,
classified PER-SLUG in ADR-003's census; NFR-5's "operator allowlist / global" prose is stale.
`client_type_map` stays correctly-per-instance (runtime-populated by `initialize`). NG-4:
UDS/stdio keep the daemon-global registry.

### Consequences

- **Easier:** every observe/MCP/tick read on a per-slug cloud server sees real per-slug data —
  #930 (transcript), P2 (knowledge-read isolation), and P3 (config parity, INV-C1/C2) all close
  at the same seam. Per-slug `[retention]` overlays finally apply to transcript buffers (N1 gap
  closed). `signal_class_counts_json` reflects the slug's declared signal classes.
- **Easier:** the config-params-at-end shape means a future per-slug config field is a compile
  error until threaded (crt-056 anti-Defect-1), reinforcing ADR-003's class guard.
- **Harder:** `build_project_server` grows 3 params and its single caller (`main.rs:1204`) must
  pass the slug's resolved `store_config` / `retention_config` / `signal_class_names`
  (available from `resolve_slug_config`). One more construction block per slug (bounded). The
  per-slug `SignatureScanner::compile` adds one fallible step (`?`) to the registry build — an
  invalid per-slug regex now aborts that slug's provision loudly rather than degrading to
  silent no-scanning (consistent with the daemon's R-10 no-silent-fallback stance).
- **Risk (SR-04):** if P3 is cut for speed, P1+P2 are the floor; the config fields ship reading
  global/builtin defaults with a PR risk note, and ADR-003's boot assertion still fires on the
  unwired config fields so the gap is loud, not silent. This architecture takes P3 **in-scope**.

Related: #5629 (construction parity), vnc-040 #5209 (`resolve_slug_config`, the config source),
crt-056 (per-slug ServiceLayer), design-reviewer F1 / SR-03 (registry+hold pairing), ADR-001
(the funnel that consumes these instances), ADR-003 (the boot guard).
