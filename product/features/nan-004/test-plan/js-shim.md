# Test Plan: C2 — JS Shim

## Unit Tests (packages/unimatrix/test/shim.test.js)

### Argument Routing

- `test_init_arg_routes_to_js_init`: When `process.argv[2] === 'init'`, the shim calls `lib/init.js` instead of exec'ing the binary. Mock `init.js` and assert it is called.
- `test_hook_arg_routes_to_binary`: When `process.argv[2] === 'hook'`, the shim calls `execFileSync` with the resolved binary path and `['hook', ...]` args.
- `test_export_arg_routes_to_binary`: `process.argv = ['node', 'unimatrix.js', 'export']` routes to binary exec.
- `test_no_args_routes_to_binary`: `process.argv = ['node', 'unimatrix.js']` routes to binary (MCP server mode).
- `test_version_arg_routes_to_binary`: `process.argv = ['node', 'unimatrix.js', 'version']` routes to binary, not init.
- `test_dash_dash_version_routes_to_binary`: `process.argv = ['node', 'unimatrix.js', '--version']` routes to binary.
- `test_init_with_dry_run_routes_to_js`: `process.argv = ['node', 'unimatrix.js', 'init', '--dry-run']` routes to JS init with dry-run flag.

### Exit Code Passthrough

- `test_binary_exit_0_propagates`: Mock `execFileSync` returning successfully. Assert `process.exitCode` is 0 (or undefined).
- `test_binary_exit_1_propagates`: Mock `execFileSync` throwing with `status: 1`. Assert `process.exitCode` is 1.
- `test_binary_exit_signal_propagates`: Mock `execFileSync` throwing with `signal: 'SIGTERM'`. Assert shim exits with code 1.

### Error Handling

- `test_binary_not_found_prints_platforms`: Mock `resolveBinary()` throwing. Assert stderr contains "Supported platforms" and lists `linux-x64`.
- `test_binary_not_found_exits_1`: When resolution fails, assert exit code is 1.

## Integration Tests (Shell)

- `test_npx_unimatrix_version_outputs_version_string`: Run the actual `node bin/unimatrix.js version` with a real binary. Assert stdout matches `/^unimatrix \d+\.\d+\.\d+/`.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-05 | Init interception misroutes non-init | `test_hook_arg_routes_to_binary`, `test_export_arg_routes_to_binary`, `test_version_arg_routes_to_binary` |
| R-05 | Exit code not forwarded | `test_binary_exit_1_propagates`, `test_binary_exit_signal_propagates` |
