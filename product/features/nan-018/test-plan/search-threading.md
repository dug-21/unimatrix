# Test Plan — Search threading (`SearchService` field + `with_rate_config`)

**Component**: `crates/unimatrix-server/src/services/search.rs` — `SearchService.graph_penalty_params` field; resolved in `with_rate_config`; the two penalty-application sites (`:727` fallback branch, `:729` `graph_penalty` → `graph_penalty_with`).
**Wave**: 1. **Primary risks**: R-01 (missed site → default shift, **Critical**), R-13 (multiplier resolution/precedence).

## Unit / integration test expectations

### R-01 — enumerated-site grep guard (SOURCE OF TRUTH for AC-01 bit-for-bit) — AC-01(c)

- `test_enumerated_penalty_sites_route_through_config`: a grep-style guard (mirroring the #4070 procedure) asserting **every** `graph_penalty` reference in the production path routes through `graph_penalty_with` / the resolved field. Specifically:
  - `search.rs:729` calls `graph_penalty_with(.., &self.graph_penalty_params)` — NOT bare `graph_penalty`/const.
  - `search.rs:727` fallback branch uses the **resolved `fallback`** field — NOT the `FALLBACK_PENALTY` const.
  - `with_rate_config` resolves `graph_penalty_params` from config.
- `test_background_rs_not_a_penalty_site`: assert `background.rs:583` is a `tracing::error!` **log string only** — it is **excluded** from the threading targets and must NOT read penalty config. (Guards against the WARN-1 false-positive of threading config into a log line.) Implement as a grep guard: `background.rs` contains the const **only** inside a log-string context, no `graph_penalty`/`penalty_map` use.

### R-01 — config-resolution-level default-equivalence — AC-01

- `test_with_rate_config_default_resolves_to_const_params`: a `SearchService` built from a default `UnimatrixConfig` has `graph_penalty_params == GraphPenaltyParams::default()` (every field = const). The config-resolution-level proof of bit-for-bit (complements the engine-level proof in `engine-penalty.md`).
- `test_search_default_config_identical_results`: at default config, `search()` over a fixture produces byte-identical scored output to the pre-nan-018 path (the empty-TOML equivalence at the **service** level — closes the R-01 coverage requirement: engine + config-resolution + empty-TOML).

### R-13 — multiplier resolution + precedence — AC-01

- `test_with_rate_config_multiplier_scales_severities_only`: `multiplier = Some(m)` ⇒ resolved `orphan/clean_replacement/partial_supersession/dead_end/fallback` scaled per the ADR-001 transform; resolved `hop_decay` and `max_traversal_depth` **unchanged**.
- `test_with_rate_config_per_field_override_wins_over_multiplier`: both `multiplier` and an explicit per-field override set on the same lever ⇒ the **per-field value wins** (multiplier is a convenience overlay, never a replacement).
- `test_with_rate_config_multiplier_none_is_noop`: `multiplier = None` ⇒ resolved params equal exactly the per-field (or default) values — no scaling (subsumes R-01 default-equivalence when all fields default).

## Concrete assertions
- The fallback branch at `:727` must read `self.graph_penalty_params.fallback`, asserted by inspecting the resolved field, not by output coincidence.
- Sweep observability: `test_swept_lever_changes_search_score` — a non-default `clean_replacement` produces a measurably different score for a depth-2 chain entry vs the default profile (feeds AC-14 condition 4, lever-is-live).

## Boundary note
This is the **engine ↔ server boundary** seam (R-01 integration risk): engine const = source of truth, server config mirrors it, `SearchService` carries the resolved copy. A site missed here silently shifts every downstream baseline — the most-failed class (#4044, #2730, #4013, #4070). The MCP `tools`/`lifecycle` suites (OVERVIEW §4) are the MCP-level backstop that default-config search is unperturbed.
