"use strict";

// credstore.test.js — vnc-039 C1: out-of-tree per-projectHash credential store
// (~/.unimatrix/<projectHash>/remote.json, mode 0600). Covers path derivation
// (R-07), mode-0600 idempotent merge-write + dry-run (R-12, AC-08b), happy-path
// read + one-key round-trip (R-07, AC-08c), the read-error posture matrix
// (R-13), the both-consumers-one-schema keystone (AC-08c), per-project
// separation (AC-08b), and the no-token-in-message/action contract (R-09).
//
// Cumulative infra: reuses computeProjectHash from the config module and the
// temp-HOME override pattern from the config suite. No isolated scaffolding.
// Scope B independence (R-08): imports ONLY fs/os/path + credstore + config's
// computeProjectHash — never mcp-bridge.js, never a network module.

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const credstore = require("../../lib/hook-client/credstore.js");
const { computeProjectHash } = require("../../lib/hook-client/config.js");

// --- harness: override os.homedir() to a per-test temp root --------------------

let tmpHome;
let origHomedir;

beforeEach(function () {
  tmpHome = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-credstore-"));
  origHomedir = os.homedir;
  os.homedir = function () {
    return tmpHome;
  };
});

afterEach(function () {
  os.homedir = origHomedir;
  try {
    fs.rmSync(tmpHome, { recursive: true, force: true });
  } catch (_err) {
    // best-effort cleanup
  }
});

// A 16-hex projectHash derived through the SAME oracle both consumers use.
function hashFor(root) {
  return computeProjectHash(root);
}

const HASH_A = hashFor("/some/project/alpha");
const HASH_B = hashFor("/some/project/beta");

const TOKEN = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const FP = "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

function cred(overrides) {
  return Object.assign(
    {
      mcp_url: "https://host.example/v1/myslug",
      observe_url: "https://host.example/v1/myslug/observe",
      token: TOKEN,
      fingerprint: FP,
    },
    overrides || {}
  );
}

function expectedPath(hash) {
  return path.join(tmpHome, ".unimatrix", hash, "remote.json");
}

// --- path derivation (R-07) ----------------------------------------------------

describe("credstore.pathFor", function () {
  it("test_pathFor_validHash_returnsHomeUnimatrixHashRemoteJson", function () {
    assert.strictEqual(credstore.pathFor(HASH_A), expectedPath(HASH_A));
  });

  it("test_pathFor_noHomedir_returnsNull_empty", function () {
    os.homedir = function () {
      return "";
    };
    assert.strictEqual(credstore.pathFor(HASH_A), null);
  });

  it("test_pathFor_noHomedir_returnsNull_throws", function () {
    os.homedir = function () {
      throw new Error("no home");
    };
    assert.strictEqual(credstore.pathFor(HASH_A), null);
  });

  it("test_pathFor_colocatedWithSocket", function () {
    // The store dir equals the per-project root that holds unimatrix.sock
    // (ADR-003 colocation): one ~/.unimatrix/<hash>/ root.
    const storeDir = path.dirname(credstore.pathFor(HASH_A));
    const expectedDir = path.join(tmpHome, ".unimatrix", HASH_A);
    assert.strictEqual(storeDir, expectedDir);
  });
});

// --- write: mode 0600, idempotent merge, dry-run (R-12, AC-08b) ---------------

