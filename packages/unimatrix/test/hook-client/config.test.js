"use strict";

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const config = require("../../lib/hook-client/config.js");
const {
  ENV_URL,
  ENV_TOKEN,
  DEFAULT_TIMEOUTS,
  resolve,
  walkToProjectRoot,
  computeProjectHash,
  socketPathFor,
  mergeTimeouts,
} = config;

const GOLDENS = JSON.parse(
  fs.readFileSync(
    path.join(__dirname, "..", "fixtures", "parity", "project-hash-goldens.json"),
    "utf8"
  )
);

// ── Helpers (temp-dir fixtures) ─────────────────────────────────────

/** Create a temp project dir; opts.git=false skips the .git dir. */
function makeProject(opts) {
  const o = opts || {};
  const root = fs.realpathSync(
    fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-config-test-"))
  );
  if (o.git !== false) {
    fs.mkdirSync(path.join(root, ".git"));
  }
  return root;
}

/** Write {root}/.claude/settings.local.json (object or raw string). */
function writeLocalSettings(root, content) {
  const fp = path.join(root, ".claude", "settings.local.json");
  fs.mkdirSync(path.dirname(fp), { recursive: true });
  fs.writeFileSync(
    fp,
    typeof content === "string" ? content : JSON.stringify(content, null, 2),
    "utf8"
  );
  return fp;
}

function remoteSettings(url, token, extra) {
  return { unimatrix: { remote: Object.assign({ url, token }, extra || {}) } };
}

/**
 * Hand-built worktree fixture mirroring the project.rs unit tests
 * (test_detect_root_worktree_git_file etc.): main repo with
 * .git/worktrees/{name}, worktree dir whose `.git` FILE points there.
 * opts.inside → worktree nested under the main repo (for relative gitdir);
 * opts.gitdir → override the gitdir line target (absolute by default).
 */
function makeWorktree(opts) {
  const o = opts || {};
  const main = makeProject();
  const name = o.name || "wt";
  fs.mkdirSync(path.join(main, ".git", "worktrees", name), { recursive: true });
  let wt;
  if (o.inside) {
    wt = path.join(main, "worktrees", name);
    fs.mkdirSync(wt, { recursive: true });
  } else {
    wt = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-wt-")));
  }
  const gitdir =
    o.gitdir !== undefined ? o.gitdir : path.join(main, ".git", "worktrees", name);
  fs.writeFileSync(path.join(wt, ".git"), "gitdir: " + gitdir + "\n");
  return { main, wt };
}

// Env isolation across every test.
let savedUrl;
let savedTok;
beforeEach(function () {
  savedUrl = process.env[ENV_URL];
  savedTok = process.env[ENV_TOKEN];
  delete process.env[ENV_URL];
  delete process.env[ENV_TOKEN];
});
afterEach(function () {
  if (savedUrl === undefined) delete process.env[ENV_URL];
  else process.env[ENV_URL] = savedUrl;
  if (savedTok === undefined) delete process.env[ENV_TOKEN];
  else process.env[ENV_TOKEN] = savedTok;
});

// ── FR-06 Resolution Matrix (R-09) ──────────────────────────────────

