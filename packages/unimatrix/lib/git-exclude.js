"use strict";
// C5 — keep .claude/settings.local.json git-ignored via the per-repo exclude file
// resolved by `git rev-parse --git-path info/exclude` (D-011, ADR-006, FR-40..FR-46).
// Never throws except SafeWriteError from the append.
const path = require("node:path");
const fs = require("node:fs");
const { execFileSync } = require("node:child_process");
const safeWrite = require("./safe-write.js");

const REL = ".claude/settings.local.json";
const SCRUB = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_PREFIX"];
const GIT_TIMEOUT_MS = 10000;

function defaultRun(root) {
  const env = Object.assign({}, process.env);
  for (const k of SCRUB) delete env[k];
  return (args) => {
    try {
      const out = execFileSync("git", args, { cwd: root, env, stdio: ["ignore", "pipe", "pipe"], timeout: GIT_TIMEOUT_MS, encoding: "utf8", windowsHide: true });
      return { status: 0, stdout: out };
    } catch (e) {
      if (e && e.code === "ENOENT") return { status: null, error: "ENOENT" };
      if (e && typeof e.status === "number") return { status: e.status, stdout: String(e.stdout || "") };
      return { status: null, error: (e && (e.code || e.signal || e.message)) || "unknown" };
    }
  };
}

function leg(action, p, reason, warning) {
  const o = { harness: "claude-code", surface: "git-exclude", action, path: p, reason };
  if (warning) o.warning = warning;
  return o;
}

function escapePrefix(p) {
  return p.replace(/[*?[\\!]/g, (ch) => "\\" + ch).replace(/ (\/?)$/, "\\ $1");
}

function computeAppend(raw, entry) {
  const lines = (raw || "").split("\n").map((l) => l.replace(/\r$/, "").replace(/\s+$/, ""));
  if (lines.includes(entry)) return null;
  const base = raw || "";
  const sep = base !== "" && !base.endsWith("\n") ? (base.includes("\r\n") ? "\r\n" : "\n") : "";
  const eol = base.includes("\r\n") ? "\r\n" : "\n";
  return base + sep + entry + eol;
}

function ensureSettingsLocalIgnored(root, opts) {
  const o = opts || {};
  const dryRun = !!o.dryRun;
  const git = o.run || defaultRun(root);
  const target = path.join(root, REL);

  let r = git(["rev-parse", "--is-inside-work-tree"]);
  if (r.error === "ENOENT") return leg("skipped-no-git", root, "git not found; cannot ensure .claude/settings.local.json is git-ignored");
  if (r.error) return leg("skipped-git-error", root, "git failed: " + r.error);
  if (r.status !== 0 || String(r.stdout || "").trim() !== "true") return leg("skipped-not-repo", root, "not inside a git work tree; nothing to ignore");

  r = git(["ls-files", "--error-unmatch", "--", REL]);
  if (r.status === 0) {
    const w = ".claude/settings.local.json is tracked by git, so ignore rules do not apply and local wiring is committable; untrack it with 'git rm --cached .claude/settings.local.json'";
    return leg("skipped-tracked", target, "tracked by git", w);
  }
  if (r.error) return leg("skipped-git-error", root, "git failed: " + r.error);

  r = git(["check-ignore", "-q", "--", REL]);
  if (r.status === 0) return leg("unchanged", target, "already git-ignored");
  if (r.status !== 1) return leg("skipped-git-error", root, `git check-ignore failed (exit ${r.status === null ? r.error : r.status})`);

  const gp = git(["rev-parse", "--git-path", "info/exclude"]);
  const sp = git(["rev-parse", "--show-prefix"]);
  if (gp.status !== 0 || sp.status !== 0) return leg("skipped-git-error", root, "git rev-parse failed");
  const x = path.resolve(root, String(gp.stdout).trim());
  const entry = "/" + escapePrefix(String(sp.stdout).replace(/\r?\n$/, "")) + REL;

  if (!dryRun) {
    try {
      fs.mkdirSync(path.dirname(x), { recursive: true });
    } catch (_) {
      /* the append below reports the failure as SafeWriteError */
    }
  }
  const res = safeWrite.updateFileSync(x, (raw) => computeAppend(raw, entry), { dryRun, newMode: 0o600 });
  if (computeAppend(res.before, entry) === null) {
    const w = `${x} already lists ${entry} but git does not ignore .claude/settings.local.json; a higher-precedence rule (for example '!' in .gitignore) overrides it`;
    return leg("appended-not-effective", x, "already listed; still not ignored", w);
  }
  const out = leg(res.action, x, "added " + entry);
  if (!dryRun) {
    r = git(["check-ignore", "-q", "--", REL]);
    if (r.status !== 0) {
      out.action = "appended-not-effective";
      out.reason = `added ${entry}; still not ignored (higher-precedence rule)`;
      out.warning = `${x}: added ${entry} but git still does not ignore .claude/settings.local.json; a higher-precedence rule overrides it`;
    }
  }
  return out;
}

module.exports = { ensureSettingsLocalIgnored, escapePrefix, REL, SCRUB };
