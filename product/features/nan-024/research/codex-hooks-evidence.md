# Codex hooks evidence (closes OD-4 / OQ-A, OQ-B)

Source: github.com/openai/codex @ `06f334aa4b053023c5b4199dde0ba9c004d64820` (shallow clone, 2026-11-25). Paths relative to `codex-rs/`. Official docs not fetched; doc-level claims are UNVERIFIED.

## (a) Spawn mechanism — shell, not argv exec. Confidence: HIGH

- `hooks/src/engine/command_runner.rs:284` — hook `command` string passed to `build_command(&runtime.shell, command_line, ...)`.
- `command_runner.rs:456-477` — if no shell configured: `Command::new(default_shell_program(env))`, then `-lc` (Unix) / `/C` (Windows), then the command line as one arg. If configured (`registry.rs:54-55,187-188` `shell_program`/`shell_args`), uses that program + args + command line.
- `command_runner.rs:505-526` — default program = `$SHELL` from the session env snapshot, fallback `/bin/sh` (Unix); `%COMSPEC%`, fallback `cmd.exe` (Windows).
- `command_runner.rs:470-473` — Windows wraps the line as `"<cmd>"` raw arg for `/C`.
- `command_runner.rs:489-500` — env starts empty, replays session snapshot + hook env, minus non-inheritable vars. So `$HOME` is present if in the session snapshot (normal case).

Consequence: `"$HOME/..."` expands on Unix (POSIX shell via `-lc`; login shell also sources profile). On Windows `cmd.exe` does NOT expand `$HOME` (needs `%USERPROFILE%`). UNVERIFIED: behaviour when `$SHELL` is a non-POSIX shell (fish/nu) — `-lc` still accepted by fish, but quoting semantics differ.

## (b) User + project merge — additive, both fire; project requires trusted project AND per-hook trust. Confidence: HIGH

- `hooks/src/engine/discovery.rs:159-222` — iterates `config_layer_stack.layers_low_to_high()`; for each layer loads `<layer folder>/hooks.json` (`:179-184`, `load_hooks_json` `:393-397`) and `[hooks]` in that layer's config.toml (`:185`), and appends all handlers. No override/replace; handlers from user and project layers accumulate. Same folder only loaded once (`visited_json_hook_folders`, `:180`). Warning if one layer has both json and toml hooks (`:187-197`).
- Layer order (`config/src/loader/mod.rs:136-150`): system, user `${CODEX_HOME}`, profile, cwd/tree/repo `.codex/`, runtime.
- Project trust gate: `config/src/state.rs:874-877` `layers_low_to_high` skips disabled layers; `config/src/loader/mod.rs:143-145,1156-1172` — project layers are "loaded but disabled when untrusted"; disabled reason explicitly names "project-local config, hooks, and exec policies". Untrusted project => its `.codex/hooks.json` does not fire.
- Per-hook trust gate (applies to user AND project, non-managed): `discovery.rs:1072-1089` status is `Trusted` only if stored trusted hash == current hash, else `Modified`/`Untrusted`; `discovery.rs:954-961` handler is registered only if `Managed|Trusted` or `bypass_hook_trust`. Hash covers event, matcher, group, handler config (`discovery.rs:877`) — editing the command invalidates trust.
- Trust state is read only from User and SessionFlags layers (`hooks/src/config_rules.rs:15-29`), keyed `file:<path>:<event>:<group>:<handler>` (`config_rules.rs:83`). Bypass: `--bypass-hook-trust` CLI (`exec/src/cli.rs:193`, `exec/src/lib.rs:672`) / app-server override (`app-server/src/config_manager.rs:713`).
- UNVERIFIED: the interactive UX that records trust (e.g. `/hooks` approval) and its exact config.toml key name; not traced.

## Implications for shim wiring

1. `"$HOME/.../shim"` form works on Unix/macOS. Not portable to Windows default shell; use absolute path written at install time (safest everywhere) or accept Unix-only.
2. Installing in both `~/.codex/hooks.json` and `.codex/hooks.json` double-fires — pick one location (user-level avoids project-trust dependency).
3. A freshly written hook will NOT run until the user trusts it (or `--bypass-hook-trust`). Installer must either document the approval step or write the trusted hash into user config.toml (hash format reverse-engineering = UNVERIFIED, fragile). Any later change to the command string re-untrusts it — keep the command string stable across upgrades (favours a stable path + versioned shim behind it).
