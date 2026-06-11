# Agent Report — vnc-034-wave-2-agent-1-pseudocode (Stage 3a, Wave 2)

> Back-fill report (Gate 3a iteration 1, Check 5). The pseudocode artifacts PASSED and
> are unchanged; this report supplies the missing `## Knowledge Stewardship` block.

## Deliverables (all under `wave2/pseudocode/`)
- `product/features/vnc-034/wave2/pseudocode/OVERVIEW.md`
- `product/features/vnc-034/wave2/pseudocode/project-router.md`
- `product/features/vnc-034/wave2/pseudocode/project-registry-cli.md`
- `product/features/vnc-034/wave2/pseudocode/projects-config.md`

## Components covered

| Component | Source file | Pseudocode |
|-----------|-------------|------------|
| **ProjectRouter** — Wave-2 `StoreResolver` impl | `crates/unimatrix-server/src/http/router.rs` (extend) | `project-router.md` |
| **ProjectRegistry + lifecycle CLI** — `register`/`list`/`delete` | `crates/unimatrix-server/src/projects.rs` *(new)* | `project-registry-cli.md` |
| **`[[projects]]` config + slug validation** | `crates/unimatrix-server/src/infra/config.rs` | `projects-config.md` |

## What the Wave-2 pseudocode produced

**StoreResolver swap (project-router.md).** Designed the drop-in resolver swap at the
single `main.rs` ~L898 call site: `DefaultResolver` → `ProjectRouter::from_registry`.
`PathRouter::new`, `SlugRouter::new`, `parse_project_key`, `ProjectKey`, `ProjectSlug`,
and `RouteError` are kept untouched (ADR-003 — one trait-impl swap, no interface re-cut).
`resolve_store(Slug(unknown))` has no fallthrough: it returns `Ok(entry.store)` or
`UnknownProject`, never a default and never another slug (mirrors `DefaultResolver`).

**Per-slug `adapter_for` funnel + Wave-1 discard eliminated (project-router.md).**
Captured the seam-funnel honesty record: Wave 1's `let _store` discard (seam.rs:283) plus
`self.project_router.route_mcp` fixed-adapter dispatch (seam.rs:299) is removed in its
entirety, not supplemented. The resolved `Arc<Store>` is USED; `adapter_for(&key)` becomes
the SOLE dispatch route (Default included), with no `None`-means-fixed fallback. Added
`adapter_for` as a second `StoreResolver` trait method with **no default impl**, so every
resolver (including the minimally-extended `DefaultResolver`) answers dispatch from the same
map its `resolve_store` reads — resolution and dispatch can never diverge. The
HTTP-`ProjectRouter<ReqBody>` vs resolver-`ProjectRouter` name collision is resolved and
flagged (OQ-PR-1/2/3/8/9).

**ProjectRegistry / CLI with D4/D5/D6 (project-registry-cli.md).**
- **D4** — `delete` (default) is de-register-only (preserves the on-disk data dir + hash
  chain); `--purge` is the sole destroy path, loud, requires `--confirm <slug>` matching
  exactly; re-register RE-ATTACHES to the preserved chain (State B branch on `data_exists`,
  never funneling through a truncating path). OQ-CLI-7 raised on `Store::open`
  non-destructiveness.
- **D5** — reserved-slug refusal at `register`, a check SEPARATE from the D1 charset
  allowlist; `tools` is proven charset-valid-yet-rejected (shadows `/v1/tools/...`, ADR-005).
- **D6** — two-state `register` idempotence: State A (data + routing) → loud error; State B
  (data, de-registered) → re-attach; State C (no data) → fresh create — never collapsed into
  one generic message.

**`[[projects]]` config with reused D1 allowlist (projects-config.md).**
`ProjectsConfig`/`ProjectConfigEntry` (slug-only — no D2 overlay surface). Each slug is
validated to `ProjectSlug` at load by REUSING the merged Wave-1 `ProjectSlug::TryFrom`
(seam.rs:71–104, `^[a-z0-9][a-z0-9-]{0,62}$`) — no second validator, no widening, the drifted
issue-#727 `[a-z0-9_-]{0,63}` rejected. Single `RESERVED_SLUGS` list defined here and imported
by the register CLI (no second list). `per_slug_data_dir(base, &ProjectSlug)` is the single
slug→path translation, consumed by both ProjectRouter construction and the register/delete CLI.

## Stage-3a refinement pass

Folded the locked decisions D4/D5/D6 (and the seam-funnel honesty record) into the
already-drafted pseudocode: added the D4 de-register/`--purge`/re-attach state machine and the
`data_exists` genesis branch to `project-registry-cli.md`, the D5 single `RESERVED_SLUGS` source
with the separate-from-charset check, the D6 two-state register branch, and the explicit
Wave-1-discard-elimination / sole-`adapter_for`-dispatch correction to `project-router.md`. D1's
"reuse the merged `TryFrom`" framing was tightened across OVERVIEW and all three component files.

## Open questions flagged (carried to Stage 3b)
- **OQ-CLI-7** (load-bearing, D4 integrity): confirm `Store::open` is non-destructive on an
  existing DB, OR implement the explicit `data_exists`-gated genesis branch.
- **OQ-PR-2/8/9**: name the Wave-2 resolver type so it does not shadow HTTP
  `ProjectRouter<ReqBody>`; thread the default `McpAdapter` into `DefaultResolver`; ensure the
  HTTP `ProjectRouter<ReqBody>` is not reachable as a per-request MCP dispatch fallback once
  `adapter_for` is the sole route.
- **OQ-CFG-1**: confirm `infra/config.rs` importing `ProjectSlug` from `crate::http` does not
  create a dependency cycle; if it does, extract `ProjectSlug` to a leaf module — do NOT
  duplicate the regex.

## Knowledge Stewardship
- **Queried:** `mcp__unimatrix__context_briefing` (Wave-2 pseudocode task scope) +
  `context_search`. Surfaced and applied:
  - **#4963** — build-but-unwireable seam lesson → drove the single-call-site resolver swap and
    the `PathRouter`-holds-`SlugRouter` structural framing (resolver is the sole swap point).
  - **#4958** — `Store` has no `PartialEq`; use `Arc::ptr_eq` idiom → informed the N:1 shared-store
    identity reasoning in the per-slug isolation invariant (shared-store vs distinct-store handles).
  - **ADR #4949 (ADR-005, default alias)** → the `tools`/`/v1/tools/...` shadowing rationale behind
    the D5 reserved-slug check being separate from the charset allowlist.
  - **ADR #4950 (ADR-003, seam)** → the "one trait-impl swap, no interface re-cut; per-slug routing
    lives INSIDE the seam method" invariant — governs the `adapter_for`-inside-the-seam design and
    the untouched grammar/`ProjectKey`/`RouteError`.
  - **ADR #4951 (ADR-004, slug allowlist/no-listing)** → the D1 reuse-the-merged-`TryFrom` decision
    and the D3 no-per-slug-network-health posture (avoids reopening the slug-listing rejection).
- **Stored / Deviations:** Nothing novel stored — the design extends the merged Wave-1 seam per
  ADR-003 and reuses the merged D1 allowlist (`ProjectSlug::TryFrom`) and ADR-005 default alias; no
  new generalizable pattern emerged at the pseudocode stage. No deviations from established patterns.
  (The reusable insight worth storing post-merge is the infra-001 transport-mismatch testing
  pattern, already captured in the test-plan agent reports and OVERVIEW §4; it belongs to the
  tester/leader, not this read-only design agent.)
