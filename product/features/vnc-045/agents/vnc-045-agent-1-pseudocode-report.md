# Agent Report — vnc-045-agent-1-pseudocode

**Role:** Pseudocode specialist (Session 2 Stage 3a). Produced per-component pseudocode for the
`context_tag` mechanism.

## Deliverables

`product/features/vnc-045/pseudocode/`
- `OVERVIEW.md` — component interaction, data flow, shared types (`TagParams`, `TagAction`,
  `TagResult`, `TagAuditMetadata`), audit shape, sequencing constraints.
- `store-tag-primitive.md` — `add_tag` / `remove_tag` / `replace_tag` (atomic namespace-scoped).
- `audit-op-list.md` — add `'context_tag'` to `audit_write_count_since` op-list.
- `store-tag-service.md` — `StoreTagService::tag` (throttle → store → fire-and-forget audit).
- `context-tag-handler.md` — `#[tool(name="context_tag")]` handler + `TagParams`.

## Components covered

Direct tag-write primitives (unimatrix-store); audit op-list (unimatrix-store); StoreTagService
(unimatrix-server, new); context_tag handler (unimatrix-server). Only active ADRs (001/002/004/008/009)
designed; deferred ADRs (003/005/006/007 — protected_tags) NOT designed.

## Delivery-critical items honored

- replace = ONE atomic tx (namespace-scoped `DELETE ... LIKE 'ns:%'` + `INSERT`, rollback-safe,
  mirrors the single-tx tag write; NOT delete-all, NOT `update()`).
- Colon-less/null-namespace replace degrades to `add` (service routes `namespace=None` → `add_tag`).
- Complete generic audit `{action, namespace, tag, prior_value, new_value}`; `prior_value` mandatory
  on remove/replace; namespace derived-never-validated; never the `"{}"` sentinel (serialize error →
  warn + SKIP); `session_id` captured before `tokio::spawn`; one event per mutation.
- Value-opacity: tag written uninterpreted; both retrofit seams are comments only; `validate_outcome_tags`
  explicitly NOT invoked.
- R-08: `like_escape` + `ESCAPE '\'` on the derived prefix; all queries bound-parameterized.
- Lifecycle guards re-implemented in-op (quarantined refused; deprecated allowed).
- No ProtectedTagsConfig, no min_trust_level, no cadence guard, no config threading, no stub, no
  `update()` reuse, no `content_hash` touch, no schema change.

## Deviations from established patterns

- **`remove_tag` returns `Option<String>`** (BRIEF signature was `-> Result<()>`). Additive refinement
  so the service satisfies ADR-009 "prior_value non-null on remove" from the primitive's own report
  rather than re-deriving. Mirrors `replace_tag`'s prior-returning shape. Flagged as Open Question.
- All other signatures match the BRIEF/ARCHITECTURE integration surface exactly.

## Open questions / gaps (non-blocking)

1. `remove_tag` return type (see above) — confirm the additive `Option<String>`.
2. R-08 strategy choice: pseudocode implements **LIKE-escape in the store** (safe regardless) and
   marks handler-side reject as the alternative. Delivery should pick ONE (recommended: escape).
3. `remove` of an absent tag: primitive returns `None`, but service still records `prior_value = tag`
   (client-intent-of-record) to keep ADR-009 non-null on remove. Confirm this over "prior_value:null
   when the tag didn't exist" — spec says prior_value mandatory/non-null on remove, so intent-of-record
   is the ADR-conformant reading. Flagged for tester/architect confirmation.

## Gate 3a rework (2026-07-07)

REWORKABLE FAIL on Check 4 (interface consistency) — sole issue: `derive_namespace` and the
lifecycle-guard decision were designed inline in the non-constructible `#[tool]` handler, so the
R-06 boundary table and R-05 guard tests (which bind extracted `pub(crate)` seam fns, #5389/#5468)
were unrunnable. Fixed by promoting both to designed `pub(crate)` module-scope seam fns:
- `pub(crate) fn derive_namespace(tag: &str) -> Option<String>`
- `pub(crate) fn check_tag_lifecycle(status: &Status) -> Result<(), LifecycleRejection>` (+ `enum LifecycleRejection { Quarantined }`)

Both added to OVERVIEW.md shared-types surface with bodies designed in context-tag-handler.md;
handler Steps 5/7 now CALL them; OVERVIEW sequencing wording reconciled from "Derivation lives in
the HANDLER" to "the handler CALLS the extracted pub(crate) helper." No other change; all
delivery-critical items and other checks remained PASS. Scope unchanged.

Files edited: `pseudocode/OVERVIEW.md`, `pseudocode/context-tag-handler.md`.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern + decision) and `context_briefing` — MCP server
  returned "No such tool available" for all context_* calls (unavailable this session). Non-blocking
  per protocol; proceeded from the three source documents, active ADR files, and direct HEAD codebase
  reads (`store_correct.rs`, `write.rs`, `audit.rs`, `read.rs`, `write_ext.rs`, `gateway.rs`,
  `tools.rs`, `schema.rs`, `services/mod.rs`, `db.rs`).
- Deviations from established patterns: one additive signature refinement (`remove_tag -> Option<String>`),
  documented above. All reused contracts traced to HEAD line numbers.