describe("config.resolve — FR-06 resolution matrix", function () {
  it("test_env_pair_wins_over_file", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://file.example.com", "file-token"));
    process.env[ENV_URL] = "https://env.example.com";
    process.env[ENV_TOKEN] = "env-token";

    const reads = [];
    const origRead = fs.readFileSync;
    fs.readFileSync = function (...args) {
      reads.push(String(args[0]));
      return origRead.apply(fs, args);
    };
    let result;
    try {
      result = resolve(root);
    } finally {
      fs.readFileSync = origRead;
    }

    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.source, "env");
    assert.strictEqual(result.url, "https://env.example.com");
    assert.strictEqual(result.token, "env-token");
    assert.deepStrictEqual(result.timeouts, { ...DEFAULT_TIMEOUTS });
    assert.strictEqual(result.urlHost, "env.example.com");
    // File never consulted for url/token on the env path.
    assert.ok(
      !reads.some((p) => p.includes("settings.local.json")),
      "settings.local.json must not be read when env pair is set"
    );
  });

  it("test_partial_env_pair_is_misconfig (url only)", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://file.example.com", "file-token"));
    process.env[ENV_URL] = "https://env.example.com";

    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "partial_env");
    assert.strictEqual(result.token, undefined, "no token on failure shapes");
    assert.strictEqual(result.url, undefined, "no url on failure shapes");
  });

  it("test_partial_env_pair_is_misconfig (token only)", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://file.example.com", "file-token"));
    process.env[ENV_TOKEN] = "env-token";

    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "partial_env");
  });

  it("test_file_resolution_happy", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://remote.example.com:8443", "s3cr3t"));

    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.source, "file");
    assert.strictEqual(result.url, "https://remote.example.com:8443");
    assert.strictEqual(result.token, "s3cr3t");
    assert.deepStrictEqual(result.timeouts, { ...DEFAULT_TIMEOUTS });
    assert.strictEqual(result.urlHost, "remote.example.com:8443");
  });

  it("test_missing_file", function () {
    // ADR-002 §3: absent settings file is NOT a failure → local UDS mode.
    const root = makeProject();
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "uds");
    assert.strictEqual(result.projectRoot, root);
    assert.ok(result.projectHash);
    assert.notStrictEqual(result.reason, "missing", "the 'missing' reason is retired");
  });

  it("test_file_without_remote_key", function () {
    // Claude Code key-drop simulation: file present, unimatrix.remote absent.
    const root = makeProject();
    writeLocalSettings(root, { permissions: { allow: ["Read"] } });
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "uds");
  });

  it("test_file_with_incomplete_remote_key", function () {
    const root = makeProject();
    writeLocalSettings(root, { unimatrix: { remote: { url: "https://x.example.com" } } });
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "uds");
  });

  it("test_malformed_settings_json", function () {
    const root = makeProject();
    writeLocalSettings(root, "{ this is not json !!!");
    let result;
    assert.doesNotThrow(function () {
      result = resolve(root);
    });
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "malformed");
  });

  it("test_subdirectory_cwd", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://sub.example.com", "tok"));
    const sub = path.join(root, "a", "b", "c");
    fs.mkdirSync(sub, { recursive: true });

    const result = resolve(sub);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, root, "root found via .git walk");
    assert.strictEqual(result.url, "https://sub.example.com");
  });

  it("test_stdin_cwd_overrides_process_cwd", function () {
    // config.resolve uses the cwd it is GIVEN (index.js passes stdin.cwd when
    // non-empty); process.cwd() is unrelated to the project being resolved.
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://stdin-cwd.example.com", "tok"));
    assert.notStrictEqual(walkToProjectRoot(process.cwd()), root);

    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, root);
  });

  it("test_empty_cwd_falls_back_to_process_cwd", function () {
    const expected = walkToProjectRoot(process.cwd());
    const result = resolve("");
    assert.strictEqual(result.projectRoot, expected);
  });

  it("test_resolve_never_throws_on_junk_cwd", function () {
    for (const junk of [null, undefined, 42, {}, ""]) {
      assert.doesNotThrow(function () {
        const r = resolve(junk);
        assert.strictEqual(typeof r.ok, "boolean");
      });
    }
  });
});

// ── Transport selection — mode matrix + single derivation (ADR-002 §3,
//    ADR-007, AC-02, R-05) ────────────────────────────────────────────

