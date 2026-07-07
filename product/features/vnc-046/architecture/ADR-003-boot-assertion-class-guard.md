## ADR-003: A Real Boot Assertion Plus Exhaustive Field Census Guard the Whole "Constructor-Default Never Overwritten" Class — Not a `debug_assert`, Not Only the 9 Known Fields

### Context

The known per-slug field set grew **2 → 9** during the #930 audit ("the more you look, the more
you find"). SR-02 warns that latent fields outside the 9 (e.g. per-slug `ExtractionContext` /
neural enhancer #5170, `client_type_map`) or a *future* field added to `UnimatrixServer` could
ship still-global, recreating the split-brain on a new field. A guard that only checks the 9
enumerated fields does not close the class.

Two failure modes constrain the guard:

- **SR-06:** a `debug_assert!` is compiled out of release → **zero coverage** on the shipped
  cloud binary. The guard must be a **real runtime boot check that returns an error** and aborts
  boot, exactly like the existing `assert_wave_b_precondition` (`main.rs:81`, returns
  `Result<(), ServerError>`), which today only guards the *global* registry.
- **The class problem:** Rust has no runtime field reflection, so "assert every field was
  overwritten" cannot be written generically at runtime. The class must be closed at
  **compile time** by forcing every new field to be classified.

### Decision

Two complementary guards, both mandatory (AC-08), neither a `debug_assert`:

**(1) Runtime per-slug boot assertion — `assert_per_slug_isolation`.** Generalize
`assert_wave_b_precondition` into a helper called **once per built slug** during boot, returning
`Result<(), ServerError>` so any failure aborts daemon boot loud.

*Param refinement (ratifies Stage-3a OQ-1).* The original signature
`assert_per_slug_isolation(input: &ProjectServerInput, resolver, config)` asserted "after
`build_project_server`, **before** `from_servers` moves the inputs." That literal ordering is
**impossible**: the `Arc::ptr_eq` convergence check needs the *resolver*, and the resolver does
not exist until `from_servers` has already consumed (moved) the per-slug inputs. The accepted
shape captures a lightweight per-slug **`IsolationProbe`** — `{ slug, session_registry, pending,
services, has_hold, signal_class_names }`, all cheap `Arc` clones — in the existing pre-move
tick-context loop (`main.rs:1229`, where the inputs are still owned), **then** builds the router
via `from_servers`, **then** asserts per slug against the built resolver:
`assert_per_slug_isolation(&probe, &*router, &config)?`. This is a param-type refinement only
(`&ProjectServerInput` → `&IsolationProbe`); every guarantee below is preserved unchanged — the
`ptr_eq` handles compared are the same instances, captured before the move and checked after the
resolver exists. For each built slug it asserts:

- **P1 convergence (`Arc::ptr_eq`):** `resolver.registry_for(&slug)` **is**
  `input.server.session_registry`, and `resolver.pending_for(&slug)` **is**
  `input.server.pending_entries_analysis` — the instance the write path resolves is the instance
  the read path holds. (This is a boot check on real handles, not a `debug_assert`.)
- **P1 pairing (F1/SR-03):** `input.server.session_registry.has_transcript_hold()` is true —
  the registry carries a wired hold, so the purge gate cannot split.
- **P2 convergence:** `resolver.services_for(&slug)` resolves to the slug's `ServiceLayer` (same
  store handle the server dispatches against).
- **P3 non-default (where a sentinel is checkable):** the config-snapshot fields are the slug's
  resolved values, not the `UnimatrixServer::new` defaults (e.g. `transcript_signal_class_names`
  non-empty when the slug declares signals; retention/store config equal the slug's resolved
  config). Fields with no clean runtime sentinel are covered by guard (2).
- **Correctly-global fields:** `embed_service` is `Arc::ptr_eq` the one shared model across every
  slug (catches an accidental per-slug re-load).

**(2) Compile-time field census — exhaustive destructuring, no `..`.** A census helper/test
destructures `UnimatrixServer` with an **exhaustive pattern and no `..` rest**:

```rust
let UnimatrixServer {
    session_registry, transcript_hold, pending_entries_analysis,
    observation_registry, inference_config, store_config, retention_config,
    transcript_signal_class_names, services, embed_service, categories,
    client_type_map, /* …every field… */
} = server;   // NO `..` — a newly added field is a COMPILE ERROR here
```

Each binding is routed to its classification — **PER-SLUG** (asserted by guard 1),
**CORRECTLY-GLOBAL** (`embed_service` — one shared ONNX model), or **CORRECTLY-PER-INSTANCE**
(`client_type_map`, `tool_router`). Adding any field to `UnimatrixServer` breaks compilation
until the author classifies it.

**`categories` is classified PER-SLUG, matching shipped code** — the census records reality, not
prose. `main.rs:1183` builds a per-slug `slug_categories` from `r.knowledge.categories` and
threads it into `build_project_server` (`http_provision.rs:153,257,267`), so `categories` is
config-driven per slug today (crt-031/vnc-040). It is a **config-snapshot** field (set at the
constructor from the threaded param, like the P3 fields) — so it needs no `Arc::ptr_eq`
handle-convergence boot check in guard (1); where a sentinel is checkable it is covered like the
other config-snapshots, otherwise by the census + AC-06 exception. NFR-5's characterization of
`categories` as a "global operator allowlist" is **stale** relative to shipped code; a reviewer
should not read the per-slug classification as an NFR-5 violation (NFR-5 prose is the candidate
for correction at retro, not this census). This is the structural whole-class guard SR-02 demands: a future
field cannot ship unclassified, and a per-slug classification forces it into guard (1).

The two guards are **complements, not substitutes** for the behavioral suite (ADR-004): the
census + boot assertion are white-box structural guards (catch unwired fields the behavioral
enumeration might miss and fire at boot before any request); the behavioral suite proves the
observable property implementation-agnostically. SCOPE OQ-4 confirmed both required.

### Consequences

- **Easier:** the whole "constructor-default never overwritten" bug class is closed — a new
  global field either compiles-fails the census (unclassified) or boot-aborts (classified
  per-slug but unwired). #930's class of silent-read-zero becomes loud-at-boot for good. Serves
  the integrity goal (#5474): forecloses the class, not the instance.
- **Easier:** the census doubles as living documentation of every field's isolation
  classification — the SR-02 "enumerate and classify" recommendation, enforced by the compiler.
- **Harder:** the census must be maintained (that is the point — it *forces* the maintenance);
  adding a field is deliberately not free. The boot assertion adds a bounded per-slug startup
  cost (`ptr_eq` + `has_transcript_hold`, no I/O).
- **Boundary (SR-05/OQ-3):** config fields with no clean public observation surface are covered
  by guard (1)+(2) as a **documented AC-06 exception** (recorded in ADR-004's coverage list) —
  never silently dropped from proof.

Related: crt-054 / ADR-010 `assert_wave_b_precondition` (extended here), #5629 (the ptr_eq
boot-assertion guard is part of the governing pattern), SR-02 (inventory incompleteness), SR-06
(no `debug_assert` reliance), ADR-002 (construction this pins), ADR-004 (the behavioral
complement).
