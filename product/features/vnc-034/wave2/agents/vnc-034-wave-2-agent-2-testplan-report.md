# Agent Report — vnc-034-wave-2-agent-2-testplan (Stage 3a, Wave 2)

## Deliverables (all under `wave2/test-plan/`)
- `product/features/vnc-034/wave2/test-plan/OVERVIEW.md`
- `product/features/vnc-034/wave2/test-plan/project-router.md`
- `product/features/vnc-034/wave2/test-plan/project-registry-cli.md`
- `product/features/vnc-034/wave2/test-plan/projects-config.md`

## Risk coverage mapping (Wave 2 + cross-wave)
| Risk | Priority | Owner plan | Key tests |
|------|----------|------------|-----------|
| R-01 | Critical | project-router | swap-at-callsite, `PathRouter::new` takes `Arc<dyn StoreResolver>` (structural), unknown-slug→UnknownProject (no fallback), routing inside `resolve_store` |
| R-03 | High (fix-before-merge) | projects-config | security table T-SEC-01..20 incl. discriminators **T-SEC-15 underscore-reject** + **T-SEC-16 64-char-reject**; no-fs-escape assertion |
| R-04 | High | project-router | Default path unchanged under ProjectRouter; slug⟂path-hash disjoint |
| R-06 | High | project-router | N:1 shared `Arc<Store>` (`Arc::ptr_eq`); no payload project field (structural) |
| R-10 | Medium | project-router | BearerValidator/TlsConfig/slug seams intact (AC-CT-C6) |
| R-12 | Medium | project-router | only `/health` unauth; no per-slug health endpoint (D3) |
| R-13 | Low | projects-config | `[[projects]]`-absent ⇒ Default unchanged |

## AC coverage
AC-W2-R1/R3/R5 → Rust HTTP integration; AC-W2-R2 → unit + infra-001 smoke;
AC-W2-R4 → registry-CLI unit; AC-W2-R6 → security table; AC-CT-C4 → structural +
smoke; AC-CT-C6 → structural.

## D1/D2/D3 handling
- **D1** — exact `^[a-z0-9][a-z0-9-]{0,62}$`. Discriminators T-SEC-15 (`my_project`
  REJECT) and T-SEC-16 (64-char REJECT) turn the drifted `[a-z0-9_-]{0,63}` impl red;
  T-SEC-17 (63-char ACCEPT) pins the bound. PR review must see all three.
- **D2** — `test_no_per_project_config_overlay_merge` (negative) asserts no overlay
  surface introduced.
- **D3** — `list` MAY carry operator-side store-open status
  (`test_list_may_carry_store_open_status`); `test_no_per_slug_health_endpoint` +
  `test_list_exposes_no_network_health` assert NO per-slug network surface; AC-W1-S6
  stays intact.

## Integration suite plan
- **infra-001 = backward-compat gate only.** It spawns `serve --stdio --project-dir`
  (single-project, no HTTP slug edge), so it cannot test slug routing/isolation.
  Mandatory `pytest -m smoke`; recommended `tools`, `lifecycle`, `protocol` to prove
  the resolver swap did not regress the Default path.
- **Slug routing + isolation = NEW Rust HTTP integration** in
  `crates/unimatrix-server/tests/project_routing_integration.rs` (two real per-slug
  stores behind a `ProjectRouter` injected via `PathRouter::new`, driven through the
  `/v1/{slug}/` edge with the router-test body helpers).

## Open questions (for Stage 3b/3c + scrum master)
1. **`delete` store-dir semantics** — design says "removes or retires" the per-slug
   dir; confirm whether delete hard-removes `/data/.unimatrix/{slug}/` or retires it
   (affects `test_delete_removes_or_retires_store_dir`). Hard-remove destroys an
   unrollbackable hash chain — needs an explicit operator confirm/retention call.
2. **`register` idempotence** — idempotent no-op vs loud "already registered". Test
   `test_register_idempotent_or_errors_on_existing` pins whichever is chosen; design
   should state which.
3. **Reserved-slug refusal in register** — seam.rs documents reserved words
   (`health`/`observe`/`v1`/`tools`) as a Wave-2 CLI concern. Confirm `register`
   rejects them (planned `test_register_rejects_reserved_slug`); if not, drop that test.
4. **D3 list status field** — is the per-slug store-open status actually wanted on
   `list`, or omitted entirely? Plan covers both (status present → assert local-only;
   absent → only the negative no-network-health test applies).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + two `context_search` calls — surfaced
  ADR-003 (#4950 seam), ADR-004 (#80/slug allowlist), ADR-005 (#4949 default alias),
  and lessons #4962/#4963 (build-but-unwireable seam → `PathRouter` holds
  `SlugRouter`, resolver is the sole swap point). Applied directly to the structural
  R-01/AC-CT-C4 tests and the HTTP integration fixture wiring.
- Stored: **nothing** — attempted to store the infra-001-transport-mismatch pattern
  (infra-001 spawns single-project stdio MCP, cannot reach the HTTP `/v1/{slug}/`
  edge, so slug routing/isolation needs Rust HTTP integration not infra-001) via
  `context_store`, but the call was rejected: this agent context lacks Write
  capability (`MCP error -32003: Agent 'anonymous' lacks Write capability`). The
  insight is captured in OVERVIEW.md §4 and this report; the leader/SM should store
  it post-merge (it is a reusable testing pattern for any HTTP-path-specific server
  feature).
