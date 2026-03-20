//! NLI service handle — lazy-loading state machine for the NLI cross-encoder.
//!
//! This is a compile-time placeholder. The full implementation ships in Wave 2
//! of crt-023 (`NliServiceHandle`, state machine Loading → Ready | Failed → Retrying,
//! SHA-256 hash verification, mutex poison detection).
//!
//! Wave 1 exposes this module so other Wave 1 components (`config.rs`, `error.rs`)
//! can reference the module path without compilation failures.
