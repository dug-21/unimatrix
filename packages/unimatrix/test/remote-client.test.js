"use strict";

// remote-client.test.js — vnc-034 Wave-1 RemoteClient (#725). Covers the C1
// bundle decoder (bundle.js), the C2 cert pin (cert-pin.js), and the
// `init --remote <bundle> [--slug s]` flow (init.js initRemote bundle path).
//
// PARITY: the fingerprint + bundle expectations are READ FROM the committed Rust
// oracle corpus (crates/unimatrix-server/tests/fixtures/c1c2-parity/). The JS
// golden is NEVER hand-written (SR-02 / ADR-002 / ADR-006). Cumulative infra —
// extends the existing node:test suites.

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const {
  decodeBundle,
  assertSlugAllowlist,
  BundleError,
  MAX_RAW_LEN,
} = require("../lib/hook-client/bundle.js");
const {
  computeFingerprint,
  makeCheckServerIdentity,
  applyCertPin,
} = require("../lib/hook-client/cert-pin.js");

const initModule = require("../lib/init.js");
const { initRemote, resolveRemoteTarget } = initModule;
const transport = require("../lib/hook-client/transport-http.js");

// ----------------------------------------------------------------------------
// Committed Rust-oracle parity corpus (NEVER hand-write these values).
// ----------------------------------------------------------------------------
const CORPUS_DIR = path.join(
  __dirname,
  "..",
  "..",
  "..",
  "crates",
  "unimatrix-server",
  "tests",
  "fixtures",
  "c1c2-parity"
);
const FINGERPRINT_GOLDEN = JSON.parse(
  fs.readFileSync(path.join(CORPUS_DIR, "fingerprint-golden.json"), "utf8")
);
const BUNDLE_GOLDEN = JSON.parse(
  fs.readFileSync(path.join(CORPUS_DIR, "bundle-golden.json"), "utf8")
);

// ----------------------------------------------------------------------------
// Test helpers (cumulative — mirror init-remote.test.js conventions).
// ----------------------------------------------------------------------------
function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-vnc034-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

function readSettingsLocal(projectRoot) {
  const fp = path.join(projectRoot, ".claude", "settings.local.json");
  return JSON.parse(fs.readFileSync(fp, "utf8"));
}

// Stub pingForInit; capture the args it was called with so the pinned-fp thread
// is observable. Restore in afterEach.
let origPing;
let lastPingArgs;
function stubPing(fn) {
  origPing = transport.pingForInit;
  lastPingArgs = null;
  transport.pingForInit = function (...args) {
    lastPingArgs = args;
    return fn(...args);
  };
}
function okPing() {
  return Promise.resolve({ ok: true, message: "Pong from host (server x)" });
}

// A valid bundle string from the committed corpus (first entry).
const GOLDEN_BUNDLE = BUNDLE_GOLDEN[0];
const VALID_WIRE = GOLDEN_BUNDLE.wire;

// ============================================================================
// C2 — cert pinning (R-02 / AC-W1-C2)
// ============================================================================
describe("cert pin — computeFingerprint parity (R-02 / SR-02)", () => {
  it("test_checkserveridentity_computes_pin_over_cert_raw — matches Rust oracle for every golden DER", () => {
    assert.ok(FINGERPRINT_GOLDEN.length > 0, "corpus must be non-empty");
    for (const entry of FINGERPRINT_GOLDEN) {
      const der = Buffer.from(entry.der_hex, "hex");
      const got = computeFingerprint(der);
      assert.strictEqual(
        got,
        entry.fp,
        "fingerprint parity for der_hex=" + entry.der_hex.slice(0, 16) + "..."
      );
      // Format contract: sha256:<64 lowercase hex>.
      assert.match(got, /^sha256:[0-9a-f]{64}$/);
    }
  });
});

