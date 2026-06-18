"use strict";

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const https = require("https");
const crypto = require("crypto");
const { spawnSync } = require("child_process");

const config = require("../../lib/hook-client/config.js");
const credstore = require("../../lib/hook-client/credstore.js");
const transport = require("../../lib/hook-client/transport-http.js");
const { computeFingerprint } = require("../../lib/hook-client/cert-pin.js");
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

const STORE_TOKEN = "s3cr3t";

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

/**
 * Seed the out-of-tree store ~/.unimatrix/<projectHash>/remote.json (vnc-039
 * ADR-004) for the project rooted at `root`. cred merges over the canonical
 * schema defaults; pass observe_url/fingerprint/timeouts overrides. Mirrors
 * credstore.write but lets a test inject a raw string / drop fields to exercise
 * malformed / incomplete postures. Relies on the temp-HOME override below.
 */
function writeRemoteStore(root, cred) {
  const hash = computeProjectHash(root);
  const fp = credstore.pathFor(hash);
  fs.mkdirSync(path.dirname(fp), { recursive: true });
  const body =
    typeof cred === "string"
      ? cred
      : JSON.stringify(
          Object.assign({ schema_version: 1 }, cred),
          null,
          2
        );
  fs.writeFileSync(fp, body, { mode: 0o600 });
  return fp;
}

