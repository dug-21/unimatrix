# Test Plan — C7 OpenCode Domain Pack

`crates/unimatrix-observe/src/domain/mod.rs` — add an opencode builtin/config `DomainPack`
(`{ source_domain: "opencode", event_types, categories, rules }`; `source_domain` format
`^[a-z0-9_-]{1,64}$`). `resolve_source_domain` (:180) is UNCHANGED and is used only for legacy NULL
rows. Under-cap file (106 code-lines).

Risks owned: **R-04 (legacy-row resolution)**, contributes R-15 (format contract). ACs: supports
AC-03 for legacy/NULL rows.

Test surface: Rust `crates/unimatrix-observe/tests/domain_pack_tests.rs` (extend — existing tests:
`test_resolve_source_domain_known_event_type` :145, `test_resolve_source_domain_unknown_event_type_
returns_unknown` :161).

## Unit expectations

### Domain pack registration
- `test_opencode_domain_pack_registered` — the opencode `DomainPack` is present in the registry
  with `source_domain="opencode"` and the correct event_types/categories.
- `test_opencode_source_domain_format_valid` — the pack's `source_domain` matches
  `^[a-z0-9_-]{1,64}$` (R-15 format contract).

### resolve_source_domain unchanged (R-05, backward-compat)
- Existing `test_resolve_source_domain_known_event_type` (:145 → `"claude-code"`) and
  `test_resolve_source_domain_unknown_event_type_returns_unknown` (:161 → `"unknown"`) MUST re-run
  green, unchanged. The opencode pack must NOT change the resolution of claude-code's canonical
  event names (OpenCode emits the SAME canonical names — so `resolve_source_domain` continues to
  return the read-derived value for legacy/NULL rows; the opencode distinction comes from the
  STORED `source_domain` at write, c5, not from event-type resolution).
- `test_opencode_pack_does_not_shadow_claude_code_resolution` — asserts that adding the opencode
  pack does not cause a claude-code canonical event to resolve to `opencode` (would break T-SEC-12/13
  and the legacy fallback). This guards the subtle trap that a domain pack keyed on shared event
  names cannot be the opencode discriminator — that is what ADR-001's ingest stamp is for.

## Role clarity (important — avoids the SR-07 trap)
The opencode domain pack is for **read-path/category registration + legacy-NULL row resolution**,
NOT the mechanism that makes a NEW opencode row read back as `opencode`. New rows get
`source_domain="opencode"` from the ingest stamp (c5). If a test tried to prove AC-03 via the domain
pack alone, it would be proving the wrong mechanism (event-derived resolution reads opencode's shared
canonical names back as claude-code — the exact SR-07/SR-11 defect). c7 therefore does NOT own an
AC-03 stored-record assertion; it owns the legacy-row resolution and format contract.

## Cross-references
- c5 owns the ingest stamp (new opencode rows).
- c6 owns the read fork that PREFERS the stored value and falls back to this pack's resolution for
  NULL rows.