describe("cert pin — checkServerIdentity (R-02 / AC-W1-C2 / AC-CT-ROT)", () => {
  // Reuse a corpus DER as the "served leaf" so the pinned value is oracle-derived.
  const LEAF = FINGERPRINT_GOLDEN[2];
  const leafDer = Buffer.from(LEAF.der_hex, "hex");
  const pinnedFp = LEAF.fp;

  it("test_pin_match_accepts — matching cert → undefined (accept)", () => {
    const check = makeCheckServerIdentity(pinnedFp);
    const result = check("host.example", { raw: leafDer });
    assert.strictEqual(result, undefined);
  });

  it("test_pin_mismatch_rejects_with_diagnosable_error — names expected vs presented + remediation", () => {
    const wrongDer = Buffer.from(FINGERPRINT_GOLDEN[3].der_hex, "hex");
    const presented = FINGERPRINT_GOLDEN[3].fp;
    const check = makeCheckServerIdentity(pinnedFp);
    const result = check("host.example", { raw: wrongDer });
    assert.ok(result instanceof Error, "mismatch must return an Error");
    // Diagnosable contract (AC-CT-ROT): BOTH fingerprints + remediation present.
    assert.ok(result.message.includes(pinnedFp), "names expected (pinned) fp");
    assert.ok(result.message.includes(presented), "names presented (server) fp");
    assert.ok(
      result.message.includes("client-bundle") &&
        result.message.includes("init --remote"),
      "points at re-bundle remediation"
    );
    // NOT a bare opaque TLS handshake error.
    assert.ok(
      /mismatch/i.test(result.message),
      "is the explicit pin-mismatch message"
    );
  });

  it("test_no_cert_presented → diagnosable error, not a crash", () => {
    const check = makeCheckServerIdentity(pinnedFp);
    assert.ok(check("h", null) instanceof Error);
    assert.ok(check("h", {}) instanceof Error);
  });

  it("test_pin_bypasses_ca_chain — applyCertPin sets pin path, clears CA trust", () => {
    const opts = {};
    applyCertPin(opts, true, pinnedFp);
    assert.strictEqual(typeof opts.checkServerIdentity, "function");
    assert.strictEqual(opts.rejectUnauthorized, true);
    assert.strictEqual(opts.ca, undefined, "no CA trust path is supplied");
    // Non-TLS / unpinned → no-op (no identity override on plain http).
    const plain = {};
    applyCertPin(plain, false, pinnedFp);
    assert.strictEqual(plain.checkServerIdentity, undefined);
    const unpinned = {};
    applyCertPin(unpinned, true, null);
    assert.strictEqual(unpinned.checkServerIdentity, undefined);
  });
});

// ============================================================================
// C1 — bundle decode (R-05 / AC-W1-C9 / AC-W1-C10)
// ============================================================================
describe("bundle decode — parity vs Rust encoder (R-05 sc.3)", () => {
  it("test_decode_roundtrip_matches_oracle — every committed wire decodes to its fields", () => {
    assert.ok(BUNDLE_GOLDEN.length > 0, "corpus must be non-empty");
    for (const entry of BUNDLE_GOLDEN) {
      const got = decodeBundle(entry.wire);
      assert.deepStrictEqual(got, {
        v: 1,
        base_url: entry.fields.base_url,
        token: entry.fields.token,
        fp: entry.fields.fp,
      });
    }
  });
});

describe("bundle decode — guard ordering (AC-W1-C10)", () => {
  it("test_length_cap_before_decode — over-cap invalid-base64url rejects on LENGTH", () => {
    // A string longer than the cap that is ALSO not valid base64url and lacks the
    // scheme: must reject on the length guard (GUARD 1), proving the cap runs
    // before any decode/scheme work.
    const overCap = "!".repeat(MAX_RAW_LEN + 1); // each '!' is 1 byte
    assert.ok(Buffer.byteLength(overCap, "utf8") > MAX_RAW_LEN);
    assert.throws(
      () => decodeBundle(overCap),
      (err) => err instanceof BundleError && /too long/.test(err.message)
    );
  });

  it("test_at_cap_boundary — exactly cap bytes is NOT rejected on length", () => {
    // Pad a valid scheme'd string to exactly MAX_RAW_LEN; it will fail a LATER
    // guard (not the length guard) — proving the boundary is inclusive.
    const base = "unimatrix-bundle:";
    const pad = "A".repeat(MAX_RAW_LEN - Buffer.byteLength(base, "utf8"));
    const atCap = base + pad;
    assert.strictEqual(Buffer.byteLength(atCap, "utf8"), MAX_RAW_LEN);
    assert.throws(
      () => decodeBundle(atCap),
      (err) => err instanceof BundleError && !/too long/.test(err.message),
      "at-cap must fail on a non-length guard"
    );
  });
});

