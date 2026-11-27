"use strict";
// C5 git-exclude (D-011, ADR-006, FR-40..FR-46). Real git in temp repos plus injected `run`.
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { ensureSettingsLocalIgnored, escapePrefix, REL } = require("../lib/git-exclude.js");

const ENTRY = "/" + REL;
const GENV = (() => {
  const e = Object.assign({}, process.env, { GIT_CONFIG_NOSYSTEM: "1", GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@t", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@t" });
  for (const k of ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_PREFIX"]) delete e[k];
  return e;
})();
let hasGit = true;
try { execFileSync("git", ["--version"], { stdio: "ignore" }); } catch (_) { hasGit = false; }
const needGit = hasGit ? {} : { skip: "git not installed" };
const posixOnly = process.platform === "win32" ? { skip: "posix" } : {};

const tmpd = (p) => fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), p || "gx-")));
const git = (cwd, ...a) => execFileSync("git", ["-c", "protocol.file.allow=always", ...a], { cwd, env: GENV, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
function repo() {
  const d = tmpd();
  git(d, "init", "-q");
  git(d, "config", "core.excludesFile", path.join(d, ".no-global-excludes"));
  return d;
}
function commitAll(d) { git(d, "add", "-A"); git(d, "commit", "-q", "--allow-empty", "-m", "c"); }
const excl = (d) => path.resolve(d, git(d, "rev-parse", "--git-path", "info/exclude").trim());
const read = (p) => (fs.existsSync(p) ? fs.readFileSync(p, "utf8") : null);
const ignored = (d) => { try { git(d, "check-ignore", "-q", "--", REL); return true; } catch (_) { return false; } };
function putLocal(d) { fs.mkdirSync(path.join(d, ".claude"), { recursive: true }); fs.writeFileSync(path.join(d, REL), "{}\n"); }
function cleanDiff(d) { git(d, "diff", "--exit-code"); git(d, "diff", "--cached", "--exit-code"); }
function recorder(root) {
  const calls = [];
  const real = (args) => {
    calls.push(args);
    try { return { status: 0, stdout: execFileSync("git", args, { cwd: root, env: GENV, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }) }; }
    catch (e) { return { status: e.status, stdout: String(e.stdout || "") }; }
  };
  return { calls, run: real };
}
const stub = (map) => (args) => { const k = args.slice(0, 2).join(" "); const v = map[k]; return typeof v === "function" ? v(args) : v || { status: 0, stdout: "" }; };

test("fresh repo: created, git status omits settings.local.json, second run unchanged", needGit, () => {
  const d = repo(); commitAll(d); putLocal(d);
  const x = excl(d); fs.rmSync(x, { force: true });
  const a = ensureSettingsLocalIgnored(d, {});
  assert.strictEqual(a.harness, "claude-code"); assert.strictEqual(a.surface, "git-exclude");
  assert.strictEqual(a.action, "created"); assert.strictEqual(a.path, x);
  assert.strictEqual(read(x), ENTRY + "\n");
  assert.ok(!git(d, "status", "--porcelain").includes("settings.local.json"));
  const m = fs.statSync(x).mtimeMs;
  const b = ensureSettingsLocalIgnored(d, {});
  assert.strictEqual(b.action, "unchanged"); assert.strictEqual(fs.statSync(x).mtimeMs, m);
  cleanDiff(d);
});

test("existing exclude without entry: updated, prior bytes kept; no trailing newline and CRLF", needGit, () => {
  for (const [prior, want] of [["# hi\n*.log\n", "# hi\n*.log\n" + ENTRY + "\n"], ["*.log", "*.log\n" + ENTRY + "\n"], ["a\r\nb\r\n", "a\r\nb\r\n" + ENTRY + "\r\n"]]) {
    const d = repo(); const x = excl(d); fs.writeFileSync(x, prior);
    const r = ensureSettingsLocalIgnored(d, {});
    assert.strictEqual(r.action, "updated"); assert.strictEqual(read(x), want); assert.ok(ignored(d));
  }
});

test("already ignored via .gitignore / core.excludesFile: unchanged, exclude untouched", needGit, () => {
  const d1 = repo(); fs.writeFileSync(path.join(d1, ".gitignore"), ".claude/\n");
  const x1 = excl(d1); const before = read(x1); const m = fs.existsSync(x1) && fs.statSync(x1).mtimeMs;
  assert.strictEqual(ensureSettingsLocalIgnored(d1, {}).action, "unchanged");
  assert.strictEqual(read(x1), before); if (m) assert.strictEqual(fs.statSync(x1).mtimeMs, m);
  const d2 = repo(); const g = path.join(d2, ".no-global-excludes"); fs.writeFileSync(g, "settings.local.json\n");
  assert.strictEqual(ensureSettingsLocalIgnored(d2, {}).action, "unchanged");
});

test("tracked file: skipped-tracked with warning; exclude and index unchanged", needGit, () => {
  const d = repo(); putLocal(d); commitAll(d);
  const x = excl(d); const before = read(x); const idx = fs.readFileSync(path.join(d, ".git", "index"));
  const r = ensureSettingsLocalIgnored(d, {});
  assert.strictEqual(r.action, "skipped-tracked"); assert.match(r.warning, /git rm --cached/);
  assert.strictEqual(read(x), before); assert.ok(fs.readFileSync(path.join(d, ".git", "index")).equals(idx));
  cleanDiff(d);
});

test("'!' override: appended-not-effective; repeat appends nothing", needGit, () => {
  const d = repo(); fs.writeFileSync(path.join(d, ".gitignore"), "!" + ENTRY + "\n");
  const x = excl(d);
  const a = ensureSettingsLocalIgnored(d, {});
  assert.strictEqual(a.action, "appended-not-effective"); assert.ok(a.warning);
  const after = read(x); assert.strictEqual(after.split("\n").filter((l) => l === ENTRY).length, 1);
  const b = ensureSettingsLocalIgnored(d, {});
  assert.strictEqual(b.action, "appended-not-effective"); assert.match(b.warning, /already lists/);
  assert.strictEqual(read(x), after);
});

test("existing entry with trailing spaces/CR counts as listed", () => {
  const d = tmpd(); const x = path.join(d, "exclude"); fs.writeFileSync(x, ENTRY + "  \r\n");
  const r = ensureSettingsLocalIgnored(d, { run: stub({ "rev-parse --is-inside-work-tree": { status: 0, stdout: "true\n" }, "ls-files --error-unmatch": { status: 1, stdout: "" }, "check-ignore -q": { status: 1, stdout: "" }, "rev-parse --git-path": { status: 0, stdout: x + "\n" }, "rev-parse --show-prefix": { status: 0, stdout: "\n" } }) });
  assert.strictEqual(r.action, "appended-not-effective"); assert.strictEqual(read(x), ENTRY + "  \r\n");
});

test("non-repo root: skipped-not-repo with path R", needGit, () => {
  const d = tmpd();
  const r = ensureSettingsLocalIgnored(d, { run: (a) => { try { return { status: 0, stdout: execFileSync("git", a, { cwd: d, env: Object.assign({}, GENV, { GIT_CEILING_DIRECTORIES: path.dirname(d) }), encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }) }; } catch (e) { return { status: e.status, stdout: "" }; } } });
  assert.strictEqual(r.action, "skipped-not-repo"); assert.strictEqual(r.path, d);
});

test("PATH without git: skipped-no-git", needGit, () => {
  const d = repo(); const saved = process.env.PATH;
  process.env.PATH = tmpd("nogit-");
  try { assert.strictEqual(ensureSettingsLocalIgnored(d, {}).action, "skipped-no-git"); }
  finally { process.env.PATH = saved; }
});

test("injected git failures map to skipped-git-error / skipped-not-repo", () => {
  const ok = { status: 0, stdout: "true\n" };
  const cases = [
    [{ "rev-parse --is-inside-work-tree": { status: 128, stdout: "" } }, "skipped-not-repo"],
    [{ "rev-parse --is-inside-work-tree": { status: 0, stdout: "false\n" } }, "skipped-not-repo"],
    [{ "rev-parse --is-inside-work-tree": { status: null, error: "ETIMEDOUT" } }, "skipped-git-error"],
    [{ "rev-parse --is-inside-work-tree": ok, "ls-files --error-unmatch": { status: 1 }, "check-ignore -q": { status: 128 } }, "skipped-git-error"],
    [{ "rev-parse --is-inside-work-tree": ok, "ls-files --error-unmatch": { status: 1 }, "check-ignore -q": { status: 1 }, "rev-parse --git-path": { status: 128 } }, "skipped-git-error"],
    [{ "rev-parse --is-inside-work-tree": ok, "ls-files --error-unmatch": { status: 1 }, "check-ignore -q": { status: 0 } }, "unchanged"],
  ];
  for (const [m, want] of cases) assert.strictEqual(ensureSettingsLocalIgnored("/nonexistent-root", { run: stub(m) }).action, want);
});

test("real git hang is bounded: timeout maps to skipped-git-error", posixOnly, () => {
  const bin = tmpd("fakegit-"); fs.writeFileSync(path.join(bin, "git"), "#!/bin/sh\nexec /bin/sleep 30\n", { mode: 0o755 });
  const saved = process.env.PATH; process.env.PATH = bin;
  const t0 = Date.now();
  try { assert.strictEqual(ensureSettingsLocalIgnored(tmpd(), {}).action, "skipped-git-error"); }
  finally { process.env.PATH = saved; }
  assert.ok(Date.now() - t0 < 20000);
});

test("linked worktree: entry lands in the common-dir exclude", needGit, () => {
  const d = repo(); commitAll(d); const wt = path.join(tmpd(), "wt");
  git(d, "worktree", "add", "-q", wt);
  const x = excl(wt);
  const r = ensureSettingsLocalIgnored(wt, {});
  assert.strictEqual(r.path, x); assert.ok(read(x).split("\n").includes(ENTRY)); assert.ok(ignored(wt));
});

test("submodule root: entry in <super>/.git/modules/<n>/info/exclude", needGit, () => {
  const sub = repo(); commitAll(sub); const sup = repo(); commitAll(sup);
  git(sup, "submodule", "add", "-q", sub, "mod");
  const root = path.join(sup, "mod");
  git(root, "config", "core.excludesFile", path.join(sup, ".no-global-excludes"));
  const r = ensureSettingsLocalIgnored(root, {});
  assert.strictEqual(r.path, path.join(sup, ".git", "modules", "mod", "info", "exclude"));
  assert.ok(["created", "updated"].includes(r.action)); assert.ok(ignored(root));
});

test("subdir root with special chars: prefixed escaped entry, then ignored", needGit, () => {
  const d = repo(); const sub = path.join(d, "x", "a*b[1]!"); fs.mkdirSync(sub, { recursive: true });
  const r = ensureSettingsLocalIgnored(sub, {});
  assert.ok(read(excl(d)).split("\n").includes("/x/a\\*b\\[1]\\!/" + REL));
  assert.notStrictEqual(r.action, "appended-not-effective"); assert.ok(ignored(sub));
  assert.strictEqual(escapePrefix("p q /"), "p q\\ /");
});

test("missing info/ dir is created", needGit, () => {
  const d = repo(); fs.rmSync(path.join(d, ".git", "info"), { recursive: true, force: true });
  assert.strictEqual(ensureSettingsLocalIgnored(d, {}).action, "created"); assert.ok(ignored(d));
});

test("symlinked exclude stays a link; target appended", { ...needGit, ...posixOnly }, () => {
  const d = repo(); const x = excl(d); const tgt = path.join(tmpd(), "real-exclude"); fs.writeFileSync(tgt, "*.o\n");
  fs.rmSync(x, { force: true }); fs.symlinkSync(tgt, x);
  ensureSettingsLocalIgnored(d, {});
  assert.ok(fs.lstatSync(x).isSymbolicLink()); assert.strictEqual(read(tgt), "*.o\n" + ENTRY + "\n");
});

test("parent GIT_DIR pointing at another repo is scrubbed; other repo untouched", needGit, () => {
  const d = repo(); const other = repo(); const ox = excl(other); const before = read(ox);
  const saved = process.env.GIT_DIR; process.env.GIT_DIR = path.join(other, ".git");
  try { ensureSettingsLocalIgnored(d, {}); } finally { if (saved === undefined) delete process.env.GIT_DIR; else process.env.GIT_DIR = saved; }
  assert.strictEqual(read(ox), before); assert.ok(read(excl(d)).includes(ENTRY));
  assert.strictEqual(process.env.GIT_DIR, saved);
});

test("dryRun: reports created/updated, writes nothing, read-only git only", needGit, () => {
  const d = repo(); const x = excl(d);
  for (const [setup, want] of [[() => fs.rmSync(x, { force: true }), "created"], [() => fs.writeFileSync(x, "*.o\n"), "updated"]]) {
    setup(); const before = read(x); const rec = recorder(d);
    const r = ensureSettingsLocalIgnored(d, { dryRun: true, run: rec.run });
    assert.strictEqual(r.action, want); assert.strictEqual(read(x), before);
    for (const a of rec.calls) assert.ok(["rev-parse", "ls-files", "check-ignore"].includes(a[0]), a.join(" "));
  }
});

test("already listed in existing exclude: unchanged, bytes and mtime equal, git diff clean", needGit, () => {
  const d = repo(); putLocal(d);
  const x = excl(d); fs.writeFileSync(x, "*.o\n" + ENTRY + "\n");
  const before = read(x); const m = fs.statSync(x).mtimeMs;
  const r = ensureSettingsLocalIgnored(d, {});
  assert.strictEqual(r.action, "unchanged"); assert.strictEqual(read(x), before); assert.strictEqual(fs.statSync(x).mtimeMs, m);
  cleanDiff(d);
});

test("linked worktree and submodule: entry in git-reported file and git diff clean", needGit, () => {
  const d = repo(); commitAll(d); const wt = path.join(tmpd(), "wt");
  git(d, "worktree", "add", "-q", wt);
  const r = ensureSettingsLocalIgnored(wt, {});
  assert.ok(["created", "updated"].includes(r.action)); assert.ok(read(r.path).split("\n").includes(ENTRY));
  cleanDiff(wt); cleanDiff(d);
  const sub = repo(); commitAll(sub); const sup = repo(); commitAll(sup);
  git(sup, "-c", "protocol.file.allow=always", "submodule", "add", "-q", sub, "mod");
  const root = path.join(sup, "mod");
  git(root, "config", "core.excludesFile", path.join(sup, ".no-global-excludes"));
  const s = ensureSettingsLocalIgnored(root, {});
  assert.ok(["created", "updated"].includes(s.action), JSON.stringify(s)); assert.ok(ignored(root));
  cleanDiff(root);
});
