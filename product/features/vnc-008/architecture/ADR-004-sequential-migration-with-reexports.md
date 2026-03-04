## ADR-004: Sequential Migration with Temporary Re-Exports

### Context

Moving 20+ modules simultaneously changes every `use crate::*` import path in the crate. SR-03 flagged this as high severity / medium likelihood — a single massive rename commit creates high merge conflict risk and is difficult to review or bisect.

Options considered:
1. **Big bang**: Move everything in one commit. Simple but high risk. Unreviewable diff. If it breaks, hard to find the cause.
2. **Sequential with re-exports**: Move modules in groups, adding temporary `pub use` re-exports from old paths. Each step compiles independently. Re-exports removed in final step.
3. **Sequential without re-exports**: Move modules in groups, update all imports immediately. Each step requires touching every file that imports from the moved module.

### Decision

Sequential migration with temporary re-exports (option 2), in 5 steps:

1. **Create `infra/` and move infrastructure modules** — Add `pub mod infra;` to lib.rs. Move 13 modules. Add re-exports: `pub use infra::audit;` etc. in lib.rs for backward compatibility.
2. **Create `mcp/` and move MCP modules** — Move tools.rs, identity.rs. Split response.rs into `mcp/response/`. Create `mcp/context.rs`.
3. **Create `uds/` and move UDS modules** — Move uds_listener.rs -> `uds/listener.rs`. Move hook.rs.
4. **Add SessionWrite + StatusService** — Add capability variant, wire UDS capabilities, extract StatusService, integrate ToolContext into handlers.
5. **Clean up re-exports** — Remove temporary re-exports from lib.rs. Update any remaining old-path imports.

Each step produces a compilable, testable crate. If a step introduces a regression, `git bisect` can isolate it to one of 5 commits rather than one massive commit.

### Consequences

- 5 commits instead of 1 — more granular but more commit churn
- Temporary re-exports (e.g., `pub use infra::audit as audit;`) exist between steps 1 and 5 — they look like dead code but serve migration purposes
- Step 5 is a "cleanup" commit that removes re-exports — easy to review (just deletions)
- Each step can be reviewed independently
- Merge conflict surface is reduced: each step touches a smaller set of files
- Integration tests that `use unimatrix_server::response::*` work through re-exports until step 5
