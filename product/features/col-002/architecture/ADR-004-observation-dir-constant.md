## ADR-004: Observation Directory as Constant

### Context

The observation directory (`~/.unimatrix/observation/`) stores per-session JSONL files. This path must be known by both the hook scripts (which write) and the analysis engine (which reads). Three options:
1. Compile-time constant in the observe crate, mirrored in hook scripts.
2. Runtime configuration via config file or environment variable.
3. Passed as a parameter to every function.

The SCOPE.md states: "Observation directory is a constant. `~/.unimatrix/observation/` is defined as a constant, not configurable. Future configurability is a one-line change (constant to config lookup)."

### Decision

Option 1 + 3 hybrid. The observe crate defines a `DEFAULT_OBSERVATION_DIR` constant that resolves `~/.unimatrix/observation/`. All public functions accept the observation directory as a `&Path` parameter (option 3), making them testable with temp directories. The server crate passes the resolved default path. Hook scripts hardcode the same path.

This allows test code to use temp directories while production code uses the constant.

### Consequences

- **Easier**: No config infrastructure needed. Tests use temp directories cleanly. Future configurability requires changing one call site in the server.
- **Harder**: If a user wants a custom observation directory, they must rebuild. Hook scripts and the Rust crate must agree on the path (coordination by convention, not by shared config).
