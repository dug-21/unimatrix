# vnc-048 Architect Report — agent vnc-048-agent-1-architect

## Deliverables
- `product/features/vnc-048/architecture/ARCHITECTURE.md`
- ADR-001 .. ADR-006 (files + Unimatrix #5693–#5698)

## Key decisions
1. ADR-001 — reuse triad: one base derivation (`data_dir.parent()`, no unwrap), one join site (`per_slug_data_dir`, `&ProjectSlug` only), one validation edge (`validate_slug`), wrapped in a single `resolve_slug_store` funnel. No second scheme (SR-01/C-1/C-2/C-6).
2. ADR-002 — existence check strictly before `SqlxStore::open`; gate is file-existence, not registration (SR-02/SR-04/C-3).
3. ADR-003 — live-PID-only import hard-error makes the shutdown vector-clobber structurally unreachable (SR-03/C-4/OQ-1/AC-13); names PID path + `stop→import→start`.
4. ADR-004 — HNSW rebuild redirected to `slug_dir/vector`; PID stays base-scoped path-hash (SR-10/AC-02/AC-12).
5. ADR-005 — non-empty-`audit_log` pre-flight refusal; supported target is a freshly-registered (audit-empty) slug (SR-05/C-5/OQ-2/AC-10).
6. ADR-006 — fail-loud naming the fully-resolved absolute path on every accept-but-inert path + export stderr count summary (SR-08/SR-11/C-7/AC-03/AC-06).

## Coverage confirmation
- Every User-Facing Entry Point in SCOPE-RISK-ASSESSMENT mapped from the CLI invocation (not an internal seam); no-`--slug` paths untouched (ADR-002 §3).
- SR-01/02/03/11 addressed as named; AC-09 seam design documented (the `_with_base` verbatim-base wrinkle makes the resolvers able to disagree).
- Four deploy shapes carried as a coverage axis (ARCHITECTURE §Deploy-Shape Coverage), host bind-mount = fail-loud corner.
- Three reuse invariants held; no second scheme invented.

## Open questions
- OQ-5 (`#5586`/`#5691` `delivery:proven → partial` retag on AC-09/AC-10 evidence) — vision-session call, flagged for the human, not filed.
- SR-06 (slug-awareness for the other six CLIs as one tracked item) — human call; this feature establishes the `--slug` + `resolve_slug_store` pattern they copy.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get -- surfaced #4972 (per-slug dirs siblings of path-hash under one base; hash names are charset-valid slugs), #5507 (two-resolver seam trap; `_with_base` sets base verbatim), #5344 (register→restart route-liveness precondition), #5087 (vnc-038 local keeps direct path-hash binding), #4359 (ADR-005 vnc-014 append-only audit_log, import cannot clear). All applied.
- Stored: entries #5693–#5698 "ADR-001..ADR-006 vnc-048" via context_store (category decision, topic vnc-048, tags [adr, vnc-048]). No typed edges — append-only constraint fully restated in ADR-005, none traversal-necessary (default-none, HIGH bar). No supersession (additive feature).
