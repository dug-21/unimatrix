# Test Plan — `context_tag` handler (`unimatrix-server` mcp/tools.rs)

> `#[tool(name="context_tag")]` fn + `TagParams { id, action, tag, agent_id, format }`. Responsibilities: identity, `Write` gate, action/tag parse, namespace derivation, lifecycle guard, marked value-opacity seam (comment only), delegate to `StoreTagService`.
>
> **BINDING (#5468): the `#[tool]` fn is NOT unit-constructible — no `RequestContext<RoleServer>` in unit scope. This file plans NO handler unit tests.** Handler-owned decision logic is tested via extracted `pub(crate)` seam fns (pattern #5389); end-to-end handler behavior (route/format, Write-gate, discovery) is proven in the Stage-3c integration suite (see OVERVIEW integration plan). Do NOT instantiate the `#[tool]` fn.

## Extraction requirement (hard, to Stage 3b)

Per pattern #5389, the handler MUST delegate its two testable decisions to free `pub(crate)` fns so they are reachable without a `RequestContext`:

- `derive_namespace(tag: &str) -> Option<&str>` (or owned) — substring before first `:`, else `None`.
- the lifecycle-guard decision (see store-tag-service.md R-05 — extracted fn OR service-resident).

If these stay inline in the `#[tool]` fn they are untestable. **Flag any inline-only decision logic as REWORKABLE at Gate 3a.**

## R-06 — Namespace derivation boundary table (Med)

Direct table test against `derive_namespace` (pure, no store). Each case asserts the derived namespace; the audit `namespace` assertion for the same inputs lives in store-tag-service.md R-03 #6.

1. `test_derive_namespace_standard` — `"delivery:proven"` → `Some("delivery")` (byte-prefix before first `:`).
2. `test_derive_namespace_colon_terminated` — `"delivery:"` (empty value) → `Some("delivery")`; full tag stored as-is (value-opaque, no rejection).
3. `test_derive_namespace_colon_less` — `"reviewed"` → `None` (→ audit `namespace:null`; `replace` degrades to add, cross-links R-02).
4. `test_derive_namespace_multi_colon` — `"delivery:proven:extra"` → `Some("delivery")` (before the **first** `:`, deterministic); full tag stored verbatim.
5. `test_derive_namespace_mid_string_colon` — `"x-delivery:proven"` → `Some("x-delivery")` (first-colon positional rule, NOT vocabulary-aware — documents that derivation is positional).
6. `test_derive_namespace_empty_or_whitespace` — `""` / whitespace-only tag → treated as malformed → the handler path surfaces `rmcp::ErrorData::invalid_params` (a silent write is a bug). Assert the parse/validation rejects it before any write. (ARCH §4.4)

**Coverage:** namespaced, colon-terminated, colon-less, multi-colon, mid-string-colon, empty — first-colon positional semantics proven; every case's derivation asserted.

## R-04 — Value-opacity static/no-validator proof (Low-Med)

Static / review assertions (grep + code review, no runtime seam):

1. `test_no_protected_tags_type_shipped` (or grep gate) — confirm NO `ProtectedTagsConfig`, `ProtectedTagRule`, `TagDisposition`, `evaluate_protected_tag`, allow-list, or vocabulary type was introduced anywhere in the diff. (AC-05, R-04)
2. `test_value_opacity_seam_is_marked_comment_only` — the single pre-write interception point exists as an ADR/code note (comment marker) — NO stub, NO empty config, NO `evaluate(tag)` call. Exactly one marked point. (FR-07, ADR-008 pt 4)
3. `test_not_conflated_with_validate_outcome_tags` — grep/review confirms the `context_tag` path does NOT call `validate_outcome_tags` (tools.rs:895-898); the two vocabularies are independent. (Runtime half is in store-tag-service.md R-04 #2.)

> **Do NOT write a rejection-path test — no validator ships.** The two preserved seams (value-opacity pre-write point; `Write`-gate trust-elevation LOCATION) are marked notes with no behavior; their correctness is covered negatively (no validator shipped; no `TrustLevel` consulted — authz assertion in the integration Write-gate test).

## Deferred to Stage-3c integration (route/format — NOT here)

These need the live MCP transport (`RequestContext`) and belong to infra-001, per OVERVIEW:
- tool registered/discoverable with the `TagParams` schema (protocol suite).
- agent lacking `Capability::Write` rejected at the route (security suite).
- add/remove/replace callable end-to-end and reflected in the next tag-filtered read — **read-freshness NFR-04** (lifecycle suite: add→present, remove→absent).
- quarantined entry refused end-to-end (security suite).
- SQL-metachar `tag` stored literally, no sibling over-match observable via by-tag search (security suite, R-08 end-to-end).

## Out of Scope (do NOT test)
- `min_trust_level` / trust-elevation accept-reject difference — DEFERRED, not shipped.
- Any `evaluate(tag)` hygiene rejection — DEFERRED.
- Handler behavior by instantiating the `#[tool]` fn — impossible (#5468).
