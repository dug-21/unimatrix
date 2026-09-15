# Agent Report — vnc-049-agent-1-pseudocode

**Agent:** uni-pseudocode | **Stage:** Session 2 Stage 3a (pseudocode) | **Date:** 2026-09-15

## Deliverables

Per-component pseudocode written to `product/features/vnc-049/pseudocode/`:

- `OVERVIEW.md` — component interactions, data flow, shared types, derivation-site pin (OQ-4), and the
  4-wave implementation dependency ordering.
- `c1-plugin-shim.md` (C1, TS net-new) — 7-event map, model resolution, subagent alignment, Stop
  over-count guard, PreCompact gate, fail-open.
- `c2-provider-arm-rust.md` (C2, Rust) — KNOWN_PROVIDERS + new `hook/opencode.rs`, model_id plumbing.
- `c3-provider-arm-js.md` (C3, JS) — normalize.js mirror of C2 (col-022).
- `c4-wire-carriers.md` (C4, Rust) — `model_id` on HookInput + ImplantEvent, charset flag.
- `c5-ingest-persistence.md` (C5, Rust+SQL) — schema migration + write-path provider-first stamp.
- `c6-read-path.md` (C6, Rust) — prefer-stored source_domain, legacy NULL fallback, surface model_id.
- `c7-domain-pack.md` (C7, Rust) — opencode DomainPack (legacy fallback + categories; EC-07 collision flag).
- `c8-parity-corpus.md` (C8, Rust) — opencode cases across 7 events, drift sentinel.
- `c9-installer.md` (C9, JS) — non-clobbering additive branch, idempotence, byte-for-byte preservation.

## Coverage confirmations

- Ingest-persistence reframe (ADR-001): C5 write-path stamp opencode-only, fail-loud canary; C6 read fork.
- model_id wire carrier (ADR-002): C4 field → C2 plumb → C5 column → C6 surface → C8 parity.
- source_domain opencode-only stamp (2026-09-15 human ruling): C5 derive (opencode→"opencode", else NULL),
  C6 legacy fallback preserves T-SEC-12/13 byte-for-byte.
- New-module-with-thin-wiring (ADR-005): new `hook/opencode.rs` (C2), `opencode-install.js` (C9); over-cap
  files (hook.rs/listener.rs/observation.rs/db.rs/migration.rs) get wiring lines only.
- OpenCode→canonical 7-event map (C1) mirrored by C2/C3, covered by C8.

## Derivation-site pin (OQ-4/R-02)

Stored `source_domain` for opencode is produced at the **write path (C5 listener insert site)** from
`ImplantEvent.provider`; `resolve_source_domain` (C7) is authoritative for legacy NULL rows only. Pinned
by test in C6/C8.

## Dependency ordering (waves)

- Wave 1: C4, C7, C5a (schema).
- Wave 2: C5b (write bind), C2.
- Wave 3: C6, C3, C1.
- Wave 4: C8, C9.
- Critical path: C4 → C5 → {C2, C1} → C8. col-022 triad (C2+C3+C8, and C1 event map) lands in one change.

## Open questions / gaps flagged (not silently resolved)

- **OQ-A (new):** two `ObservationRow` structs — write-path struct (`listener.rs:3383/3416`, has
  topic_signal/phase/topic_source) vs read struct (`observations.rs:14`, lacks them). Confirm which gains
  the new fields and where it is built from `ImplantEvent`.
- **model_id charset:** value `"<providerID>/<modelID>"` contains `/`, so it is NOT the source_domain
  `^[a-z0-9_-]{1,64}$` contract; C4 defines a separate `^[a-z0-9._/-]{1,128}$` carrier charset. Confirm.
- **C7 EC-07 collision:** opencode pack must NOT claim shared canonical event_types (non-deterministic
  resolution) and must NOT use empty `event_types` (claims-all). Delivery must pick an explicit safe form.
- **OQ-3/R-11:** `session.idle`→Stop semantics need a delivery-time PoC (C1 guards conservatively meanwhile).
- **OQ-7/R-12:** PreCompact experimental flag default (C1).
- **R-06 ordering:** whether subagent parent→cycle correlation resolves plugin-side or parentID is carried
  for server-side correlation (C1).
- **Schema version:** resolve NEXT against `CURRENT_SCHEMA_VERSION` at delivery — do NOT hardcode (32 today).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern) → #5737 three coupled touchpoints, #4298 normalize at
  ingest boundary, #4305 canonical names ADR; mcp__unimatrix__context_search (decision, topic vnc-049) →
  #5748 (ADR-001), #5739 (ADR-002), #5740/#5743/#5744/#5745. All incorporated.
- Deviations from established patterns: none. Pseudocode follows #5737 (three coupled touchpoints),
  #4306 provider-field convention, #4751 parity-corpus oracle, and lesson #5670 (4-layer model_id landing).
- Read-only tier; no storage performed.
</content>
