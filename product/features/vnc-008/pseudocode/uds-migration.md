# Pseudocode: uds-migration

## Purpose
Move UDS transport modules into `src/uds/`, create `uds/mod.rs` with re-exports and UDS_CAPABILITIES constant.

## Files Created
- `src/uds/mod.rs`

## Files Moved
- `src/uds_listener.rs` -> `src/uds/listener.rs`
- `src/hook.rs` -> `src/uds/hook.rs`

## Files Modified
- `src/lib.rs`

## Pseudocode

### src/uds/mod.rs

```
// UDS transport layer modules.

use crate::infra::registry::Capability;

pub mod hook;
pub mod listener;

/// Fixed capabilities for UDS connections. Not configurable at runtime.
/// UDS connections can read, search, and perform session-scoped writes.
/// They cannot perform knowledge writes (Write) or admin operations (Admin).
pub(crate) const UDS_CAPABILITIES: &[Capability] = &[
    Capability::Read,
    Capability::Search,
    Capability::SessionWrite,
];
```

Note: `SessionWrite` does not exist yet at this step. `UDS_CAPABILITIES` is added as part of the session-write component. During uds-migration, the mod.rs only has the module declarations.

Revised uds/mod.rs during uds-migration step:
```
pub mod hook;
pub mod listener;
```

The UDS_CAPABILITIES constant is added in the session-write component.

### src/lib.rs changes

Replace `pub mod uds_listener;` and `pub mod hook;` with `pub mod uds;`.

Add temporary re-exports:
```
pub use uds::listener as uds_listener;
pub use uds::hook as hook;
```

### Import updates in uds/listener.rs

Module-level rename: file was `uds_listener.rs`, now `uds/listener.rs`. No content changes needed for the file itself except internal imports.

Update imports from:
```
use crate::embed_handle::EmbedServiceHandle;
use crate::server::PendingEntriesAnalysis;
use crate::session::{...};
```

To:
```
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::server::PendingEntriesAnalysis;
use crate::infra::session::{...};
```

### Import updates in uds/hook.rs

No internal crate imports (hook.rs uses unimatrix_engine crates directly). No changes needed.

### Cross-module import updates

`mcp/tools.rs` currently imports:
```
use crate::uds_listener::{run_confidence_consumer, run_retrospective_consumer, write_signals_to_queue};
```

Update to:
```
use crate::uds::listener::{run_confidence_consumer, run_retrospective_consumer, write_signals_to_queue};
```

These imports will later move to `services/status.rs` during StatusService extraction.

## Compilation Gate

After this step: `cargo check --workspace` must succeed.
