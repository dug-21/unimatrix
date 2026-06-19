## ADR-001: Additive `UnimatrixServer::new` — accept a pre-built `ServiceLayer` via `Option`

### Context
`UnimatrixServer::new` (`server.rs:281-386`) builds the `ServiceLayer` *inside itself* with
hardcoded **test defaults** (`server.rs:306-333`): a size-1 `test-pool`, `NliServiceHandle::new()`
(unloaded), `nli_enabled: false`, `InferenceConfig::default()`, `ConfidenceParams::default()`,
`CategoryAllowlist::new()`, built-in-only domain packs. Every caller — the daemon
(`main.rs:919-933`), per-slug `build_project_server` (`http_provision.rs:186-197`), and all unit
tests — gets these test defaults. That is the *direct cause* of Defect 1: per-slug servers serve
in test-config mode.

For Wave 1, per-slug servers must serve with the daemon's *config-driven* `ServiceLayer`. But the
test-default construction is **existing, depended-upon behavior** (SR-03): dozens of unit tests,
`test_support.rs`, `uds/listener.rs::make_services`, `infra/shutdown.rs` helpers all rely on
`UnimatrixServer::new` producing a usable server with no config. Dropping that path breaks the
suite and risks a cloud-only code path the local single-project install never exercises — a
direct violation of the vnc-034 ADR-003 single-isolation-seam constraint.

OQ-4 asks: required `ServiceLayer` param vs `Option` (None ⇒ test default)?

### Decision
Make the refactor **additive** by appending a single final parameter
`services: Option<ServiceLayer>` to `UnimatrixServer::new` (params-at-end, entries #2552/#2553).

```rust
pub fn new(
    /* ...existing 10 params... */,
    instructions: Option<String>,
    services: Option<ServiceLayer>,   // NEW, last param
) -> Self {
    // ...existing server_info / usage_dedup setup...
    let services = services.unwrap_or_else(|| {
        // EXISTING test-default body, moved verbatim (server.rs:306-333):
        // size-1 pool, unloaded NLI, default configs, empty allowlist.
        ServiceLayer::new(/* ...test defaults... */)
    });
    let effectiveness_state = services.effectiveness_state_handle();
    // ...rest unchanged...
}
```

- **Per-slug + daemon callers** pass `Some(config_driven_service_layer)` — built by
  `build_project_server` (ADR-002) / already built at `main.rs:880-898`.
- **All existing test callers** pass `None` — behavior is byte-for-byte the current test default
  (AC-6). The only required edit at test call sites is appending `, None`.

`Option` is chosen over a required param specifically because it **least disturbs call sites**
(OQ-4): test callers append two characters; no test reconstructs a config-driven `ServiceLayer`.

**Single-isolation-seam compliance:** the single-project daemon path (`main.rs:919-933`) ALSO
switches to passing `Some(services)` (it already builds the config-driven `services` at
`main.rs:880-898` and currently discards it for the in-constructor default). So the daemon and
per-slug servers traverse the **same** `Some(...)` parity path; `None` is the test-only branch.
No cloud-only branch is introduced.

### Consequences
- **Easier:** per-slug parity becomes "pass `Some(service_layer)`." The daemon stops
  double-building (it built `services` then ignored it in favor of the in-constructor default).
  Test ergonomics are preserved (`None`).
- **Harder / cost:** every `UnimatrixServer::new` call site gains a trailing arg. Per #2552/#2553
  these are enumerated: `main.rs` (daemon + stdio), `server.rs` self, `http_provision.rs`,
  `test_support.rs`, `uds/listener.rs`, `infra/shutdown.rs`. Mechanical, params-at-end.
- **Risk retired (SR-03):** the `None` arm holds the exact prior body ⇒ AC-6 is structural, not
  hopeful. The daemon + per-slug sharing the `Some` arm retires the cloud-only-path risk.
- **Boundary:** this ADR does NOT change `ServiceLayer::new` (ADR-002 owns its call site). It only
  changes who *constructs* the `ServiceLayer` and passes it in.

Related: ADR-002 (builds the `Some(...)` value per slug), ADR-003 (the handle set inside it).
