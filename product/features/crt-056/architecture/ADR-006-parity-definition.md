## ADR-006: Parity definition — closed checklist; `adapt_service` per-slug; `session_capabilities` OUT (settled)

### Context
SR-05 / OQ-5: parity was under-defined. AC-1 lists config fields, but two items needed resolution:
(a) is `adapt_service` per-slug (independent state, current) or shared? (b) is `session_capabilities`
per-slug — needed for parity, or out of scope? An incomplete parity list ships a still-degraded slug
and a silent C5 gap. Parity must be a **closed checklist** asserted field-by-field against the
daemon's RESOLVED config (not a representative subset). Both are now resolved: `adapt_service` is
per-slug (same config, independent state); `session_capabilities` is OUT (human design-review
decision on OQ-5, recorded below — not a recommendation).

### Decision

**The parity checklist (the 8 config-driven fields threaded in ADR-002), asserted field-by-field in
AC-1 against the daemon's resolved config:**
1. `nli_enabled` (config value, not hardcoded `false`)
2. `nli_top_k`
3. shared loaded `nli_handle` — the **one** model (AC-2: one in memory, not N, not unloaded)
4. `inference_config` — fusion/PPR/blending weights (not `InferenceConfig::default()`)
5. `confidence_params` — operator weights (not `ConfidenceParams::default()`)
6. `category_allowlist` — operator allowlist + lifecycle policy (not `CategoryAllowlist::new()`)
7. `observation_registry` — operator domain packs (not built-in-only)
8. `rayon_pool` — sized per config (not the size-1 `test-pool`)

These are the daemon's resolved values from `main.rs:880-898`; AC-1 asserts equality per field.

**`adapt_service`: per-slug (independent state), confirmed — NOT shared.** Today
`build_project_server` constructs a per-slug `AdaptationService::new(AdaptConfig::default())`
(`http_provision.rs:180`). `AdaptationService` carries per-project adaptation state; sharing it
across slugs would be a cross-slug shared-mutable hazard (SR-01 class) and would conflate distinct
projects' adaptation. Parity here means **same config, independent state** — each slug adapts on its
own store. (If `AdaptConfig` itself becomes operator-configurable, that config — not the service
state — would be threaded; not the case in crt-056, where `AdaptConfig::default()` is the resolved
value.) This is consistent with per-slug stores being isolated knowledge surfaces (FR-C3).

**`session_capabilities`: OUT of crt-056 parity scope — SETTLED (human design-review decision, not a
recommendation).** `session_capabilities` is a per-session/per-client negotiated surface (MCP
initialize handshake), not a config-driven analytics or retrieval-quality field. It does not depend
on, and does not feed, the per-slug analytics handles or the threaded config. The two crt-056 defects
(test-config serving, dead analytics) do not touch it. Including it would expand scope without
advancing the C5 "first-class Unimatrix" claim, which is about retrieval quality + maintained
analytics. The human resolved OQ-5 in design review: `session_capabilities` is **OUT**. AC-1's parity
checklist is the 8 config-driven fields above and does **NOT** include `session_capabilities`. This is
no longer an open question (ARCHITECTURE.md §8 records OQ-5 as settled). If it is ever required it is
an additive, separately-scoped AC — not a 9th item retro-fitted into crt-056's checklist.

### Consequences
- **Easier:** AC-1 becomes a concrete, closed, field-by-field assertion — no "representative subset"
  ambiguity (SR-05 retired). The reviewer has an exact checklist.
- **`adapt_service` clarity:** "parity = same config, independent state" is the rule; it prevents an
  accidental shared-`adapt_service` refactor that would reintroduce cross-slug corruption.
- **Scope held (settled):** the `session_capabilities` exclusion is final for crt-056 — it keeps the
  feature to retrieval-quality + analytics parity; if later required it is an additive, separately
  scoped AC, not a re-architecture and not a 9th checklist item here.
- **Boundary (SR-06):** this checklist is the daemon's single GLOBAL resolved config. Per-slug
  CUSTOM overrides remain #785 / C6.

Related: ADR-002 (threads these 8 fields), ADR-003 (the independent per-slug analytics state behind
"independent state").
