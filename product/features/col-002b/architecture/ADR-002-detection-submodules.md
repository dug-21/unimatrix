## ADR-002: Detection Rules as Submodules

### Context

col-002 ships 3 detection rules in `detection.rs` (approximately 150-200 lines). col-002b adds 18 more. At ~50-80 lines per rule (struct definition, threshold constant, detect implementation, evidence collection), the total would be 1000-1700 lines in a single file.

The project convention favors focused modules. The col-002 observe crate already separates parser, attribution, metrics, report, files, and types into distinct modules.

### Decision

Refactor `detection.rs` into a module directory `detection/`:

```
detection/
  mod.rs          — DetectionRule trait, detect_hotspots(), default_rules(), HotspotCategory, Severity
  agent.rs        — 7 agent hotspot rules
  friction.rs     — 2 new + 2 existing friction rules (permission retries, sleep workarounds from col-002)
  session.rs      — 4 new + 1 existing session rule (session timeout from col-002)
  scope.rs        — 5 scope hotspot rules
```

col-002's existing 3 rules move into their respective category files (permission retries and sleep workarounds into `friction.rs`, session timeout into `session.rs`). `mod.rs` re-exports the trait, engine function, and default rules list.

### Consequences

- **Easier**: Each category file is self-contained (200-500 lines). Adding a new rule means editing one category file and adding to `default_rules()`. Code review is focused — a PR that adds agent rules touches only `agent.rs`.
- **Harder**: col-002's `detection.rs` must be restructured into a directory. This is a one-time refactor that col-002b performs as its first step. Import paths change from `crate::detection::DetectionRule` to `crate::detection::DetectionRule` (same, since mod.rs re-exports).