describe("bundle decode — strict schema reject (AC-W1-C9, load-bearing)", () => {
  // Build wires from arbitrary objects to exercise schema rejection.
  function wireFrom(obj) {
    const json = JSON.stringify(obj);
    return "unimatrix-bundle:" + Buffer.from(json, "utf8").toString("base64url");
  }
  const VALID_FIELDS = GOLDEN_BUNDLE.fields;

  it("test_missing_field_rejected", () => {
    const o = Object.assign({}, VALID_FIELDS);
    delete o.fp;
    assert.throws(() => decodeBundle(wireFrom(o)), BundleError);
  });

  it("test_extra_field_rejected", () => {
    const o = Object.assign({}, VALID_FIELDS, { extra: "x" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /unexpected fields/.test(err.message)
    );
  });

  it("test_unsupported_version_rejected", () => {
    const o = Object.assign({}, VALID_FIELDS, { v: 2 });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /unsupported bundle version/.test(err.message)
    );
  });

  it("test_non_https_base_url_rejected", () => {
    const o = Object.assign({}, VALID_FIELDS, { base_url: "http://cloud.example:8443" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /https/.test(err.message)
    );
  });

  it("test_non_hex_token_rejected", () => {
    const o = Object.assign({}, VALID_FIELDS, { token: "ZZZ" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /token/.test(err.message)
    );
  });

  it("test_malformed_fp_rejected", () => {
    const o = Object.assign({}, VALID_FIELDS, { fp: "sha256:nothex" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /fp/.test(err.message)
    );
  });

  it("test_wrong_type_field_rejected", () => {
    const o = Object.assign({}, VALID_FIELDS, { token: 12345 });
    assert.throws(() => decodeBundle(wireFrom(o)), BundleError);
  });

  it("test_valid_base64url_invalid_json_rejected", () => {
    const wire = "unimatrix-bundle:" + Buffer.from("{not json", "utf8").toString("base64url");
    assert.throws(
      () => decodeBundle(wire),
      (err) => err instanceof BundleError && /JSON/.test(err.message)
    );
  });

  it("test_missing_scheme_rejected", () => {
    assert.throws(
      () => decodeBundle("eyJ2IjoxfQ"),
      (err) => err instanceof BundleError && /prefix/.test(err.message)
    );
  });

  it("test_token_never_in_error_message — schema reject on a bundle bearing a real token", () => {
    // Wrong-type fp but a valid 64-hex token present: the token must NOT leak.
    const o = Object.assign({}, VALID_FIELDS, { fp: 999 });
    let msg = "";
    try {
      decodeBundle(wireFrom(o));
    } catch (err) {
      msg = err.message;
    }
    assert.ok(msg.length > 0, "must throw");
    assert.ok(!msg.includes(VALID_FIELDS.token), "token must not appear in the error");
  });
});

// ============================================================================
// Slug allowlist (C5 / ADR-004)
// ============================================================================
describe("slug allowlist (C5 / ADR-004)", () => {
  it("test_valid_slugs_accepted", () => {
    for (const s of ["a", "abc", "my-project", "p1", "a".repeat(63)]) {
      assert.doesNotThrow(() => assertSlugAllowlist(s), "slug: " + s);
    }
  });
  it("test_invalid_slugs_rejected", () => {
    for (const s of ["", "-leading", "UPPER", "has_underscore", "has space", "a".repeat(64), "x/y"]) {
      assert.throws(() => assertSlugAllowlist(s), BundleError, "slug: " + s);
    }
  });
});

// ============================================================================
// initRemote bundle path (R-05 / R-06 / AC-W1-C5 / C6)
// ============================================================================
describe("initRemote — bundle path endpoint derivation (R-06 / C5)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_slug_appended_to_base_url — --slug foo → base_url/v1/foo", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, slug: "foo", projectDir: dir });
    const sl = readSettingsLocal(dir);
    assert.strictEqual(
      sl.unimatrix.remote.url,
      GOLDEN_BUNDLE.fields.base_url + "/v1/foo"
    );
    assert.strictEqual(sl.unimatrix.remote.token, GOLDEN_BUNDLE.fields.token);
    assert.strictEqual(sl.unimatrix.remote.fingerprint, GOLDEN_BUNDLE.fields.fp);
  });

  it("test_no_slug_default_alias — no --slug → base_url/v1", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    const sl = readSettingsLocal(dir);
    assert.strictEqual(sl.unimatrix.remote.url, GOLDEN_BUNDLE.fields.base_url + "/v1");
  });

  it("test_bad_slug_rejected_no_config_written", async () => {
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ bundle: VALID_WIRE, slug: "Bad Slug", projectDir: dir }),
      BundleError
    );
    assert.ok(
      !fs.existsSync(path.join(dir, ".claude", "settings.local.json")),
      "no config on a parse-edge rejection"
    );
  });

  it("test_pinned_fp_threaded_into_ping — Ping runs over the pinned connection", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    // pingForInit(url, token, timeouts, pinnedFp)
    assert.strictEqual(lastPingArgs[3], GOLDEN_BUNDLE.fields.fp);
  });
});