describe("config.resolve — transport selection (ADR-002 §3, ADR-007)", function () {
  it("test_remote_env_pair_yields_http_mode", function () {
    const root = makeProject();
    process.env[ENV_URL] = "https://env.example.com";
    process.env[ENV_TOKEN] = "env-token";
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "http");
    assert.strictEqual(result.url, "https://env.example.com");
    assert.strictEqual(result.token, "env-token");
    assert.strictEqual(result.socketPath, undefined, "no socketPath in http mode");
  });

  it("test_settings_local_remote_yields_http_mode", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://file.example.com:8443", "tok"));
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "http");
    assert.strictEqual(result.source, "file");
    assert.strictEqual(result.socketPath, undefined);
  });

  it("test_http_wins_even_if_local_socket_live", function () {
    // Remote config present → HTTP unconditionally; no probe for a live local
    // socket, no override knob (FR-13, OQ1). The socket dir exists on disk here.
    const root = makeProject();
    const sockDir = path.join(os.homedir(), ".unimatrix", computeProjectHash(root));
    fs.mkdirSync(sockDir, { recursive: true });
    writeLocalSettings(root, remoteSettings("https://remote.example.com", "tok"));
    const result = resolve(root);
    assert.strictEqual(result.mode, "http", "remote config wins regardless of local socket");
    assert.strictEqual(result.socketPath, undefined);
  });

  it("test_no_remote_yields_uds_mode_with_socketpath", function () {
    const root = makeProject();
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "uds");
    assert.strictEqual(result.source, "local");
    assert.strictEqual(result.urlHost, "", "no remote host in uds mode");
    assert.strictEqual(
      result.socketPath,
      path.join(os.homedir(), ".unimatrix", result.projectHash, "unimatrix.sock")
    );
  });

  it("test_missing_breadcrumb_path_removed", function () {
    // The former terminal { ok:false, reason:"missing" } no longer exists: every
    // no-remote layout now resolves to UDS, never a "missing" breadcrumb.
    const noFile = makeProject();
    const noKey = makeProject();
    writeLocalSettings(noKey, { permissions: { allow: ["Read"] } });
    const incomplete = makeProject();
    writeLocalSettings(incomplete, { unimatrix: { remote: { url: "https://x.example.com" } } });
    for (const r of [resolve(noFile), resolve(noKey), resolve(incomplete)]) {
      assert.strictEqual(r.ok, true);
      assert.strictEqual(r.mode, "uds");
      assert.notStrictEqual(r.reason, "missing");
    }
  });

  it("test_partial_env_stays_terminal", function () {
    const root = makeProject();
    process.env[ENV_URL] = "https://env.example.com";
    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "partial_env");
    assert.strictEqual(result.mode, undefined, "no transport mode on a terminal result");
  });

  it("test_malformed_config_stays_terminal", function () {
    const root = makeProject();
    writeLocalSettings(root, "{ not json !!!");
    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "malformed");
    assert.strictEqual(result.mode, undefined);
  });

  it("test_no_home_uds_is_terminal_malformed", function () {
    // No HOME → no socket can be derived → honest terminal, never "missing".
    const root = makeProject();
    const origHomedir = os.homedir;
    os.homedir = function () {
      throw new Error("no home directory");
    };
    let result;
    try {
      assert.doesNotThrow(function () {
        result = resolve(root);
      });
    } finally {
      os.homedir = origHomedir;
    }
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "malformed");
  });

  it("test_no_remote_no_daemon_resolves_uds_not_terminal", function () {
    // R-13: resolution succeeds in UDS mode even with no daemon present (the
    // enqueue path owns the no-daemon UX, not a terminal config breadcrumb).
    const root = makeProject();
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "uds");
    assert.ok(result.socketPath, "socketPath derived even with no live daemon");
  });

  // ── Single-derivation invariant (ADR-007 §1, R-05 s3) ─────────────

  it("test_socketpath_dirname_equals_statedir_parent", function () {
    // Both live under ~/.unimatrix/{projectHash}/ — they can never disagree.
    for (const opts of [{}, { git: false }]) {
      const root = makeProject(opts);
      const result = resolve(root);
      assert.strictEqual(result.mode, "uds");
      assert.strictEqual(path.dirname(result.socketPath), path.dirname(result.stateDir));
    }
  });

  it("test_socketpath_uses_same_projecthash_as_statedir", function () {
    const root = makeProject();
    const sub = path.join(root, "deep", "er");
    fs.mkdirSync(sub, { recursive: true });
    const result = resolve(sub);
    // One computeProjectHash(walkToProjectRoot(cwd)); socketPathFor consumes it.
    assert.strictEqual(result.projectHash, computeProjectHash(root));
    assert.strictEqual(result.socketPath, socketPathFor(result.projectHash));
    assert.ok(result.socketPath.includes(result.projectHash));
    assert.ok(result.stateDir.includes(result.projectHash));
  });

  it("test_socketpathfor_null_on_no_home", function () {
    const origHomedir = os.homedir;
    os.homedir = function () {
      return "";
    };
    try {
      assert.strictEqual(socketPathFor("deadbeefdeadbeef"), null);
    } finally {
      os.homedir = origHomedir;
    }
  });
});