/** Canonical-schema cred object (observe_url is the post target, NOT url). */
function remoteCred(observeUrl, token, extra) {
  return Object.assign(
    {
      mcp_url: "https://host.example/v1/slug",
      observe_url: observeUrl,
      token: token,
      fingerprint: null,
    },
    extra || {}
  );
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

// Env isolation + temp-HOME override across every test. The store now lives
// under ~/.unimatrix/<projectHash>/remote.json (vnc-039 ADR-004), so every test
// gets an isolated temp HOME — no test writes into the real home, and store
// fixtures are cleaned up per test. Mirrors the credstore suite harness.
let savedUrl;
let savedTok;
let tmpHome;
let origHomedir;
beforeEach(function () {
  savedUrl = process.env[ENV_URL];
  savedTok = process.env[ENV_TOKEN];
  delete process.env[ENV_URL];
  delete process.env[ENV_TOKEN];
  tmpHome = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-config-home-")));
  origHomedir = os.homedir;
  os.homedir = function () {
    return tmpHome;
  };
});
afterEach(function () {
  if (savedUrl === undefined) delete process.env[ENV_URL];
  else process.env[ENV_URL] = savedUrl;
  if (savedTok === undefined) delete process.env[ENV_TOKEN];
  else process.env[ENV_TOKEN] = savedTok;
  os.homedir = origHomedir;
  try {
    fs.rmSync(tmpHome, { recursive: true, force: true });
  } catch (_err) {
    // best-effort cleanup
  }
});

// ── FR-06 Resolution Matrix (R-09) ──────────────────────────────────

describe("config.resolve — FR-06 resolution matrix", function () {
  it("test_env_pair_wins_over_file", function () {
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://file.example.com/observe", "file-token"));
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
    assert.strictEqual(result.pinnedFp, null, "env path stays unpinned (ADR-004)");
    // Store never consulted on the env path.
    assert.ok(
      !reads.some((p) => p.includes("remote.json")),
      "the credential store must not be read when env pair is set"
    );
  });

  it("test_partial_env_pair_is_misconfig (url only)", function () {
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://file.example.com/observe", "file-token"));
    process.env[ENV_URL] = "https://env.example.com";

    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "partial_env");
    assert.strictEqual(result.token, undefined, "no token on failure shapes");
    assert.strictEqual(result.url, undefined, "no url on failure shapes");
  });

  it("test_partial_env_pair_is_misconfig (token only)", function () {
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://file.example.com/observe", "file-token"));
    process.env[ENV_TOKEN] = "env-token";

    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "partial_env");
  });

  it("test_file_resolution_happy", function () {
    const root = makeProject();
    writeRemoteStore(
      root,
      remoteCred("https://remote.example.com:8443/observe", STORE_TOKEN)
    );

    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.source, "file");
    // observe_url is the post target (NOT the never-written url key).
    assert.strictEqual(result.url, "https://remote.example.com:8443/observe");
    assert.strictEqual(result.token, STORE_TOKEN);
    assert.deepStrictEqual(result.timeouts, { ...DEFAULT_TIMEOUTS });
    assert.strictEqual(result.urlHost, "remote.example.com:8443");
  });

  it("test_missing_file", function () {
    // ADR-002 §3: absent store is NOT a failure → local UDS mode.
    const root = makeProject();
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "uds");
    assert.strictEqual(result.projectRoot, root);
    assert.ok(result.projectHash);
    assert.notStrictEqual(result.reason, "missing", "the 'missing' reason is retired");
  });

  it("test_file_with_incomplete_remote_key", function () {
    // Store present but missing observe_url → UDS fall-through (R-13).
    const root = makeProject();
    writeRemoteStore(root, { mcp_url: "https://x.example.com/v1/s", token: "tok", fingerprint: null });
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.mode, "uds");
  });

  it("test_malformed_store_json", function () {
    const root = makeProject();
    writeRemoteStore(root, "{ this is not json !!!");
    let result;
    assert.doesNotThrow(function () {
      result = resolve(root);
    });
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "malformed");
  });

  it("test_subdirectory_cwd", function () {
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://sub.example.com/observe", "tok"));
    const sub = path.join(root, "a", "b", "c");
    fs.mkdirSync(sub, { recursive: true });

    const result = resolve(sub);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, root, "root found via .git walk");
    assert.strictEqual(result.url, "https://sub.example.com/observe");
  });

  it("test_stdin_cwd_overrides_process_cwd", function () {
    // config.resolve uses the cwd it is GIVEN (index.js passes stdin.cwd when
    // non-empty); process.cwd() is unrelated to the project being resolved.
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://stdin-cwd.example.com/observe", "tok"));
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

  it("test_store_remote_yields_http_mode", function () {
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://file.example.com:8443/observe", "tok"));
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
    writeRemoteStore(root, remoteCred("https://remote.example.com/observe", "tok"));
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
    const noStore = makeProject();
    const incomplete = makeProject();
    writeRemoteStore(incomplete, {
      mcp_url: "https://x.example.com/v1/s",
      token: "tok",
      fingerprint: null,
    });
    for (const r of [resolve(noStore), resolve(incomplete)]) {
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
    writeRemoteStore(root, "{ not json !!!");
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
    // Each root keys the store by its OWN projectHash; nearest .git wins, so the
    // store read is keyed by the inner root's hash.
    const outer = makeProject();
    writeRemoteStore(outer, remoteCred("https://outer.example.com/observe", "outer-tok"));
    const pkg = path.join(outer, "pkg");
    fs.mkdirSync(path.join(pkg, ".git"), { recursive: true });
    writeRemoteStore(pkg, remoteCred("https://inner.example.com/observe", "inner-tok"));
    const sub = path.join(pkg, "sub");
    fs.mkdirSync(sub);

    const result = resolve(sub);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, pkg, "nearest .git wins");
    assert.strictEqual(result.url, "https://inner.example.com/observe", "store keyed by nearest root");
    // Split-brain assertion: the SAME root string feeds the state-dir hash.
    assert.strictEqual(result.projectHash, computeProjectHash(result.projectRoot));
  });

  it("test_no_git_root_is_resolved_cwd", function () {
    const root = makeProject({ git: false });
    const sub = path.join(root, "deep", "er");
    fs.mkdirSync(sub, { recursive: true });

    const storeReads = [];
    const origRead = fs.readFileSync;
    fs.readFileSync = function (...args) {
      if (String(args[0]).includes("remote.json")) storeReads.push(String(args[0]));
      return origRead.apply(fs, args);
    };
    let result;
    try {
      result = resolve(sub);
    } finally {
      fs.readFileSync = origRead;
    }

    assert.strictEqual(result.projectRoot, path.resolve(sub), "no .git → resolved cwd");
    // At most one store read, no multi-location probing (ADR-006). ENOENT here.
    assert.ok(storeReads.length <= 1, "at most one store read");
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

  it("test_worktree_finds_main_root_store", function () {
    // Claim (b) end-to-end: the store is keyed by the MAIN repo hash (every
    // worktree shares one root → one hash → one store); resolve from a worktree
    // cwd must find it, or every worktree spawn silently drops events.
    const { main, wt } = makeWorktree();
    writeRemoteStore(main, remoteCred("https://wt.example.com/observe", "tok"));
    const result = resolve(wt);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.source, "file");
    assert.strictEqual(result.url, "https://wt.example.com/observe");
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
    // Remote config via the env pair (homedir-independent); no homedir → the
    // store/state/socket can't be derived, but the env-mode resolution still
    // returns http with persistence disabled (stateDir null), not fatal.
    const root = makeProject();
    process.env[ENV_URL] = "https://home.example.com";
    process.env[ENV_TOKEN] = "tok";
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
    assert.strictEqual(result.mode, "http");
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
    writeRemoteStore(
      root,
      remoteCred("https://t.example.com/observe", "tok", {
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
    writeRemoteStore(root, remoteCred("not a url at all", "tok"));
    const result = resolve(root);
    // Fail-open: transport classifies at send time; init is the loud check.
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.urlHost, "");
  });
});

// ── AC-08c — canonical-read regression (R-06): observe_url + pinnedFp ──
//
// vnc-039 ADR-004: file-mode resolve() is repointed to the out-of-tree store and
// reads the canonical schema (observe_url + fingerprint, NOT the never-written
// url). The break being fixed: today the read keys on `url` (absent) and never
// reads `fingerprint`, so file-mode observe silently falls through to UDS and
// would run unpinned. These are field-shape regressions; the wire proof is
// AC-08d below (field-presence is necessary but NOT sufficient — lesson #4970).

const SAMPLE_FP =
  "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

describe("config.resolve — AC-08c canonical store read (R-06)", function () {
  it("test_resolve_fileMode_returnsObserveUrlNotUrl", function () {
    const root = makeProject();
    writeRemoteStore(
      root,
      remoteCred("https://cloud.example/v1/slug/observe", STORE_TOKEN, {
        fingerprint: SAMPLE_FP,
      })
    );
    const r = resolve(root);
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.source, "file");
    assert.strictEqual(
      r.url,
      "https://cloud.example/v1/slug/observe",
      "post target is observe_url"
    );
  });

  it("test_resolve_fileMode_populatesPinnedFpFromFingerprint", function () {
    // The load-bearing regression assertion: pinnedFp (never read before) now
    // equals the store's fingerprint.
    const root = makeProject();
    writeRemoteStore(
      root,
      remoteCred("https://cloud.example/observe", STORE_TOKEN, { fingerprint: SAMPLE_FP })
    );
    const r = resolve(root);
    assert.strictEqual(r.pinnedFp, SAMPLE_FP);
  });

  it("test_resolve_fileMode_readsTokenAndTimeouts", function () {
    const withT = makeProject();
    writeRemoteStore(
      withT,
      remoteCred("https://a.example/observe", STORE_TOKEN, {
        fingerprint: SAMPLE_FP,
        timeouts: { connect_ms: 111, sync_ms: 222, fnf_ms: 333 },
      })
    );
    const r1 = resolve(withT);
    assert.strictEqual(r1.token, STORE_TOKEN);
    assert.deepStrictEqual(r1.timeouts, { connectMs: 111, syncMs: 222, fnfMs: 333 });

    const noT = makeProject();
    writeRemoteStore(noT, remoteCred("https://b.example/observe", STORE_TOKEN, { fingerprint: SAMPLE_FP }));
    const r2 = resolve(noT);
    assert.deepStrictEqual(r2.timeouts, { ...DEFAULT_TIMEOUTS }, "absent timeouts → defaults");
  });

  it("test_resolve_oldUrlKeyNeverRead", function () {
    // A store with NO url key still resolves (the hook client never reads url);
    // a stray url key is ignored — observe_url is the only post target.
    const root = makeProject();
    writeRemoteStore(root, {
      mcp_url: "https://x.example/v1/s",
      observe_url: "https://x.example/v1/s/observe",
      url: "https://STRAY-SHOULD-BE-IGNORED.example/observe",
      token: STORE_TOKEN,
      fingerprint: SAMPLE_FP,
    });
    const r = resolve(root);
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.url, "https://x.example/v1/s/observe", "stray url key ignored");

    // And the source string never contains the production literal "url" guard.
    const src = fs.readFileSync(require.resolve("../../lib/hook-client/config.js"), "utf8");
    assert.ok(
      !/cred\.url\b/.test(src),
      "resolve() must never read cred.url (the old broken key)"
    );
  });

  it("test_resolve_precedencePreserved_envPairWinsUnpinned", function () {
    const root = makeProject();
    writeRemoteStore(
      root,
      remoteCred("https://store.example/observe", STORE_TOKEN, { fingerprint: SAMPLE_FP })
    );
    process.env[ENV_URL] = "https://env.example.com";
    process.env[ENV_TOKEN] = "env-token";
    const r = resolve(root);
    assert.strictEqual(r.source, "env", "env pair wins outright");
    assert.strictEqual(r.url, "https://env.example.com");
    assert.strictEqual(r.pinnedFp, null, "env path stays unpinned even with a store fingerprint present");
  });

  it("test_okHttp_gainsPinnedFpField", function () {
    // okHttp resolved config now carries pinnedFp threaded to transport-http.post
    // via config.pinnedFp — the minimal change that makes the observe path pin.
    const root = makeProject();
    writeRemoteStore(
      root,
      remoteCred("https://k.example/observe", STORE_TOKEN, { fingerprint: SAMPLE_FP })
    );
    const r = resolve(root);
    assert.ok(Object.prototype.hasOwnProperty.call(r, "pinnedFp"), "config carries a pinnedFp field");
    assert.strictEqual(r.pinnedFp, SAMPLE_FP);
  });
});

// ── R-15 — legacy fingerprint:null stays unpinned (AC-08c) ────────────

describe("config.resolve — R-15 legacy fingerprint:null", function () {
  it("test_resolve_nullFingerprint_resolvesUnpinned", function () {
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://legacy.example/observe", STORE_TOKEN, { fingerprint: null }));
    let r;
    assert.doesNotThrow(function () {
      r = resolve(root);
    });
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.source, "file");
    assert.strictEqual(r.pinnedFp, null, "null fingerprint → unpinned (preserves legacy), no crash");
  });

  it("test_resolve_presentFingerprint_pinned", function () {
    const root = makeProject();
    writeRemoteStore(root, remoteCred("https://bundle.example/observe", STORE_TOKEN, { fingerprint: SAMPLE_FP }));
    const r = resolve(root);
    assert.strictEqual(r.pinnedFp, SAMPLE_FP, "present fingerprint → pinned (driven by value, not pin-or-fail)");
  });
});