describe("initRemote — 1:1 unrepresentable (R-06 / AC-W1-C5)", () => {
  it("test_client_has_no_second_project_field — config bakes exactly one endpoint", () => {
    // resolveRemoteTarget yields a flat {remote, token, pinnedFp}: there is no
    // array/list/second-endpoint field by which a second project can be named.
    const t = resolveRemoteTarget({ bundle: VALID_WIRE, slug: "only" });
    assert.deepStrictEqual(Object.keys(t).sort(), ["pinnedFp", "remote", "token"]);
    assert.strictEqual(typeof t.remote, "string");
    // The endpoint is a single string; cross-project fan-out is unrepresentable.
    assert.ok(t.remote.endsWith("/v1/only"));
  });
});

describe("initRemote — malformed bundle reaches init (R-05 / AC-W1-C9-C10)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_init_remote_rejects_malformed_bundle — no config written, process survives", async () => {
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ bundle: "unimatrix-bundle:!!!notbase64!!!", projectDir: dir }),
      BundleError
    );
    assert.ok(!fs.existsSync(path.join(dir, ".claude", "settings.local.json")));
  });

  it("test_init_remote_length_cap_before_decode — over-cap arg rejects on length", async () => {
    const dir = makeTempProject();
    const overCap = "@".repeat(MAX_RAW_LEN + 50);
    await assert.rejects(
      () => initRemote({ bundle: overCap, projectDir: dir }),
      (err) => err instanceof BundleError && /too long/.test(err.message)
    );
  });
});

// ============================================================================
// Onboarding artifacts (AC-W1-C6) + secret hygiene (R-12 / NFR-06)
// ============================================================================
describe("initRemote — onboarding artifacts (AC-W1-C6)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_skills_copied — .claude/skills present after init", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    const skillsDir = path.join(dir, ".claude", "skills");
    // Skills are copied when the package ships a skills/ dir; assert the copy
    // path ran (dir exists) OR the package has no bundled skills (documented skip).
    const sourceDir = path.join(__dirname, "..", "skills");
    if (fs.existsSync(sourceDir)) {
      assert.ok(fs.existsSync(skillsDir), "skills copied into project");
    }
  });

  it("test_claudemd_block_not_appended — init does not write CLAUDE.md", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    assert.ok(
      !fs.existsSync(path.join(dir, "CLAUDE.md")),
      "init must not append a CLAUDE.md knowledge block (uni-init owns it)"
    );
  });

  it("test_unimatrix_init_pointer_printed — stdout names /unimatrix-init", async () => {
    const dir = makeTempProject();
    const lines = [];
    const origLog = console.log;
    console.log = (...a) => lines.push(a.join(" "));
    try {
      await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    } finally {
      console.log = origLog;
    }
    assert.ok(
      lines.some((l) => l.includes("/unimatrix-init")),
      "the /unimatrix-init pointer is printed"
    );
  });
});

