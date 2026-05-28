## ADR-002: No Provenance Summary Line in log_config_provenance

### Context

OQ-03 asks whether `log_config_provenance` should log a single summary line after the individual source lines (e.g., "effective config: per-project primary, global defaults applied").

Currently, `log_config_provenance` logs one line per source (global, project, env_override). The effective config is already logged separately at main.rs:1280 (`"config loaded"` with preset). Adding a summary line would duplicate information already derivable from the individual lines.

### Decision

Do not add a summary line to `log_config_provenance`. The updated individual source labels ("primary config loaded (per-project)", "defaults config loaded (global)") are sufficient. The effective config preset is already logged at the call site.

### Consequences

- **Easier**: Less log noise. Each provenance line carries its own hierarchy label — no redundancy.
- **Easier**: The function remains a simple match-and-log with no state aggregation.
- **Harder**: Operators who want a single "what won?" line must read two log lines instead of one. This is acceptable given the labels clearly indicate primacy.