// ── Root Walk + Hash (split-brain prevention) ───────────────────────

describe("config root walk + hash", function () {
  it("test_nested_git_monorepo_nearest_root_wins", function () {
    const outer = makeProject();
    writeLocalSettings(outer, remoteSettings("https://outer.example.com", "outer-tok"));
    const pkg = path.join(outer, "pkg");
    fs.mkdirSync(path.join(pkg, ".git"), { recursive: true });
    writeLocalSettings(pkg, remoteSettings("https://inner.example.com", "inner-tok"));
    const sub = path.join(pkg, "sub");
    fs.mkdirSync(sub);

    const result = resolve(sub);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, pkg, "nearest .git wins");
    assert.strictEqual(result.url, "https://inner.example.com", "config read from nearest root");
    // Split-brain assertion: the SAME root string feeds the state-dir hash.
    assert.strictEqual(result.projectHash, computeProjectHash(result.projectRoot));
  });

  it("test_no_git_root_is_resolved_cwd", function () {
    const root = makeProject({ git: false });
    const sub = path.join(root, "deep", "er");
    fs.mkdirSync(sub, { recursive: true });

    const reads = [];
    const origRead = fs.readFileSync;
    fs.readFileSync = function (...args) {
      reads.push(String(args[0]));
      return origRead.apply(fs, args);
    };
    let result;
    try {
      result = resolve(sub);
    } finally {
      fs.readFileSync = origRead;
    }

    assert.strictEqual(result.projectRoot, path.resolve(sub), "no .git → resolved cwd");
    // One file read, no multi-location probing (ADR-006).
    assert.strictEqual(reads.length, 1, "exactly one file read");
    assert.strictEqual(
      reads[0],
      path.join(path.resolve(sub), ".claude", "settings.local.json")
    );
  });

  // ── Worktree parity (project.rs::resolve_git_file oracle) ─────────

  it("test_git_file_worktree_resolves_main_root", function () {
    // Oracle: test_detect_root_worktree_git_file — absolute gitdir.
    const { main, wt } = makeWorktree();
    assert.strictEqual(walkToProjectRoot(wt), main);
  });

  it("test_worktree_relative_gitdir", function () {
    // Oracle: test_worktree_relative_gitdir — relative path resolved against
    // the dir containing the .git file.
    const { main, wt } = makeWorktree({
      inside: true,
      name: "rel-wt",
      gitdir: path.join("..", "..", ".git", "worktrees", "rel-wt"),
    });
    assert.strictEqual(walkToProjectRoot(wt), main);
  });

  it("test_worktree_same_hash_as_main", function () {
    // Oracle: test_worktree_same_hash_as_main_repo — one state dir for all
    // worktrees of a repo.
    const { main, wt } = makeWorktree();
    const a = resolve(main);
    const b = resolve(wt);
    assert.strictEqual(a.projectRoot, b.projectRoot);
    assert.strictEqual(a.projectHash, b.projectHash);
    assert.strictEqual(b.projectHash, computeProjectHash(main));
  });

  it("test_worktree_subdirectory_cwd_resolves_main_root", function () {
    const { main, wt } = makeWorktree();
    const sub = path.join(wt, "deep", "er");
    fs.mkdirSync(sub, { recursive: true });
    assert.strictEqual(walkToProjectRoot(sub), main);
  });

  it("test_worktree_finds_main_root_settings", function () {
    // Claim (b) end-to-end: settings.local.json lives ONLY in the main root
    // (gitignored — absent in worktrees); resolve from a worktree cwd must
    // find it, or every worktree spawn silently drops events.
    const { main, wt } = makeWorktree();
    writeLocalSettings(main, remoteSettings("https://wt.example.com", "tok"));
    const result = resolve(wt);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.source, "file");
    assert.strictEqual(result.url, "https://wt.example.com");
    assert.strictEqual(result.projectRoot, main);
  });

  it("test_git_file_no_gitdir_line_falls_back", function () {
    // Oracle: test_worktree_git_file_no_gitdir_line — Rust errors (hook.rs
    // then uses raw cwd); the fail-open JS falls back to the containing dir.
    const root = makeProject({ git: false });
    fs.writeFileSync(path.join(root, ".git"), "something unexpected\n");
    assert.strictEqual(walkToProjectRoot(root), root);
  });

  it("test_git_file_dangling_gitdir_falls_back", function () {
    const root = makeProject({ git: false });
    fs.writeFileSync(
      path.join(root, ".git"),
      "gitdir: " + path.join(root, "nonexistent", ".git", "worktrees", "x") + "\n"
    );
    assert.strictEqual(walkToProjectRoot(root), root);
  });

  it("test_git_file_no_git_dir_ancestor_falls_back", function () {
    // gitdir target exists but no `.git` DIRECTORY ancestor (project.rs:112-113).
    const root = makeProject({ git: false });
    const target = path.join(root, "not-git", "worktrees", "x");
    fs.mkdirSync(target, { recursive: true });
    fs.writeFileSync(path.join(root, ".git"), "gitdir: " + target + "\n");
    assert.strictEqual(walkToProjectRoot(root), root);
  });

  it("test_symlinked_root_resolves_to_realpath_same_hash", function () {
    // ADR-007 §3 healthy layout: a symlink pointing at the repo root. Both Rust
    // (canonicalize) and TS (realpathSync) resolve through the link, so a spawn
    // whose cwd is the symlink shares the main project's hash / socket / state
    // dir. Without realpath parity the symlink alias would hash to a second
    // state dir and queued frames would never replay.
    if (process.platform === "win32") return; // POSIX symlink semantics
    const real = makeProject();
    const linkParent = fs.realpathSync(
      fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-link-"))
    );
    const link = path.join(linkParent, "linked-project");
    fs.symlinkSync(real, link);

    assert.strictEqual(walkToProjectRoot(link), real, "symlink resolves to the real root");
    assert.strictEqual(
      computeProjectHash(walkToProjectRoot(link)),
      computeProjectHash(real),
      "symlinked path hashes identically to the real root"
    );
    const viaLink = resolve(link);
    const viaReal = resolve(real);
    assert.strictEqual(viaLink.projectHash, viaReal.projectHash);
    assert.strictEqual(viaLink.socketPath, viaReal.socketPath);
  });

  it("test_hash_parity_with_rust", function () {
    // Goldens generated from project.rs::compute_project_hash — never hand-written.
    assert.ok(GOLDENS.cases.length >= 3, "fixture must carry at least 3 golden cases");
    for (const c of GOLDENS.cases) {
      assert.strictEqual(
        computeProjectHash(c.normalized),
        c.hash,
        `hash parity for ${c.normalized}`
      );
      assert.match(c.hash, /^[0-9a-f]{16}$/);
    }
  });

  it("test_trailing_slash_normalized_before_hash", function () {
    if (process.platform === "win32") return; // posix path shape
    const c = GOLDENS.cases.find((x) => x.input.endsWith("/"));
    assert.ok(c, "trailing-slash golden present");
    assert.strictEqual(path.resolve(c.input), c.normalized);
    // walkToProjectRoot resolves before hashing, so the trailing slash never
    // reaches the hash input (same-root spawns hash identically).
    assert.strictEqual(computeProjectHash(path.resolve(c.input)), c.hash);
  });

  it("test_state_dir_path_shape", function () {
    const root = makeProject();
    const result = resolve(root);
    assert.strictEqual(
      result.stateDir,
      path.join(os.homedir(), ".unimatrix", result.projectHash, "hook-client")
    );
  });

  it("test_hash_deterministic_same_root_different_subdir", function () {
    const root = makeProject();
    const sub = path.join(root, "x", "y");
    fs.mkdirSync(sub, { recursive: true });
    const a = resolve(root);
    const b = resolve(sub);
    assert.strictEqual(a.projectRoot, b.projectRoot);
    assert.strictEqual(a.projectHash, b.projectHash);
  });
});

