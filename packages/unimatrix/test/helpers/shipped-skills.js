"use strict";

// In-process overlay for the shipped skills tree (`<pkg>/skills`). installSkills
// reads that tree via the `fs` module object, so tests inject fixture skills by
// patching fs in THIS process only. Writing into the real package dir instead
// races sibling test files that node --test runs in parallel processes (they
// readdir a skill dir another file has just removed -> ENOENT flake).

const fs = require("fs");
const os = require("os");
const path = require("path");

const SKILLS_SOURCE = path.join(__dirname, "..", "..", "skills");

/**
 * Run sync `fn` with `manifest` ({ skillDir: { fileName: content } }) visible
 * as extra shipped skills. The real tree is never modified.
 */
function withShippedSkills(manifest, fn) {
  const overlay = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-skills-overlay-"));
  for (const [name, files] of Object.entries(manifest)) {
    fs.mkdirSync(path.join(overlay, name), { recursive: true });
    for (const [file, content] of Object.entries(files)) {
      fs.writeFileSync(path.join(overlay, name, file), content);
    }
  }
  const names = new Set(Object.keys(manifest));
  const map = (p) => {
    if (typeof p !== "string" || !p.startsWith(SKILLS_SOURCE + path.sep)) return p;
    const rel = p.slice(SKILLS_SOURCE.length + 1);
    return names.has(rel.split(path.sep)[0]) ? path.join(overlay, rel) : p;
  };
  const orig = {};
  for (const k of ["existsSync", "statSync", "copyFileSync", "readFileSync", "readdirSync"]) {
    orig[k] = fs[k];
  }
  fs.existsSync = (p, ...a) => orig.existsSync(map(p), ...a);
  fs.statSync = (p, ...a) => orig.statSync(map(p), ...a);
  fs.readFileSync = (p, ...a) => orig.readFileSync(map(p), ...a);
  fs.copyFileSync = (s, d, ...a) => orig.copyFileSync(map(s), d, ...a);
  fs.readdirSync = (p, ...a) => {
    const out = orig.readdirSync(map(p), ...a);
    if (typeof p === "string" && path.resolve(p) === SKILLS_SOURCE) {
      return out
        .filter((e) => !names.has(typeof e === "string" ? e : e.name))
        .concat(orig.readdirSync(overlay, ...a));
    }
    return out;
  };
  try {
    return fn();
  } finally {
    Object.assign(fs, orig);
    fs.rmSync(overlay, { recursive: true, force: true });
  }
}

module.exports = { SKILLS_SOURCE, withShippedSkills };