describe("credstore.write", function () {
  it("test_write_creates0600File", function () {
    credstore.write(HASH_A, cred());
    const mode = fs.statSync(expectedPath(HASH_A)).mode & 0o777;
    assert.strictEqual(mode, 0o600);
  });

  it("test_write_persistsCanonicalSchema", function () {
    credstore.write(HASH_A, cred());
    const obj = JSON.parse(fs.readFileSync(expectedPath(HASH_A), "utf8"));
    assert.strictEqual(obj.schema_version, 1);
    assert.strictEqual(obj.mcp_url, "https://host.example/v1/myslug");
    assert.strictEqual(obj.observe_url, "https://host.example/v1/myslug/observe");
    assert.strictEqual(obj.token, TOKEN);
    assert.strictEqual(obj.fingerprint, FP);
    // timeouts omitted when not provided.
    assert.strictEqual(Object.prototype.hasOwnProperty.call(obj, "timeouts"), false);
  });

  it("test_write_persistsTimeoutsWhenProvided", function () {
    const t = { connect_ms: 750, sync_ms: 2000, fnf_ms: 3000 };
    credstore.write(HASH_A, cred({ timeouts: t }));
    const obj = JSON.parse(fs.readFileSync(expectedPath(HASH_A), "utf8"));
    assert.deepStrictEqual(obj.timeouts, t);
  });

  it("test_write_idempotent_sameCredNoDuplicateNoGrowth", function () {
    credstore.write(HASH_A, cred());
    const first = fs.readFileSync(expectedPath(HASH_A), "utf8");
    credstore.write(HASH_A, cred());
    const second = fs.readFileSync(expectedPath(HASH_A), "utf8");
    assert.strictEqual(first, second);
    assert.strictEqual(fs.statSync(expectedPath(HASH_A)).mode & 0o777, 0o600);
  });

  it("test_write_update_overwritesEntryForSameHash", function () {
    credstore.write(HASH_A, cred({ token: "aaaa" }));
    credstore.write(HASH_A, cred({ token: "bbbb" }));
    const obj = JSON.parse(fs.readFileSync(expectedPath(HASH_A), "utf8"));
    assert.strictEqual(obj.token, "bbbb");
  });

  it("test_write_mergePreservesUnknownFutureField", function () {
    // Seed a readable existing entry with an unknown future field.
    const dir = path.dirname(expectedPath(HASH_A));
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(
      expectedPath(HASH_A),
      JSON.stringify({
        schema_version: 1,
        mcp_url: "old",
        observe_url: "old",
        token: "old",
        fingerprint: null,
        future_field: "survives",
      }) + "\n"
    );
    credstore.write(HASH_A, cred());
    const obj = JSON.parse(fs.readFileSync(expectedPath(HASH_A), "utf8"));
    assert.strictEqual(obj.future_field, "survives");
    assert.strictEqual(obj.token, TOKEN); // canonical field still overwritten
  });

  it("test_write_overExistingMalformed_replacesWithValid", function () {
    const dir = path.dirname(expectedPath(HASH_A));
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(expectedPath(HASH_A), "{ not json");
    credstore.write(HASH_A, cred());
    const obj = JSON.parse(fs.readFileSync(expectedPath(HASH_A), "utf8"));
    assert.strictEqual(obj.schema_version, 1);
    assert.strictEqual(obj.token, TOKEN);
  });

  it("test_write_dryRun_noFileWritten", function () {
    const actions = credstore.write(HASH_A, cred(), { dryRun: true });
    assert.ok(Array.isArray(actions) && actions.length >= 1);
    assert.strictEqual(fs.existsSync(expectedPath(HASH_A)), false);
    // No dir created either.
    assert.strictEqual(
      fs.existsSync(path.dirname(expectedPath(HASH_A))),
      false
    );
  });

  it("test_write_returnsActionStrings_tokenFree", function () {
    const actions = credstore.write(HASH_A, cred());
    assert.ok(actions.length >= 1);
    for (const a of actions) {
      assert.strictEqual(a.includes(TOKEN), false);
      assert.strictEqual(a.includes(FP), false);
    }
  });

  it("test_write_dryRunActionStrings_tokenFree", function () {
    const actions = credstore.write(HASH_A, cred(), { dryRun: true });
    for (const a of actions) {
      assert.strictEqual(a.includes(TOKEN), false);
      assert.strictEqual(a.includes(FP), false);
    }
  });

  it("test_write_reassertsModeOnExistingFile", function () {
    const dir = path.dirname(expectedPath(HASH_A));
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(expectedPath(HASH_A), "{}\n", { mode: 0o644 });
    fs.chmodSync(expectedPath(HASH_A), 0o644);
    credstore.write(HASH_A, cred());
    assert.strictEqual(fs.statSync(expectedPath(HASH_A)).mode & 0o777, 0o600);
  });

  it("test_write_fingerprintNull_persistsNullNotOmitted", function () {
    credstore.write(HASH_A, cred({ fingerprint: null }));
    const obj = JSON.parse(fs.readFileSync(expectedPath(HASH_A), "utf8"));
    assert.strictEqual(Object.prototype.hasOwnProperty.call(obj, "fingerprint"), true);
    assert.strictEqual(obj.fingerprint, null);
  });

  it("test_write_fingerprintUndefined_persistsNull", function () {
    const c = cred();
    delete c.fingerprint;
    credstore.write(HASH_A, c);
    const obj = JSON.parse(fs.readFileSync(expectedPath(HASH_A), "utf8"));
    assert.strictEqual(obj.fingerprint, null);
  });

  it("test_write_noHomedir_throws", function () {
    os.homedir = function () {
      return "";
    };
    assert.throws(function () {
      credstore.write(HASH_A, cred());
    });
  });
});