// ── R-13 — store read-error posture (hook-client mapping) ─────────────
//
// Same credstore.read throw, hook-client mapping: ENOENT/incomplete → UDS
// fall-through (fail-open); malformed/unknown-version → terminal `malformed`
// (the bridge fails loud on both — asymmetry asserted here for the hook side).

describe("config.resolve — R-13 store read-error posture", function () {
  it("test_resolve_storeEnoent_udsFallthrough", function () {
    const root = makeProject(); // no store written
    const r = resolve(root);
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.mode, "uds");
  });

  it("test_resolve_storeMalformed_terminalMalformed", function () {
    const root = makeProject();
    writeRemoteStore(root, "{ not valid json ::::");
    const r = resolve(root);
    assert.strictEqual(r.ok, false);
    assert.strictEqual(r.reason, "malformed");
    assert.strictEqual(r.mode, undefined);
  });

  it("test_resolve_unknownSchemaVersion_terminal", function () {
    const root = makeProject();
    writeRemoteStore(root, {
      schema_version: 999,
      observe_url: "https://x.example/observe",
      token: STORE_TOKEN,
      fingerprint: SAMPLE_FP,
    });
    const r = resolve(root);
    assert.strictEqual(r.ok, false, "unknown schema_version is terminal, not a silent skip");
    assert.strictEqual(r.reason, "malformed");
  });

  it("test_resolve_incompleteEntry_definedPosture", function () {
    // Missing observe_url → defined UDS fall-through, NOT an unpinned silent run.
    const root = makeProject();
    writeRemoteStore(root, { mcp_url: "https://x.example/v1/s", token: STORE_TOKEN, fingerprint: SAMPLE_FP });
    const r = resolve(root);
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.mode, "uds");
  });

  it("test_resolve_missingToken_udsFallthrough", function () {
    const root = makeProject();
    writeRemoteStore(root, { mcp_url: "https://x.example/v1/s", observe_url: "https://x.example/observe", fingerprint: SAMPLE_FP });
    const r = resolve(root);
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.mode, "uds");
  });
});

