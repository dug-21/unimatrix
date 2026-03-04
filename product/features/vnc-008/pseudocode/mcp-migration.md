# Pseudocode: mcp-migration

## Purpose
Move MCP transport modules (`tools.rs`, `identity.rs`) into `src/mcp/`, create `mcp/mod.rs` with re-exports.

## Files Created
- `src/mcp/mod.rs`

## Files Moved
- `src/tools.rs` -> `src/mcp/tools.rs`
- `src/identity.rs` -> `src/mcp/identity.rs`

## Files Modified
- `src/lib.rs`

## Pseudocode

### src/mcp/mod.rs

```
// MCP transport layer modules.

pub mod context;       // ToolContext (created in tool-context component)
pub mod identity;
pub mod response;      // Response formatting (created in response-split component)
pub mod tools;
```

Note: `context` and `response` sub-modules are created by their respective components. During mcp-migration, only `identity` and `tools` are available. The mod.rs must be created with all four declarations since response-split and tool-context happen in the same wave.

### src/lib.rs changes

Replace `pub mod tools;` and `pub mod identity;` with `pub mod mcp;`.

Add temporary re-exports:
```
pub use mcp::tools;
pub use mcp::identity;
```

### Import updates in mcp/tools.rs

Update internal imports from:
```
use crate::response::*;
use crate::identity::*;
use crate::uds_listener::{run_confidence_consumer, ...};
```

To:
```
use crate::mcp::response::*;
use crate::mcp::identity::*;
use crate::uds_listener::{run_confidence_consumer, ...};  // stays until uds-migration
```

Note: `tools.rs` imports `run_confidence_consumer` and `run_retrospective_consumer` and `write_signals_to_queue` from `uds_listener`. These functions are used in the `context_status` maintain path. After StatusService extraction, these imports move to `services/status.rs`. During migration, the import path updates to `crate::uds::listener::*`.

### Import updates in mcp/identity.rs

Update from:
```
use crate::error::ServerError;
use crate::registry::{AgentRegistry, Capability, TrustLevel};
```

To:
```
use crate::error::ServerError;
use crate::infra::registry::{AgentRegistry, Capability, TrustLevel};
```

## Compilation Gate

After this step: `cargo check --workspace` must succeed.
