# Pseudocode: C1 — Rust UNIMATRIX_TICK_INTERVAL_SECS env var

## File
`crates/unimatrix-server/src/background.rs`

## Purpose
Replace the hardcoded `const TICK_INTERVAL_SECS: u64 = 900` with a runtime read of the
`UNIMATRIX_TICK_INTERVAL_SECS` environment variable. Falls back to 900 on any error.

## New Function

```
fn read_tick_interval() -> u64:
    match std::env::var("UNIMATRIX_TICK_INTERVAL_SECS"):
        Ok(val) =>
            match val.trim().parse::<u64>():
                Ok(secs) =>
                    tracing::info!("tick interval set to {}s via UNIMATRIX_TICK_INTERVAL_SECS", secs)
                    return secs
                Err(_) =>
                    tracing::warn!("UNIMATRIX_TICK_INTERVAL_SECS='{}' is not a valid u64, using default 900s", val)
                    return 900
        Err(_) =>
            // env var not set — normal case
            return 900
```

## Modification

Remove:
```
const TICK_INTERVAL_SECS: u64 = 900;
```

In `background_tick_loop`, replace:
```
let mut interval = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
```
With:
```
let tick_interval_secs = read_tick_interval();
let mut interval = tokio::time::interval(Duration::from_secs(tick_interval_secs));
```

Also update the `next_scheduled` computation at line ~413:
Replace:
```
meta.next_scheduled = Some(now_secs() + TICK_INTERVAL_SECS);
```
With a captured value. Since `background_tick_loop` is async, capture `tick_interval_secs`
in the outer scope before the loop and use it inside the loop for `next_scheduled`:
```
meta.next_scheduled = Some(now_secs() + tick_interval_secs);
```

## Error Handling
- Missing env var: silently fall back to 900 (expected in production)
- Non-parseable value: warn log + fall back to 900
- No panics, no process::exit — purely additive

## Key Test Scenarios
- Env var unset → returns 900
- Env var = "30" → returns 30
- Env var = "abc" → warns and returns 900
- Env var = "" → warns and returns 900

## Unit Test Location
Add a `#[cfg(test)]` module at the bottom of background.rs with:
```
mod tests {
    use super::*;

    #[test]
    fn test_read_tick_interval_default():
        // env var not set → 900
        std::env::remove_var("UNIMATRIX_TICK_INTERVAL_SECS")
        assert_eq!(read_tick_interval(), 900)

    #[test]
    fn test_read_tick_interval_custom():
        std::env::set_var("UNIMATRIX_TICK_INTERVAL_SECS", "30")
        assert_eq!(read_tick_interval(), 30)
        std::env::remove_var("UNIMATRIX_TICK_INTERVAL_SECS")

    #[test]
    fn test_read_tick_interval_invalid():
        std::env::set_var("UNIMATRIX_TICK_INTERVAL_SECS", "not-a-number")
        assert_eq!(read_tick_interval(), 900)
        std::env::remove_var("UNIMATRIX_TICK_INTERVAL_SECS")
}
```

NOTE: These tests mutate env vars. Mark with `#[serial_test::serial]` if serial_test is available,
OR use a temp_env crate, OR simply note that cargo test runs these in serial within the file.
Check if serial_test is already a dev-dependency. If not, use a different approach:
document that env var tests must not run in parallel — use `cargo test -- --test-threads=1`
for the background module, or wrap in a Mutex. The simplest safe approach without new deps:
use `std::env::set_var` + `remove_var` and accept potential parallel test flakiness (unlikely
given these are unit tests within one file). If flaky, add serial_test or a file-level mutex.
