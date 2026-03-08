# Pseudocode: server-e2e-tests

## Purpose

Full SearchService pipeline tests with real ONNX embeddings. Server-level validation of ranking behavior.

## File: src/test_support.rs

Feature-gated: `#[cfg(any(test, feature = "test-support"))]`

```
// Re-export internal types for integration tests
pub use crate::services::{
    ServiceLayer, SearchService, ServiceSearchParams,
    RetrievalMode, CallerId, AuditContext, AuditSource,
    SecurityGateway,
};

pub struct TestServiceLayer;

impl TestServiceLayer {
    pub async fn new(store_path: &Path) -> Option<ServiceLayer>
        // Check ONNX model existence first
        // Open Store at path
        // Create VectorIndex (empty)
        // Create StoreAdapter, VectorAdapter
        // Create AsyncEntryStore, AsyncVectorStore
        // Load EmbedServiceHandle (real ONNX)
        // Create AdaptationService default
        // Create AuditLog
        // Create UsageDedup default
        // Construct ServiceLayer::new(...)
        // Return Some(layer) or None if model missing
}

pub fn skip_if_no_model() -> bool
    // Check standard model paths
    // Return true if should skip
```

## File: tests/pipeline_e2e.rs

```
use unimatrix_server::test_support::*;

// T-E2E-skip: Skip with message when model absent
// Each test calls skip_if_no_model() at start

// T-E2E-01: Active above deprecated
#[tokio::test]
async fn test_active_above_deprecated()
    if skip_if_no_model() { return; }
    // Store active entry about "error handling in Rust"
    // Store deprecated entry about "error handling patterns"
    // Search for "error handling"
    // Assert active ranks above deprecated

// T-E2E-02: Supersession injection
#[tokio::test]
async fn test_supersession_injection()
    // Store A (deprecated, superseded_by=B) + B (active)
    // Search for content matching A
    // Assert B appears in results

// T-E2E-03: Provenance boost
#[tokio::test]
async fn test_provenance_boost()
    // Store lesson-learned + convention with similar content
    // Search, assert lesson-learned ranks above

// T-E2E-04: Co-access boost
#[tokio::test]
async fn test_co_access_boost()
    // Store 3 entries, record co-access between 1 and 2
    // Search matching entry 1
    // Assert entry 2 ranks higher than entry 3

// T-E2E-05: Golden regression
#[tokio::test]
async fn test_golden_regression()
    // Fixed scenario with known entries + query
    // Assert exact top-3 IDs

// T-TSL-01: TestServiceLayer constructs
#[tokio::test]
async fn test_service_layer_construction()
    if skip_if_no_model() { return; }
    // Verify construction succeeds with valid store path
```

## Cargo.toml Changes

```toml
[features]
default = ["mcp-briefing"]
mcp-briefing = []
test-support = ["unimatrix-store/test-support", "unimatrix-engine/test-support"]

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
unimatrix-server = { path = ".", features = ["test-support"] }
```

## lib.rs Changes

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
```
