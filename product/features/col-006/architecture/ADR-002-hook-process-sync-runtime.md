## ADR-002: Hook Process Uses Blocking std I/O (No Tokio Runtime)

### Context

The hook subcommand (`unimatrix-server hook <EVENT>`) is an ephemeral process spawned by Claude Code for each lifecycle event. It performs a single operation: connect to the UDS, send one request, optionally receive one response, and exit. The 50ms latency budget covers the entire lifecycle including process startup.

ASS-014 estimated process startup at ~3ms for a compiled Rust binary. Adding tokio runtime initialization adds ~1-3ms (thread pool allocation, I/O driver setup). For a process that lives <15ms and performs one blocking I/O operation, this overhead is significant (6-20% of budget) with no functional benefit.

The alternative is to use `std::os::unix::net::UnixStream` with `SO_RCVTIMEO`/`SO_SNDTIMEO` socket options for timeout behavior. This provides the same functionality (connect with timeout, read with timeout, write) without any async runtime.

### Decision

The hook process path (`unimatrix-server hook <EVENT>`) uses blocking standard library I/O exclusively. No tokio runtime is initialized when the `hook` subcommand is selected.

Specifically:
- `std::os::unix::net::UnixStream` for UDS connection
- `SO_RCVTIMEO` / `SO_SNDTIMEO` set via `UnixStream::set_read_timeout()` and `set_write_timeout()` for timeout behavior (default: 40ms, leaving 10ms margin within the 50ms budget)
- `std::io::Read` / `std::io::Write` for framed message I/O
- `std::io::stdin().read_to_string()` for reading Claude Code hook JSON

The `main.rs` entry point branches early:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Hook { event }) => {
            // Sync path: no tokio, no tracing init, minimal startup
            hook::run(event)
        }
        None => {
            // Async path: full server with tokio runtime
            tokio_main()
        }
    }
}

#[tokio::main]
async fn tokio_main() -> Result<(), Box<dyn std::error::Error>> {
    // ... existing server startup ...
}
```

This ensures the hook code path never pays for tokio initialization.

The `Transport` trait has a synchronous public API (`fn request(&mut self, ...) -> Result<...>`). The `LocalTransport` implementation wraps the blocking `UnixStream` calls. The server-side UDS listener remains async (tokio `UnixListener`) because it runs inside the existing tokio runtime.

### Consequences

**Easier:**
- Hook process startup is ~3ms (binary init + clap parse + project hash), not ~5ms (adding tokio).
- The `Transport` trait is simpler — no async, no Pin, no Future bounds.
- Testing the transport is simpler — no need for tokio test runtime in transport unit tests.
- The `main.rs` structure clearly separates the two execution modes.

**Harder:**
- If future hook operations need concurrent I/O (e.g., connect to socket while reading stdin), blocking I/O becomes a limitation. However, the current design reads stdin fully before connecting — no concurrency needed.
- If the hook evolves into a daemon (Phase 2), the daemon would need its own async runtime. The `Transport` trait's sync API would need an async wrapper. This is acceptable — the daemon is a different execution model and can wrap sync calls in `spawn_blocking` (the same pattern the server already uses for redb ops).
- Connection timeout via `SO_RCVTIMEO` has platform-specific behavior for the timeout granularity. On Linux, the minimum effective timeout is ~4ms. On macOS, it may be larger. This is acceptable — the 40ms default is well above platform minimums.
