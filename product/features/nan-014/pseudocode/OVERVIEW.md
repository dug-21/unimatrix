# nan-014: Container Packaging — Pseudocode Overview

## Components

| # | Component | File | Wave | Rationale |
|---|-----------|------|------|-----------|
| 1 | pidguard-self-pid | pidguard-self-pid.md | 1 | General correctness fix; no dependencies; blocks serve-foreground safety |
| 2 | config-env-override | config-env-override.md | 1 | Small code change; no dependencies; blocks Dockerfile ENV correctness |
| 3 | serve-foreground | serve-foreground.md | 1 | CLI addition; depends on pidguard fix being in same binary |
| 4 | health-subcommand | health-subcommand.md | 1 | CLI addition; independent of serve-foreground |
| 5 | dockerignore | dockerignore.md | 2 | Static file; blocks Dockerfile build context |
| 6 | dockerfile | dockerfile.md | 2 | Depends on wave 1 code changes being merged; depends on .dockerignore |
| 7 | docker-compose | docker-compose.md | 2 | Depends on Dockerfile existing; static config file |
| 8 | ci-container-jobs | ci-container-jobs.md | 2 | Depends on Dockerfile existing; YAML-only change to release.yml |

## Wave Plan

- **Wave 1** (components 1-4): Rust code changes. All four are independent and can be implemented in parallel. Must compile and pass tests before wave 2.
- **Wave 2** (components 5-8): Container infrastructure. Depends on wave 1 binary having --foreground and health subcommands. Components 5-8 can be implemented in parallel (Dockerfile references .dockerignore implicitly via Docker build).

## Data Flow

```
                    Container Start
                         |
                         v
    CMD ["serve", "--foreground", "--project-dir", "/data"]
                         |
                         v
              main() match arm (wave 1)
                         |
                         v
              tokio_main_daemon(cli)        <-- no setsid, no launcher
                         |
                    +---------+
                    |         |
                    v         v
           PidGuard acquire   load_config
           (self-PID guard)   (UNIMATRIX_CONFIG env var)
                    |         |
                    v         v
              Full daemon stack running
                         |
                         v
    HEALTHCHECK: unimatrix health --project-dir /data
                         |
                         v
              health::run(project_dir)
                         |
                         v
              ensure_data_directory(Some("/data"), None)
                         |
                         v
              UnixStream::connect(mcp_socket_path)
                         |
                    +----+----+
                    |         |
                    v         v
              exit 0      exit 1
             (healthy)   (unhealthy)
```

## Shared Types (No New Types)

All components use existing types. No new structs, enums, or traits are introduced beyond:

- `Command::Health` — new enum variant (no fields, uses `--project-dir` from `Cli`)
- `Serve { foreground: bool }` — new field on existing variant
- `health::run(project_dir: Option<&Path>) -> Result<(), Box<dyn Error>>` — new public function

## Integration Surface (from Architecture)

| Interface | Type | Source | Used By |
|-----------|------|--------|---------|
| `Command::Serve { foreground: bool }` | New field | main.rs | serve-foreground |
| `Command::Health` | New variant | main.rs | health-subcommand |
| `health::run(Option<&Path>)` | New pub fn | health.rs | main.rs, Dockerfile HEALTHCHECK |
| `tokio_main_daemon(Cli)` | Existing async fn | main.rs | serve-foreground (direct call) |
| `ensure_data_directory(Option<&Path>, Option<&Path>)` | Existing pub fn | project.rs | health, serve-foreground |
| `handle_stale_pid_file(&Path, Duration)` | Existing pub fn | pidfile.rs | pidguard-self-pid (modified) |
| `shutdown::shutdown_signal()` | Existing pub async fn | shutdown.rs | serve-foreground (unchanged) |
| `UNIMATRIX_CONFIG` env var | New convention | Dockerfile ENV | config-env-override |

## Sequencing Constraints

1. **pidguard-self-pid** must be in the binary before any container testing (PID 1 self-termination bug).
2. **config-env-override** must be in the binary before Dockerfile sets `UNIMATRIX_CONFIG` env var.
3. **serve-foreground** must be in the binary before Dockerfile `CMD` references `--foreground`.
4. **health-subcommand** must be in the binary before Dockerfile `HEALTHCHECK` references `health`.
5. **.dockerignore** should exist before first `docker build` (otherwise build context is enormous).
6. **dockerfile** depends on all wave 1 code being compilable.
7. **docker-compose** and **ci-container-jobs** depend on Dockerfile existing.
