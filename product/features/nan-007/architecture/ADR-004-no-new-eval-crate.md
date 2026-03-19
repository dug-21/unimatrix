## ADR-004: Eval Infrastructure Lives in unimatrix-server, Not a New Crate

### Context

ASS-025 proposed a possible `crates/unimatrix-eval/` crate to separate eval
infrastructure from the server runtime. A separate crate would need to depend on
`unimatrix-server` for `ServiceLayer`, but `unimatrix-server` is the binary crate —
a library crate depending on it creates a circular or inverted dependency.

Three options were evaluated:

**Option A — New `crates/unimatrix-eval/` crate**: Eval logic lives in a new library
crate. Requires either extracting `ServiceLayer` into a separate library crate
(significant refactor) or creating a library facade in `unimatrix-server` (non-standard
pattern). Also creates a new workspace member that must be maintained.

**Option B — Module tree inside `unimatrix-server`**: Eval logic lives in
`crates/unimatrix-server/src/eval/` as a module tree. Same crate as `export.rs`,
`import.rs`, `test_support.rs`. Consistent with the "single binary" principle stated
in SCOPE.md.

**Option C — Separate binary crate `unimatrix-eval`**: A second binary that shares
code via a shared library. Two binaries in the workspace would split the distribution
concern and complicate the npm packaging.

### Decision

Module tree inside `unimatrix-server` (Option B).

The single-binary principle is a project-wide non-negotiable (stated in SCOPE.md
Constraints). Both `export.rs` and `test_support.rs` establish the precedent of
non-server functionality living in the server crate. The eval engine's `ServiceLayer`
dependency is satisfied naturally because it is in the same crate. No new workspace
member, no circular dependency, no distribution complexity.

The module tree is:
```
crates/unimatrix-server/src/
  snapshot.rs
  eval/
    mod.rs
    profile.rs
    scenarios.rs
    runner.rs
    report.rs
```

### Consequences

- No new workspace member. Cargo.lock, CI, and the npm packaging script are unchanged.
- `eval/` module can freely access `crate::services::ServiceLayer`, `crate::infra::*`,
  and `crate::test_support::*` without any visibility boundary changes.
- The `unimatrix-server` crate grows by ~800–1200 lines. This is acceptable given the
  existing size and the cumulative nature of the codebase.
- If eval infrastructure were ever to be extracted to a separate crate in the future,
  the module boundary is already clean and the extraction would be mechanical.