// ── R-07 — both consumers, one schema; keyed by projectHash round-trip ─

describe("config.resolve — R-07 one schema, one key", function () {
  it("test_resolve_keyedByProjectHash_roundTrip", function () {
    // Write the store for project P via credstore.write (the C4 writer path),
    // then resolve() reads it back keyed by the SAME computeProjectHash.
    const root = makeProject();
    const hash = computeProjectHash(root);
    credstore.write(hash, {
      mcp_url: "https://host/v1/slug",
      observe_url: "https://host/v1/slug/observe",
      token: STORE_TOKEN,
      fingerprint: SAMPLE_FP,
    });
    const r = resolve(root);
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.source, "file");
    assert.strictEqual(r.url, "https://host/v1/slug/observe");
    assert.strictEqual(r.token, STORE_TOKEN);
    assert.strictEqual(r.pinnedFp, SAMPLE_FP);
    assert.strictEqual(r.projectHash, hash, "read keyed by the same derivation as the write");
  });

  it("test_resolve_differentProjectHash_enoentUds", function () {
    // A different project's hash → ENOENT → UDS fall-through, not a crash.
    const written = makeProject();
    credstore.write(computeProjectHash(written), {
      mcp_url: "https://host/v1/slug",
      observe_url: "https://host/v1/slug/observe",
      token: STORE_TOKEN,
      fingerprint: SAMPLE_FP,
    });
    const other = makeProject();
    const r = resolve(other);
    assert.strictEqual(r.ok, true);
    assert.strictEqual(r.mode, "uds", "no store for this project → UDS, not a crash");
  });

  it("test_resolve_readsHookFieldsFromSharedStore", function () {
    // both-consumers-one-schema (C5 side): the hook client reads
    // observe_url/token/fingerprint/timeouts from the SAME file the bridge reads
    // mcp_url/token/fingerprint from — no per-consumer dialect. (C5 asserts the
    // hook fields; the mcp-bridge plan asserts the bridge fields on its side.)
    const root = makeProject();
    const hash = computeProjectHash(root);
    credstore.write(hash, {
      mcp_url: "https://host/v1/slug",
      observe_url: "https://host/v1/slug/observe",
      token: STORE_TOKEN,
      fingerprint: SAMPLE_FP,
      timeouts: { connect_ms: 700, sync_ms: 1900, fnf_ms: 2800 },
    });
    // The bridge-owned field is present in the same file but NOT consumed by C5.
    const raw = JSON.parse(fs.readFileSync(credstore.pathFor(hash), "utf8"));
    assert.strictEqual(raw.mcp_url, "https://host/v1/slug", "bridge field co-resident in one file");

    const r = resolve(root);
    assert.strictEqual(r.url, "https://host/v1/slug/observe");
    assert.strictEqual(r.token, STORE_TOKEN);
    assert.strictEqual(r.pinnedFp, SAMPLE_FP);
    assert.deepStrictEqual(r.timeouts, { connectMs: 700, syncMs: 1900, fnfMs: 2800 });
  });
});

