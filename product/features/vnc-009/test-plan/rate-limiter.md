# Test Plan: rate-limiter

## Risk Coverage

| Risk | Scenarios | Priority |
|------|----------|----------|
| R-02 | Mutex contention | Low |
| R-06 | Eviction correctness | Medium |
| R-07 | Briefing rate interaction | Medium |
| R-09 | UDS exemption | Medium |

## Unit Tests (services/gateway.rs)

### R-06: Rate Limiter Eviction Correctness

1. **test_check_search_rate_allows_under_limit**
   - Create RateLimiter with search_limit=300
   - Make 300 calls with same CallerId::Agent
   - Verify all return Ok

2. **test_check_search_rate_rejects_over_limit**
   - Create RateLimiter with search_limit=300
   - Make 301 calls with same CallerId::Agent
   - Verify 301st returns Err(RateLimited)
   - Verify RateLimited has limit=300, window_secs=3600

3. **test_check_write_rate_allows_under_limit**
   - Create RateLimiter with write_limit=60
   - Make 60 calls with same CallerId::Agent
   - Verify all return Ok

4. **test_check_write_rate_rejects_over_limit**
   - Create RateLimiter with write_limit=60
   - Make 61 calls with same CallerId::Agent
   - Verify 61st returns Err(RateLimited)

5. **test_rate_limiter_different_callers_independent**
   - Agent "alice" makes 300 searches -> all Ok
   - Agent "bob" makes 1 search -> Ok (separate window)

6. **test_rate_limiter_lazy_eviction**
   - Use short window (e.g., 1 second) for testability
   - Make `limit` calls
   - Sleep 1.1 seconds (past window)
   - Next call succeeds (expired entries evicted)
   - Covers: R-06 scenario 1

7. **test_rate_limiter_partial_eviction**
   - Use short window (2 seconds)
   - Make 5 calls at T=0
   - Sleep 1 second
   - Make 5 calls at T=1
   - Sleep 1.1 seconds (T=2.1)
   - Only T=0 calls expired, T=1 calls remain
   - Window has 5 entries, limit allows more
   - Covers: R-06 scenario 3

### R-09: UDS Exemption

8. **test_check_search_rate_uds_exempt**
   - Create RateLimiter with search_limit=1 (very low)
   - CallerId::UdsSession("any") makes 1000 calls
   - All return Ok
   - Covers: R-09 scenarios 1-2

9. **test_check_write_rate_uds_exempt**
   - Same as above with check_write_rate
   - CallerId::UdsSession makes unlimited calls -> all Ok

10. **test_rate_limit_agent_not_exempt**
    - CallerId::Agent("bot") with limit=1
    - 2nd call returns RateLimited
    - Covers: R-09 scenario 3

### R-02: Mutex Contention (Low Priority)

11. **test_rate_limiter_concurrent_access**
    - Spawn 10 threads, each makes 10 check_search_rate calls
    - Verify no panics, correct final count

### ServiceError::RateLimited

12. **test_service_error_rate_limited_display**
    - Create ServiceError::RateLimited { limit: 300, window_secs: 3600, retry_after_secs: 42 }
    - Verify Display output contains relevant info

13. **test_service_error_rate_limited_to_server_error**
    - Convert RateLimited to ServerError
    - Verify maps correctly

### CallerId Tests (services/mod.rs)

14. **test_caller_id_debug_clone_eq_hash**
    - Verify derives work: Debug, Clone, PartialEq, Eq, Hash
    - CallerId::Agent("a") == CallerId::Agent("a")
    - CallerId::Agent("a") != CallerId::UdsSession("a")

### Session ID Helpers (services/mod.rs)

15. **test_prefix_session_id_mcp**
    - prefix_session_id("mcp", "abc") == "mcp::abc"

16. **test_prefix_session_id_uds**
    - prefix_session_id("uds", "sess-123") == "uds::sess-123"

17. **test_strip_session_prefix_mcp**
    - strip_session_prefix("mcp::abc") == "abc"

18. **test_strip_session_prefix_uds**
    - strip_session_prefix("uds::sess-123") == "sess-123"

19. **test_strip_session_prefix_no_prefix**
    - strip_session_prefix("raw-id") == "raw-id"

20. **test_strip_session_prefix_empty_after_prefix**
    - strip_session_prefix("mcp::") == ""

21. **test_strip_session_prefix_empty_input**
    - strip_session_prefix("") == ""

22. **test_strip_session_prefix_nested_delimiter**
    - strip_session_prefix("mcp::nested::value") == "nested::value"

## Test Setup Pattern

Rate limiter tests use short windows (1-2 seconds) to avoid long sleeps.
SecurityGateway::new_permissive() updated to use u32::MAX limits.
New helper: `SecurityGateway::new_test_limited(search_limit, write_limit, window_secs)`.