describe("initRemote — token hygiene (R-12 / NFR-06)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_token_not_logged_by_client — token never on stdout/stderr", async () => {
    const dir = makeTempProject();
    const tok = GOLDEN_BUNDLE.fields.token;
    const out = [];
    const origLog = console.log;
    const origErrWrite = process.stderr.write.bind(process.stderr);
    console.log = (...a) => out.push(a.join(" "));
    process.stderr.write = (s) => {
      out.push(String(s));
      return true;
    };
    try {
      await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    } finally {
      console.log = origLog;
      process.stderr.write = origErrWrite;
    }
    const all = out.join("\n");
    assert.ok(!all.includes(tok), "token must not appear in any console output");
    // But it MUST land in the persisted config.
    assert.strictEqual(readSettingsLocal(dir).unimatrix.remote.token, tok);
  });

  it("test_settings_local_mode_0600", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    const slPath = path.join(dir, ".claude", "settings.local.json");
    const mode = fs.statSync(slPath).mode & 0o777;
    if (process.platform !== "win32") {
      assert.strictEqual(mode, 0o600, "token-bearing file is 0600");
    }
  });
});

// ============================================================================
// Install footprint < 250 KB (AC-W1-C3 / NFR-01 / R-12) — HARD GATE
// ============================================================================
describe("remote install footprint (AC-W1-C3 / NFR-01)", () => {
  const SIZE_LIMIT = 250 * 1024; // 250 KB

  // The shipped remote install is the pure-JS client (lib/) + skills/, with NO
  // native binary and NO model. Walk both trees and sum file bytes.
  function dirBytes(root) {
    let total = 0;
    const stack = [root];
    while (stack.length) {
      const cur = stack.pop();
      let entries;
      try {
        entries = fs.readdirSync(cur, { withFileTypes: true });
      } catch (_err) {
        continue;
      }
      for (const e of entries) {
        const p = path.join(cur, e.name);
        if (e.isDirectory()) {
          stack.push(p);
        } else if (e.isFile()) {
          total += fs.statSync(p).size;
        }
      }
    }
    return total;
  }

  it("test_remote_install_under_250kb — lib/ + skills/ shipped footprint", () => {
    const pkgRoot = path.join(__dirname, "..");
    const libBytes = dirBytes(path.join(pkgRoot, "lib"));
    const skillsBytes = dirBytes(path.join(pkgRoot, "skills"));
    const total = libBytes + skillsBytes;
    assert.ok(
      total < SIZE_LIMIT,
      "remote install footprint " +
        total +
        " bytes must be < " +
        SIZE_LIMIT +
        " (lib=" +
        libBytes +
        ", skills=" +
        skillsBytes +
        ")"
    );
  });
});

// ============================================================================
// Rotation overwrite (edge — re-init with a new bundle overwrites the pin)
// ============================================================================
describe("initRemote — rotation overwrite (edge)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_reinit_overwrites_pinned_fp_cleanly", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: BUNDLE_GOLDEN[0].wire, projectDir: dir });
    assert.strictEqual(
      readSettingsLocal(dir).unimatrix.remote.fingerprint,
      BUNDLE_GOLDEN[0].fields.fp
    );
    await initRemote({ bundle: BUNDLE_GOLDEN[1].wire, projectDir: dir });
    const sl = readSettingsLocal(dir);
    assert.strictEqual(sl.unimatrix.remote.fingerprint, BUNDLE_GOLDEN[1].fields.fp);
    assert.strictEqual(sl.unimatrix.remote.url, BUNDLE_GOLDEN[1].fields.base_url + "/v1");
  });
});
