# Component: Search threading — `search-threading.md`

**Wave**: 1
**Location**: `crates/unimatrix-server/src/services/search.rs` (modify)
**ADR**: ADR-001 (#4897). **Risks**: R-01 (critical — the two-site enumeration).

## Purpose

Carry the resolved `GraphPenaltyParams` on `SearchService`, resolved once in `with_rate_config`,
and apply it at the **two and only two** penalty-application sites in the Flexible loop
(`search.rs:727`, `:729`). Default config ⇒ bit-for-bit identical penalties (NFR-01).

## The two sites (LOAD-BEARING — R-01, the enumerated-site guard)

Current `services/search.rs:724-733`:

```
for (entry, _) in &results_with_scores {
    if entry.superseded_by.is_some() || entry.status == Status::Deprecated {
        let penalty = if use_fallback {
            FALLBACK_PENALTY                                   // :727  <-- SITE 1
        } else {
            graph_penalty(entry.id, &typed_graph, &all_entries) // :729  <-- SITE 2
        };
        penalty_map.insert(entry.id, penalty);
    }
}
```

After nan-018:

```
for (entry, _) in &results_with_scores {
    if entry.superseded_by.is_some() || entry.status == Status::Deprecated {
        let penalty = if use_fallback {
            self.graph_penalty_params.fallback                 // :727  SITE 1 -> resolved fallback
        } else {
            graph_penalty_with(entry.id, &typed_graph, &all_entries, &self.graph_penalty_params)  // :729 SITE 2
        };
        penalty_map.insert(entry.id, penalty);
    }
}
```

- The `use_fallback` predicate and loop condition are UNCHANGED — only the value source swaps.
- `background.rs:583` is NOT touched. Its `FALLBACK_PENALTY` reference is a `tracing::error!` log
  string ("...search using FALLBACK_PENALTY"); it applies no penalty and reads no value. Threading
  config there would be a false positive against the R-01 grep guard.

## New field on `SearchService`

```
pub struct SearchService {
    // ... existing scalar config fields, e.g. ppr_alpha: f64 ...
    pub graph_penalty_params: GraphPenaltyParams,   // resolved once; Copy
}
```
Rides the existing config-plumbing the same way `ppr_alpha` does (Integration Surface).

## Resolution in `with_rate_config`

`with_rate_config` already receives the config (`Arc<InferenceConfig>` etc.). Resolve the params
ONCE here from `config.graph_penalty` (the `GraphPenaltyConfig` from `penalty-config.md`):

```
fn with_rate_config(.., config: &UnimatrixConfig, ..) -> SearchService {
    // ... existing field resolution (ppr_alpha, etc.) ...
    let graph_penalty_params = config.graph_penalty.resolve_params();   // multiplier + overrides applied here
    SearchService {
        // ... existing ...
        graph_penalty_params,
    }
}
```

- Resolution (multiplier overlay, per-field precedence) happens in `resolve_params()`
  (`penalty-config.md`), NOT here — `with_rate_config` just calls it and stores the result.
- `eval/profile/layer.rs` already forwards the whole config override to `with_rate_config`
  (Integration Surface, `layer.rs:363-385`) — there is NO eval-specific threading site. A swept
  profile TOML's `[graph_penalty]` flows through the existing path automatically.

## Enumerated construction/forwarding sites (R-01 — assert each reads config)

1. `infra/config.rs` — `GraphPenaltyConfig` field + `Default` + serde `default_*()` fns (`penalty-config.md`).
2. `services/search.rs` — `SearchService.graph_penalty_params` field; `with_rate_config` resolution;
   Flexible-loop call at :729; fallback branch at :727.
3. `eval/profile/layer.rs` — already forwards `config_overrides` whole; no new site.

A grep-style guard test (mirrors the #4070 procedure): every `graph_penalty\b` reference in
non-test code must route through `graph_penalty_with` or the resolved `graph_penalty_params` field
— no bare `graph_penalty(` call and no bare `FALLBACK_PENALTY` application outside `background.rs`'s
log string.

## Data flow

- **Input**: `UnimatrixConfig.graph_penalty` (via `with_rate_config`).
- **Stored**: `SearchService.graph_penalty_params: GraphPenaltyParams`.
- **Output**: applied per-entry into `penalty_map` at the two sites; flows into the existing score path.

## Error handling

No new fallible path here — resolution is infallible (validation already happened at config load,
`penalty-config.md`). `graph_penalty_with` is pure and total.

## Key test scenarios

- **Enumerated-site assertion (R-01.2)**: assert each of the sites above reads threaded config,
  not the module const. Include the grep-guard test.
- **Empty-TOML byte-identity (R-01.3, NFR-02)**: a profile omitting `[graph_penalty]` produces a
  `penalty_map` byte-identical to the pre-nan-018 binary across all five fixture shapes.
- **Sweep delta (FR-04, AC-01b)**: two profiles differing in `clean_replacement` produce the
  predicted penalty delta in the resulting scores (and the report).
- **Fallback-branch value (R-01.2)**: with `use_fallback` true, assert the inserted penalty equals
  `self.graph_penalty_params.fallback` (default ⇒ `FALLBACK_PENALTY = 0.70`).
- **Existing eval profile suite green (R-01.4)**: full existing suite passes unchanged.
- **`background.rs` untouched**: grep guard asserts `background.rs` has no `graph_penalty_with` /
  no penalty-application threading — only the pre-existing log string.