// ── Cross-Platform (R-14) ───────────────────────────────────────────

describe("config cross-platform", function () {
  it(
    "test_root_walk_windows_separators",
    { skip: process.platform !== "win32" },
    function () {
      // Windows runner only: walk with native backslash separators and a
      // drive-letter root; .git dir found from a backslash subpath.
      const root = makeProject();
      const sub = path.join(root, "a", "b");
      fs.mkdirSync(sub, { recursive: true });
      assert.strictEqual(walkToProjectRoot(sub.split(path.sep).join("\\")), root);
    }
  );

  it("test_windows_backslash_root_hashes_deterministically", function () {
    // Pure string hash — verifiable on every OS against the Rust golden.
    const c = GOLDENS.cases.find((x) => x.normalized.includes("\\"));
    assert.ok(c, "windows-path golden present");
    assert.strictEqual(computeProjectHash(c.normalized), c.hash);
  });

  it("test_homedir_resolution_degrades_to_null_state_dir", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://home.example.com", "tok"));
    const origHomedir = os.homedir;
    os.homedir = function () {
      throw new Error("no home directory");
    };
    let result;
    try {
      assert.doesNotThrow(function () {
        result = resolve(root);
      });
    } finally {
      os.homedir = origHomedir;
    }
    assert.strictEqual(result.ok, true, "resolution still returns; sends still possible");
    assert.strictEqual(result.stateDir, null, "persistence disabled, not fatal");
  });

  it("test_homedir_empty_degrades_to_null_state_dir", function () {
    const root = makeProject();
    const origHomedir = os.homedir;
    os.homedir = function () {
      return "";
    };
    let result;
    try {
      result = resolve(root);
    } finally {
      os.homedir = origHomedir;
    }
    assert.strictEqual(result.stateDir, null);
  });
});