// --- per-project separation (AC-08b, R-07) ------------------------------------

describe("credstore per-project separation", function () {
  it("test_write_twoProjects_twoDistinctHashDirs", function () {
    credstore.write(HASH_A, cred({ token: "alpha" }));
    credstore.write(HASH_B, cred({ token: "beta" }));
    assert.notStrictEqual(HASH_A, HASH_B);
    assert.strictEqual(fs.existsSync(expectedPath(HASH_A)), true);
    assert.strictEqual(fs.existsSync(expectedPath(HASH_B)), true);
    assert.strictEqual(credstore.read(HASH_A).token, "alpha");
    assert.strictEqual(credstore.read(HASH_B).token, "beta");
  });

  it("test_write_reinitProjectA_doesNotTouchProjectB", function () {
    credstore.write(HASH_A, cred({ token: "alpha" }));
    credstore.write(HASH_B, cred({ token: "beta" }));
    const bBefore = fs.readFileSync(expectedPath(HASH_B), "utf8");
    credstore.write(HASH_A, cred({ token: "alpha2" }));
    const bAfter = fs.readFileSync(expectedPath(HASH_B), "utf8");
    assert.strictEqual(bBefore, bAfter);
    assert.strictEqual(credstore.read(HASH_A).token, "alpha2");
  });
});

// --- read: happy path + one-key round-trip (R-07, AC-08c) ---------------------

describe("credstore.read happy path", function () {
  it("test_read_afterWrite_roundTripsCanonicalSchema", function () {
    credstore.write(HASH_A, cred());
    const obj = credstore.read(HASH_A);
    assert.strictEqual(obj.schema_version, 1);
    assert.strictEqual(obj.mcp_url, "https://host.example/v1/myslug");
    assert.strictEqual(obj.observe_url, "https://host.example/v1/myslug/observe");
    assert.strictEqual(obj.token, TOKEN);
    assert.strictEqual(obj.fingerprint, FP);
  });

  it("test_read_returnsTimeoutsWhenPresent_absentWhenOmitted", function () {
    const t = { connect_ms: 750, sync_ms: 2000, fnf_ms: 3000 };
    credstore.write(HASH_A, cred({ timeouts: t }));
    assert.deepStrictEqual(credstore.read(HASH_A).timeouts, t);

    credstore.write(HASH_B, cred());
    assert.strictEqual(
      Object.prototype.hasOwnProperty.call(credstore.read(HASH_B), "timeouts"),
      false
    );
  });

  it("test_read_wrongHash_returnsNull", function () {
    credstore.write(HASH_A, cred());
    assert.strictEqual(credstore.read(HASH_B), null);
  });
});

// --- read-error posture matrix (R-13) -----------------------------------------

