## ADR-001: Daemonization via spawn-new-process (not fork)

### Context

SR-02 from the risk assessment identifies `fork(2)+setsid(2)` daemonization as a
High/High risk. Tokio explicitly documents that forking after runtime initialization
produces undefined behavior: the child inherits the thread pool fd set, registered
signal handlers, and open wakers. The `nix` crate provides safe Rust bindings to
`fork`, but calling them after `#[tokio::main]` is equivalent to calling the raw
syscall — the UB comes from the inherited Tokio state, not from Rust's safety model.

Three patterns were considered:

**Option A: fork-before-runtime.** The no-subcommand path runs synchronous startup
code first, calls `nix::unistd::fork()`, the parent exits, the child then calls
`#[tokio::main]`. This is safe only if the entire startup path before the fork is
synchronous. In practice, `clap::Parser::parse()` and project path initialization
are synchronous, so this is technically feasible. The problem is that `clap` parses
into a `Cli` struct before the subcommand is known — the fork-before-main structure
requires splitting parsing into two phases or using a build-time trick. The resulting
main.rs becomes structurally awkward and harder to follow. More critically, the
existing `handle_stale_pid_file` (which is synchronous but long-running — up to 10s)
runs before `PidGuard::acquire`. If this runs in the parent process before forking,
the parent blocks 10 seconds on every daemon start.

**Option B: double-fork-with-exec.** Fork, then exec the same binary with a
`--daemon-child` flag. The child is a fresh process with no inherited Tokio state.
This is the traditional Unix daemonization pattern that predates Tokio. The concern
here is that the first `fork` still happens in the launcher process, which by the
time `unimatrix serve --daemon` is dispatched to the async path, already has a Tokio
runtime running. This has the same UB problem as Option A if the fork occurs after
`#[tokio::main]`.

**Option C: spawn-new-process.** The launcher calls
`std::process::Command::new(std::env::current_exe())` with `--daemon-child` added to
the args. No `fork` call. The launcher is a parent that simply spawns a child process
via the OS-level `execve` path. The child starts with a fresh Tokio runtime and no
inherited state. The launcher optionally waits for the socket to appear (polling),
then exits.

All three options call `nix::unistd::setsid()` to create a new session. Options A and
B call it after fork. Option C calls it at the start of the child's synchronous
pre-runtime setup.

The key constraint from SCOPE.md: `#![forbid(unsafe_code)]`. The `nix` crate exposes
`nix::unistd::setsid()` as a safe Rust function, so this constraint is satisfied by
all options.

### Decision

Use **Option C: spawn-new-process**.

The launcher path for `unimatrix serve --daemon`:
1. Receives the `--daemon` flag in the `Serve` subcommand.
2. Detects it is NOT running as a daemon child (no `--daemon-child` flag).
3. Calls `std::process::Command::new(current_exe())` with all original args plus
   `--daemon-child`. Redirects child stdin to `/dev/null`, stdout/stderr to the log
   file at `~/.unimatrix/{hash}/unimatrix.log` (opened before the spawn so the path
   is known synchronously). Uses `.spawn()` — does not wait.
4. Polls `paths.mcp_socket_path` with 100ms intervals for up to 5 seconds.
5. Exits 0 when socket appears; exits 1 with error to stderr if timeout elapses.

The child path for `unimatrix serve --daemon --daemon-child`:
1. The `--daemon-child` flag is hidden in clap help output.
2. Calls `nix::unistd::setsid()` synchronously before any Tokio init to detach from
   the terminal.
3. Enters `tokio_main` for the full server stack.

No fork syscall is made anywhere in an async context. The `nix` crate is used only
for `setsid`, not `fork`. No Tokio runtime state is inherited.

### Consequences

Easier:
- No UB from Tokio + fork. This pattern is used by many production Rust daemons.
- The child starts with the exact same binary — no separate daemon binary to ship.
- `setsid` is a single safe call with no interaction with Tokio.
- The launcher's synchronous poll loop gives operators a visible startup timeout.
- `--daemon-child` can be checked with a simple `bool` arg and is transparent in
  process listings.

Harder:
- The child must re-parse command-line args that include `--daemon-child`. The flag
  must be added to the `Cli` struct even though it is an internal implementation detail.
  Using `#[arg(hide = true)]` keeps it out of help text.
- Two processes exist briefly during startup (launcher + child). Operators may see
  both in `ps` during the 5-second socket poll window.
- The log file path must be computed synchronously in the launcher before spawning the
  child, which means `ProjectPaths` computation happens twice (once in launcher, once
  in child). This is cheap (just string operations) and acceptable.