// ── AC-08d — file-mode remote observe ACTUALLY runs over pinned HTTPS ──
//
// The break-fix PROOF (R-06, lesson #4970): a LIVE wire test against a LOCAL
// pinned https.createServer. Field-presence of pinnedFp is necessary but NOT
// sufficient — this section proves the observe POST resolved from the store
// transits a pinned HTTPS connection (good-pin delivers; wrong-pin fails connect
// with the token never on the wire). [no-cloud]: the server is LOCAL, the bridge
// is never required. Reuses the cert-pin-tls recipe (openssl self-signed leaf +
// computeFingerprint). Skips cleanly if openssl is unavailable.

function generateSelfSignedCert() {
  let tmpDir;
  try {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-vnc039-tls-"));
  } catch (_err) {
    return null;
  }
  const certPath = path.join(tmpDir, "cert.pem");
  const keyPath = path.join(tmpDir, "key.pem");
  let r;
  try {
    r = spawnSync(
      "openssl",
      [
        "req", "-x509", "-newkey", "rsa:2048",
        "-keyout", keyPath, "-out", certPath,
        "-days", "1", "-nodes", "-subj", "/CN=localhost",
      ],
      { stdio: "ignore" }
    );
  } catch (_err) {
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch (_e) {}
    return null;
  }
  if (r.error || r.status !== 0 || !fs.existsSync(certPath) || !fs.existsSync(keyPath)) {
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch (_e) {}
    return null;
  }
  return { tmpDir, certPem: fs.readFileSync(certPath), keyPem: fs.readFileSync(keyPath) };
}

const TLS_GEN = generateSelfSignedCert();
const TLS_SKIP = TLS_GEN
  ? false
  : { skip: "openssl unavailable — cannot generate the self-signed TLS fixture at runtime" };
const REAL_FP = TLS_GEN
  ? computeFingerprint(new crypto.X509Certificate(TLS_GEN.certPem).raw)
  : null;
const WRONG_FP = "sha256:" + "0".repeat(64);
const OBSERVE_TIMEOUTS = Object.freeze({ connectMs: 2000, syncMs: 4000, fnfMs: 4000 });