function seedRaw(hash, raw) {
  const p = credstore.pathFor(hash);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, raw);
  return p;
}

describe("credstore.read error posture (R-13)", function () {
  it("test_read_enoent_returnsNull", function () {
    assert.strictEqual(credstore.read(HASH_A), null);
  });

  it("test_read_noHomedir_returnsNull", function () {
    os.homedir = function () {
      return "";
    };
    assert.strictEqual(credstore.read(HASH_A), null);
  });

  it("test_read_malformedJson_throws", function () {
    seedRaw(HASH_A, "{ this is not json");
    assert.throws(function () {
      credstore.read(HASH_A);
    });
  });

  it("test_read_nonObjectRoot_throws", function () {
    seedRaw(HASH_A, "[1,2,3]");
    assert.throws(function () {
      credstore.read(HASH_A);
    });
  });

  it("test_read_unknownSchemaVersion_throws", function () {
    seedRaw(HASH_A, JSON.stringify({ schema_version: 999, token: "x" }));
    assert.throws(function () {
      credstore.read(HASH_A);
    });
  });

  it("test_read_missingSchemaVersion_throws", function () {
    seedRaw(HASH_A, JSON.stringify({ mcp_url: "x", token: "y" }));
    assert.throws(function () {
      credstore.read(HASH_A);
    });
  });

  it("test_read_throwMessage_tokenFree", function () {
    // Malformed file embedding a token-shaped string in a bad position.
    seedRaw(HASH_A, '{ "token": "' + TOKEN + '" not-json');
    let caught = null;
    try {
      credstore.read(HASH_A);
    } catch (e) {
      caught = e;
    }
    assert.ok(caught, "expected a throw on malformed JSON");
    assert.strictEqual(caught.message.includes(TOKEN), false);
  });

  it("test_read_unknownVersionMessage_tokenFree", function () {
    seedRaw(
      HASH_A,
      JSON.stringify({ schema_version: 7, token: TOKEN, fingerprint: FP })
    );
    let caught = null;
    try {
      credstore.read(HASH_A);
    } catch (e) {
      caught = e;
    }
    assert.ok(caught);
    assert.strictEqual(caught.message.includes(TOKEN), false);
    assert.strictEqual(caught.message.includes(FP), false);
  });
});

// --- both-consumers-one-schema keystone (R-07, AC-08c) ------------------------

describe("credstore single-schema contract", function () {
  it("test_read_oneFile_servesBridgeFieldsAndHookFields", function () {
    const t = { connect_ms: 750, sync_ms: 2000, fnf_ms: 3000 };
    credstore.write(HASH_A, cred({ timeouts: t }));
    const obj = credstore.read(HASH_A);
    // Bridge fields.
    assert.ok(nonEmpty(obj.mcp_url));
    assert.ok(nonEmpty(obj.token));
    assert.ok(nonEmpty(obj.fingerprint));
    // Hook-client fields.
    assert.ok(nonEmpty(obj.observe_url));
    assert.ok(nonEmpty(obj.token));
    assert.ok(nonEmpty(obj.fingerprint));
    assert.deepStrictEqual(obj.timeouts, t);
  });
});

function nonEmpty(v) {
  return typeof v === "string" && v.length > 0;
}

// --- Scope B independence (R-08): structural check ----------------------------

describe("credstore Scope B independence", function () {
  it("test_credstore_doesNotRequireBridgeOrNetwork", function () {
    // The implementation pulls in no network/bridge modules — its require graph
    // is fs/os/path only. Assert the module source references no such imports.
    const src = fs.readFileSync(
      path.join(__dirname, "../../lib/hook-client/credstore.js"),
      "utf8"
    );
    assert.strictEqual(src.includes("mcp-bridge"), false);
    assert.strictEqual(/require\(["']https?["']\)/.test(src), false);
    assert.strictEqual(/require\(["']net["']\)/.test(src), false);
    assert.strictEqual(/require\(["']tls["']\)/.test(src), false);
  });
});
