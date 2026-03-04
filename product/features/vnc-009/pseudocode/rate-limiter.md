# Pseudocode: rate-limiter

## File: `crates/unimatrix-server/src/services/mod.rs` (additions)

### CallerId enum

```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CallerId {
    Agent(String),
    UdsSession(String),
}
```

### ServiceError::RateLimited variant

```
pub(crate) enum ServiceError {
    // ... existing variants ...
    RateLimited { limit: u32, window_secs: u64, retry_after_secs: u64 },
}

// Display impl addition:
ServiceError::RateLimited { limit, window_secs, retry_after_secs } =>
    write!(f, "rate limited: {limit} requests per {window_secs}s, retry after {retry_after_secs}s")

// From<ServiceError> for ServerError addition:
ServiceError::RateLimited { limit, window_secs, retry_after_secs } =>
    ServerError::InvalidInput {
        field: "rate_limit".to_string(),
        reason: format!("rate limited: {limit} per {window_secs}s, retry after {retry_after_secs}s"),
    }
```

### Session ID helpers

```
/// Prefix a raw session ID with transport identifier.
pub(crate) fn prefix_session_id(transport: &str, raw: &str) -> String {
    format!("{transport}::{raw}")
}

/// Strip transport prefix from a prefixed session ID.
/// Returns the raw ID after the first "::" delimiter.
/// If no prefix found, returns the input unchanged.
pub(crate) fn strip_session_prefix(prefixed: &str) -> &str {
    IF let Some(pos) = prefixed.find("::") THEN
        &prefixed[pos + 2..]
    ELSE
        prefixed
    END IF
}
```

## File: `crates/unimatrix-server/src/services/gateway.rs` (additions)

### RateLimiter struct

```
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Instant;
use crate::services::CallerId;

struct SlidingWindow {
    timestamps: VecDeque<Instant>,
}

pub(crate) struct RateLimiter {
    windows: Mutex<HashMap<CallerId, SlidingWindow>>,
    search_limit: u32,
    write_limit: u32,
    window_secs: u64,
}

impl RateLimiter {
    fn new(search_limit: u32, write_limit: u32, window_secs: u64) -> Self {
        RateLimiter {
            windows: Mutex::new(HashMap::new()),
            search_limit,
            write_limit,
            window_secs,
        }
    }

    /// Check and record a request against the given limit.
    /// Returns Ok(()) if under limit, Err with retry info if over.
    fn check_rate(
        &self,
        caller: &CallerId,
        limit: u32,
    ) -> Result<(), ServiceError> {
        // UdsSession exemption (structural, not conditional)
        IF matches!(caller, CallerId::UdsSession(_)) THEN
            RETURN Ok(())
        END IF

        LET now = Instant::now()
        LET window_duration = Duration::from_secs(self.window_secs)

        // Acquire lock with poison recovery
        LET mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner())

        // Get or create window for this caller
        LET window = windows.entry(caller.clone()).or_insert_with(|| SlidingWindow {
            timestamps: VecDeque::new(),
        })

        // Lazy eviction: remove timestamps older than window
        LET cutoff = now - window_duration
        WHILE let Some(front) = window.timestamps.front() {
            IF *front < cutoff THEN
                window.timestamps.pop_front()
            ELSE
                BREAK
            END IF
        }

        // Check limit
        IF window.timestamps.len() as u32 >= limit THEN
            // Calculate retry_after from oldest remaining timestamp
            LET oldest = window.timestamps.front().unwrap()  // safe: len >= limit > 0
            LET retry_after = window_duration
                .checked_sub(now.duration_since(*oldest))
                .unwrap_or_default()
                .as_secs()
            RETURN Err(ServiceError::RateLimited {
                limit,
                window_secs: self.window_secs,
                retry_after_secs: retry_after,
            })
        END IF

        // Under limit: record this request
        window.timestamps.push_back(now)
        Ok(())
    }
}
```

### SecurityGateway modifications

```
pub(crate) struct SecurityGateway {
    pub(crate) audit: Arc<AuditLog>,
    rate_limiter: RateLimiter,    // NEW
}

impl SecurityGateway {
    pub(crate) fn new(audit: Arc<AuditLog>) -> Self {
        SecurityGateway {
            audit,
            rate_limiter: RateLimiter::new(300, 60, 3600),  // S2 defaults
        }
    }

    /// S2: Check search rate limit for this caller.
    pub(crate) fn check_search_rate(&self, caller: &CallerId) -> Result<(), ServiceError> {
        self.rate_limiter.check_rate(caller, self.rate_limiter.search_limit)
    }

    /// S2: Check write rate limit for this caller.
    pub(crate) fn check_write_rate(&self, caller: &CallerId) -> Result<(), ServiceError> {
        self.rate_limiter.check_rate(caller, self.rate_limiter.write_limit)
    }

    /// Create a permissive gateway for unit tests.
    #[cfg(test)]
    pub(crate) fn new_permissive() -> Self {
        // ... existing tempdir/store/audit setup ...
        SecurityGateway {
            audit: Arc::new(audit),
            rate_limiter: RateLimiter::new(u32::MAX, u32::MAX, 3600),  // effectively unlimited
        }
    }
}
```

## File: `crates/unimatrix-server/src/services/search.rs` (modifications)

### Add caller_id parameter to search

```
pub(crate) async fn search(
    &self,
    params: ServiceSearchParams,
    audit_ctx: &AuditContext,
    caller_id: &CallerId,          // NEW
) -> Result<SearchResults, ServiceError> {
    // NEW: S2 rate check before any work
    self.gateway.check_search_rate(caller_id)?;

    // ... existing search pipeline unchanged ...
}
```

## File: `crates/unimatrix-server/src/services/store_ops.rs` (modifications)

### Add caller_id parameter to insert and correct

```
pub(crate) async fn insert(
    &self,
    entry: NewEntry,
    embedding: Option<Vec<f32>>,
    audit_ctx: &AuditContext,
    caller_id: &CallerId,          // NEW
) -> Result<InsertResult, ServiceError> {
    // NEW: S2 rate check before any work
    self.gateway.check_write_rate(caller_id)?;

    // ... existing insert pipeline unchanged ...
}
```

In `store_correct.rs`:
```
pub(crate) async fn correct(
    &self,
    original_id: u64,
    corrected: NewEntry,
    reason: Option<String>,
    audit_ctx: &AuditContext,
    caller_id: &CallerId,          // NEW
) -> Result<CorrectResult, ServiceError> {
    // NEW: S2 rate check before any work
    self.gateway.check_write_rate(caller_id)?;

    // ... existing correct pipeline unchanged ...
}
```

## File: `crates/unimatrix-server/src/services/briefing.rs` (modifications)

### Add optional caller_id parameter to assemble

```
pub(crate) async fn assemble(
    &self,
    params: BriefingParams,
    audit_ctx: &AuditContext,
    caller_id: Option<&CallerId>,   // NEW: optional for backward compat
) -> Result<BriefingResult, ServiceError> {
    // ... existing S3 validation ...

    // NEW: S2 rate check when semantic search is active
    IF params.include_semantic THEN
        IF let Some(cid) = caller_id THEN
            self.gateway.check_search_rate(cid)?;
        END IF
    END IF

    // ... existing briefing pipeline unchanged ...
}
```

## Open Questions

None. All design decisions are resolved via ADRs and SCOPE.md.