describe("config.resolve — AC-08d file-mode observe over pinned HTTPS (R-06)", { skip: TLS_SKIP }, function () {
  let server;
  let baseObserveUrl;
  let observedAuth;
  let observedPaths;

  beforeEach(async function () {
    observedAuth = [];
    observedPaths = [];
    server = https.createServer({ cert: TLS_GEN.certPem, key: TLS_GEN.keyPem }, (req, res) => {
      if (req.headers["authorization"]) observedAuth.push(req.headers["authorization"]);
      observedPaths.push(req.url);
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        res.writeHead(204);
        res.end();
      });
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const { port } = server.address();
    baseObserveUrl = "https://127.0.0.1:" + port + "/v1/slug/observe";
  });

  afterEach(async function () {
    await new Promise((resolve) => server.close(resolve));
  });

  it("test_observe_fileMode_goodPin_postsToObserveUrlOverPinnedHttps", async function () {
    // Seed the store with the LOCAL server's observe_url + REAL fp; resolve from
    // the project; POST the resolved config. It connects (it could NOT before —
    // it fell through to UDS on the missing url key) and lands on the HTTPS
    // server at observe_url's path.
    const root = makeProject();
    writeRemoteStore(
      root,
      remoteCred(baseObserveUrl, STORE_TOKEN, { fingerprint: REAL_FP, timeouts: { connect_ms: 2000, sync_ms: 4000, fnf_ms: 4000 } })
    );
    const cfg = resolve(root);
    assert.strictEqual(cfg.source, "file", "resolved to the http file path, NOT UDS");
    assert.strictEqual(cfg.mode, "http");
    assert.strictEqual(cfg.url, baseObserveUrl);
    assert.strictEqual(cfg.pinnedFp, REAL_FP);

    const res = await transport.post(cfg, { type: "Ping" }, { sync: false });
    assert.ok(res.ok, "good pin delivers over pinned HTTPS: " + JSON.stringify(res));
    assert.strictEqual(res.status, 204);
    assert.deepStrictEqual(observedPaths, ["/v1/slug/observe"], "lands on observe_url path");
    assert.ok(observedAuth.length > 0, "server received the authenticated POST on a good pin");
  });

  it("test_observe_fileMode_wrongPin_failOpenExit0_noTokenOnWire", async function () {
    const root = makeProject();
    writeRemoteStore(
      root,
      remoteCred(baseObserveUrl, STORE_TOKEN, { fingerprint: WRONG_FP, timeouts: { connect_ms: 2000, sync_ms: 4000, fnf_ms: 4000 } })
    );
    const cfg = resolve(root);
    assert.strictEqual(cfg.pinnedFp, WRONG_FP);

    const res = await transport.post(cfg, { type: "Ping" }, { sync: false });
    // Fail-open: resolves a connect failure, never throws (observe posture).
    assert.strictEqual(res.ok, false, "wrong pin fails connect-class");
    assert.strictEqual(res.failureClass, "connect");
    // The bearer never reached the server: no auth header, token never on wire.
    assert.strictEqual(observedAuth.length, 0, "server received NO authenticated request");
    assert.ok(
      observedAuth.every((a) => !a.includes(STORE_TOKEN)),
      "the Bearer token never reached the server on a mismatched pin"
    );
  });

  it("test_observe_fileMode_noUdsFallthrough_withValidRemoteCred", async function () {
    // With a valid file-mode remote credential present, observe targets
    // observe_url over HTTPS — it does NOT silently fall through to UDS (the
    // current break). Proven by the POST landing on the HTTPS server.
    const root = makeProject();
    writeRemoteStore(root, remoteCred(baseObserveUrl, STORE_TOKEN, { fingerprint: REAL_FP, timeouts: { connect_ms: 2000, sync_ms: 4000, fnf_ms: 4000 } }));
    const cfg = resolve(root);
    assert.strictEqual(cfg.mode, "http", "not UDS");
    assert.strictEqual(cfg.socketPath, undefined, "no socketPath — observe is http, not local UDS");
    const res = await transport.post(cfg, { type: "Ping" }, { sync: false });
    assert.ok(res.ok);
    assert.ok(observedPaths.length > 0, "observe POST reached the remote HTTPS server, not UDS");
  });
});

// Best-effort cleanup of the TLS fixture temp dir after the whole file.
if (TLS_GEN) {
  process.on("exit", function () {
    try { fs.rmSync(TLS_GEN.tmpDir, { recursive: true, force: true }); } catch (_e) {}
  });
}