// ── Timeout Overrides (ADR-005; keys pinned at Stage 3a) ────────────

describe("config timeout overrides", function () {
  it("test_timeout_overrides_applied", function () {
    const root = makeProject();
    writeLocalSettings(
      root,
      remoteSettings("https://t.example.com", "tok", {
        timeouts: { connect_ms: 100, sync_ms: 1500, fnf_ms: 2500 },
      })
    );
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.deepStrictEqual(result.timeouts, { connectMs: 100, syncMs: 1500, fnfMs: 2500 });
  });

  it("test_partial_timeout_override_keeps_defaults", function () {
    const t = mergeTimeouts({ sync_ms: 900 });
    assert.deepStrictEqual(t, { connectMs: 750, syncMs: 900, fnfMs: 3000 });
  });

  it("test_junk_timeout_overrides_ignored", function () {
    for (const junk of [
      { connect_ms: "fast", sync_ms: -5, fnf_ms: 9e9 },
      { connect_ms: NaN, sync_ms: Infinity, fnf_ms: 0 },
      { connect_ms: null, sync_ms: {}, fnf_ms: [] },
      "not-an-object",
      ["array"],
      null,
      undefined,
      42,
    ]) {
      assert.deepStrictEqual(
        mergeTimeouts(junk),
        { ...DEFAULT_TIMEOUTS },
        `defaults retained for ${JSON.stringify(junk)}`
      );
    }
  });

  it("test_fractional_timeout_floored", function () {
    assert.strictEqual(mergeTimeouts({ connect_ms: 100.9 }).connectMs, 100);
  });

  it("test_defaults_match_adr_005", function () {
    assert.deepStrictEqual({ ...DEFAULT_TIMEOUTS }, { connectMs: 750, syncMs: 2000, fnfMs: 3000 });
  });
});

// ── Security (R-16 adjunct) ─────────────────────────────────────────

describe("config security posture", function () {
  it("test_module_has_no_network_or_output_surface", function () {
    // config.js must be incapable of network I/O or echoing the token:
    // no network/process modules, no console/stderr/stdout writes.
    const src = fs.readFileSync(
      require.resolve("../../lib/hook-client/config.js"),
      "utf8"
    );
    assert.ok(
      !/require\(["'](?:node:)?(?:http|https|net|tls|dgram|child_process)["']\)/.test(src),
      "no network-capable module required"
    );
    assert.ok(!src.includes("console."), "no console output");
    assert.ok(!src.includes("process.stderr"), "no stderr writes");
    assert.ok(!src.includes("process.stdout"), "no stdout writes");
  });

  it("test_unparseable_url_not_rejected_here", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("not a url at all", "tok"));
    const result = resolve(root);
    // Fail-open: transport classifies at send time; init is the loud check.
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.urlHost, "");
  });
});
