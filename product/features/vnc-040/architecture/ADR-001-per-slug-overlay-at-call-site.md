## ADR-001: Per-slug config overlay resolved at the `build_project_server` call site via a `resolve_slug_config` helper, reusing `merge_configs`

### Context
crt-056 (#789, ADR-002 #5165) threaded the daemon's single RESOLVED config into
`build_project_server` and explicitly **reserved the per-slug-override seam for #785**: *"#785
later adds overrides; this ADR must not introduce that seam."* C6 (Unimatrix #5148) now needs
each routed slug to resolve its OWN categories / domain packs / confidence weights / inference
tuning, while `load_config`'s established global→project→env layering (dsn-001 #2286) stays
untouched and the local-UDS / single-project majority sees zero behavior change.

`merge_configs` (`config.rs:≈3825`) is already the canonical per-key replace merge (per-project
value if it differs from compiled `Default`, else global; `Option` via `.or()`; hash pins
global-wins per #4655). `load_single_config` (`config.rs:3783`) and `validate_config`
(`config.rs:3413`) are the existing load+validate machinery. The scope LOCKS reuse of all three
— no new merge model. The per-slug store already lives at `{base_dir}/{slug}/`
(`http_provision.rs:159`), so the natural per-slug config file is `{base_dir}/{slug}/config.toml`.

### Decision
Add a small `resolve_slug_config(base_dir, slug, &global) -> Result<Cow<UnimatrixConfig>, ServerError>`
helper at the call-site module and invoke it inside the per-slug loop (`main.rs:1089-1110`),
NOT in `load_config`. For each slug:

1. Probe `{base_dir}/{slug}/config.toml`.
2. **No file** → return the global config unchanged (Cow::Borrowed) — byte-for-byte fallthrough
   (see ADR-002). No merge, no re-derivation.
3. **File present** → `load_single_config` → per-file `validate_config` (AC-08) →
   `merge_configs(&global, &slug_file)` (third precedence layer, SAME function) → **post-merge
   `validate_config`** (ADR-003) → return Cow::Owned(merged).

The per-slug loop then derives the 7 overlayable values — the 6 engine/knowledge `Arc`s plus
`instructions` — from the resolved config and passes them into the **unchanged**
`build_project_server` signature. `instructions` is the change #785 explicitly names: today it is
sourced global at `main.rs:687` (`let server_instructions = config.server.instructions.clone();`)
and fanned identically to every slug at `main.rs:1095`; vnc-040 sources it from
`resolved.server.instructions` at the seam instead, so a per-slug file can tune it. The C6 overlay
is a THIRD precedence layer atop global→project, using the identical field-level replace
discipline of dsn-001 #2286 — replace semantics, list fields replace not append, hash pins
global-wins. No conflict; C6 extends the established pattern.

The two remaining non-handle call-site inputs are classified explicitly so none is absent:
`permissive` (a process/daemon permission flag, NOT knowledge config) stays **GLOBAL-locked** —
passed unchanged from the global daemon value, never read from the merged config;
`base_dir`/`slug` are routing identity, not config. The seam stays at the `build_project_server`
caller exactly as crt-056 reserved it; no override parameter is added to `load_config`. Transport
config is never read here.

### Consequences
- **Easier:** per-slug domain config (including `[server] instructions`, #785's explicit knob)
  with one localized change; `load_config` and the global layering are untouched; reuses proven,
  audited code paths.
- **Easier:** every call-site input gets an explicit overlayable/locked verdict (ARCHITECTURE §9),
  so no input is silently dropped — the prior framing omitted `permissive` and `instructions`.
- **Easier:** A and B share one file path (`{base_dir}/{slug}/config.toml`); Feature B (seeding)
  builds on this contract without re-litigation (SR-06). `adapt_service` stays `AdaptConfig::default()`.
- **Harder / cost:** a new caller routes a global→per-slug pairing through `merge_configs`, which
  was written for global→project. SR-02 mitigation: re-audit the inline `InferenceConfig { … }`
  merge literal (#4070) for the new call shape before reuse; do not assume identical coverage.
- **Bounded:** depends on the stable crt-056 call-site seam (A3). Design is against the live
  merged signature (`base_dir, slug, embed_handle, permissive, instructions, <9 crt-056 params>`),
  not the issue's "8 fields".
- Cross-references ADR-002 (fallthrough + model invariants) and ADR-003 (post-merge re-validation).
