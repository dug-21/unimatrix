## ADR-004: The global serve-time seed is gated structurally by `if config.http.enabled`, not the `base_dir` argument; the local path writes zero files

### Context
SCOPE Goal 1 / AC-01 / OQ-4 (RESOLVED, container only): a fresh container `serve` must
write the annotated global `config.toml` (file (a)) at the path-hash path when absent —
closing the #783 devex gap (today only `handle_version`, i.e. `init`/`version`, writes it;
`serve` only READS it via `load_config_and_build_allowlist`). AC-06 is the regression
sentinel: a local STDIO / single-project `serve` forces NO new file.

SR-04 (High) and the SCOPE Background assume container-vs-local is decided STRUCTURALLY by
`ensure_data_directory`'s `base_dir` arg (`Some(/data)` = container, `None` = local), and
recommend gating the seed on that arg. **Code inspection contradicts this assumption.**
Every `ensure_data_directory` call in the live `serve` path passes `base_dir = None`
(main.rs:599 daemon, :1347 stdio, :1779, :529, :546). Container-vs-local is actually
decided by `dirs::home_dir()` inside `ensure_data_directory` (project.rs:153-160): in the
container HOME=`/data`, so `unimatrix_base = /data/.unimatrix`; locally HOME=`~`. The
`base_dir = Some(...)` form is used only by tests, `health.rs`, `snapshot.rs`, `export.rs`.
So a global seed keyed on the `base_dir` argument at the serve call site would NEVER fire —
it is always `None` there. The risk assessment's recommended discriminator is not present
in the running serve path.

The real structural seam already exists: `if config.http.enabled` (main.rs:1011) gates the
entire HTTP listener + multi-project (`[[projects]]`) block. HTTP is the container/cloud
transport; local STDIO/UDS runs in the `else` branch. `require_http_for_projects`
(main.rs:670) already ties registered projects to `http.enabled`. This is the container
seam.

### Decision
The global seed for (a) lives INSIDE the `if config.http.enabled` block in
`tokio_main_daemon` (main.rs:1011), as a structural compile-time branch — NOT a runtime
flag and NOT keyed on the `base_dir` argument.

- On `serve` with `config.http.enabled == true` (the container/HTTP path): call
  `write_default_config_if_absent(&paths.data_dir.join("config.toml"), false)` BEFORE (or
  at the head of) the per-slug loop. The path is exactly `paths.data_dir.join("config.toml")`
  — the same (a) path `handle_version` writes and `load_config_and_build_allowlist` reads,
  resolved by `ensure_data_directory` (`/data/.unimatrix/<hash>/config.toml` in the
  container). Skip-if-exists (AC-01, AC-05) is guaranteed by the `create_new` primitive
  (ADR-001), so a subsequent boot with the file present does not overwrite it.
- On `serve` with `config.http.enabled == false` (local STDIO/UDS, the `else` branch): NO
  seed write — zero new files (AC-06). The seed call simply does not exist on that branch,
  so the regression sentinel holds by branch placement, not by a guard the gate must catch.

The global seed reuses `write_default_config_if_absent` with `DEFAULT_CONFIG_TOML` verbatim
(NOT the classification-rendered per-slug body — the global file is the daemon's own config,
not a per-slug overlay; its annotations are the existing template's).

### Consequences
- Easier: "container only" is a compile-time branch fact — the local path literally cannot
  reach the seed call, so AC-06 (local writes zero files) holds by construction, not by a
  gate spotting a runtime regression (the SR-04 forcing function the risk assessment asked
  for, expressed against the seam that actually exists).
- Easier: the global seed is the already-tested `write_default_config_if_absent` with the
  existing template; the only new behavior is a second caller on the HTTP branch.
- CORRECTION recorded (de-risks SR-04 misimplementation): do NOT gate on
  `ensure_data_directory(base_dir = Some(/data))` at the serve call site — that arg is
  `None` in serve and the seed would never fire. Gate on `config.http.enabled`.
- Cost: the seed reads/writes the path-hash (a) file at serve start; `ensure_data_directory`
  already created the dir, and `create_new` is a single syscall on the present-file path —
  negligible. It runs once per boot.
- Note (coexistence with handle_version): `handle_version` still writes (a) for
  `init`/`version`. serve's seed is the same function on the same path with skip-if-exists,
  so the two never conflict — whichever runs first wins, the other no-ops.
- Cross-references ADR-001 (the reused no-clobber primitive), SCOPE AC-01/AC-06.
