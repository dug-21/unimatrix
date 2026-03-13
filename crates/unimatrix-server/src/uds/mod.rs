//! UDS transport layer modules.
//!
//! Contains the Unix domain socket listener for hook IPC
//! and the hook subcommand handler.

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

/// Check if UDS connections have a specific capability.
pub(crate) fn uds_has_capability(cap: Capability) -> bool {
    UDS_CAPABILITIES.contains(&cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uds_capabilities_exact_set() {
        assert_eq!(
            UDS_CAPABILITIES,
            &[
                Capability::Read,
                Capability::Search,
                Capability::SessionWrite
            ]
        );
    }

    #[test]
    fn test_uds_has_capability_read() {
        assert!(uds_has_capability(Capability::Read));
    }

    #[test]
    fn test_uds_has_capability_search() {
        assert!(uds_has_capability(Capability::Search));
    }

    #[test]
    fn test_uds_has_capability_session_write() {
        assert!(uds_has_capability(Capability::SessionWrite));
    }

    #[test]
    fn test_uds_has_capability_write_false() {
        assert!(!uds_has_capability(Capability::Write));
    }

    #[test]
    fn test_uds_has_capability_admin_false() {
        assert!(!uds_has_capability(Capability::Admin));
    }
}
