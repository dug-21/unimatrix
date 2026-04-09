#!/usr/bin/env node
// C1: npm Package Structure — Test Suite
// Validates static package structure per test-plan/npm-package-structure.md

const fs = require("fs");
const path = require("path");

const PACKAGES_DIR = path.join(__dirname);
const ROOT_PKG_PATH = path.join(PACKAGES_DIR, "unimatrix", "package.json");
const PLATFORM_X64_PKG_PATH = path.join(
  PACKAGES_DIR,
  "unimatrix-linux-x64",
  "package.json"
);
const PLATFORM_ARM64_PKG_PATH = path.join(
  PACKAGES_DIR,
  "unimatrix-linux-arm64",
  "package.json"
);
const SKILLS_DIR = path.join(PACKAGES_DIR, "unimatrix", "skills");
const PLATFORM_X64_BIN_DIR = path.join(PACKAGES_DIR, "unimatrix-linux-x64", "bin");
const PLATFORM_ARM64_BIN_DIR = path.join(PACKAGES_DIR, "unimatrix-linux-arm64", "bin");

let passed = 0;
let failed = 0;

function assert(name, condition, detail) {
  if (condition) {
    console.log(`  PASS: ${name}`);
    passed++;
  } else {
    console.error(`  FAIL: ${name}${detail ? " — " + detail : ""}`);
    failed++;
  }
}

function deepEqual(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

// Load package.json files
const rootPkg = JSON.parse(fs.readFileSync(ROOT_PKG_PATH, "utf8"));
const platformPkg = JSON.parse(fs.readFileSync(PLATFORM_X64_PKG_PATH, "utf8"));
const platformArm64Pkg = JSON.parse(fs.readFileSync(PLATFORM_ARM64_PKG_PATH, "utf8"));

// --- Root package tests ---
console.log("Root package (@dug-21/unimatrix):");

assert(
  "test_root_package_scope_is_dug_21",
  rootPkg.name === "@dug-21/unimatrix",
  `got "${rootPkg.name}"`
);

assert(
  "test_root_package_version_is_0_6_2",
  rootPkg.version === "0.6.2",
  `got "${rootPkg.version}"`
);

assert(
  "test_root_package_has_bin_field",
  rootPkg.bin &&
    rootPkg.bin.unimatrix === "bin/unimatrix.js",
  `got ${JSON.stringify(rootPkg.bin)}`
);

assert(
  "test_root_package_has_optional_dependencies_x64",
  rootPkg.optionalDependencies &&
    rootPkg.optionalDependencies["@dug-21/unimatrix-linux-x64"] === "0.6.2",
  `got ${JSON.stringify(rootPkg.optionalDependencies)}`
);

assert(
  "test_root_package_has_optional_dependencies_arm64",
  rootPkg.optionalDependencies &&
    rootPkg.optionalDependencies["@dug-21/unimatrix-linux-arm64"] === "0.6.2",
  `got ${JSON.stringify(rootPkg.optionalDependencies)}`
);

assert(
  "test_root_package_has_postinstall",
  rootPkg.scripts &&
    rootPkg.scripts.postinstall === "node postinstall.js",
  `got ${JSON.stringify(rootPkg.scripts)}`
);

assert(
  "test_root_package_files_array",
  Array.isArray(rootPkg.files) &&
    rootPkg.files.includes("bin/") &&
    rootPkg.files.includes("lib/") &&
    rootPkg.files.includes("skills/") &&
    rootPkg.files.includes("postinstall.js"),
  `got ${JSON.stringify(rootPkg.files)}`
);

assert(
  "test_root_package_publish_config_restricted",
  rootPkg.publishConfig &&
    rootPkg.publishConfig.access === "public",
  `got ${JSON.stringify(rootPkg.publishConfig)}`
);

assert(
  "test_root_package_engines_node_18",
  rootPkg.engines &&
    rootPkg.engines.node === ">=18",
  `got ${JSON.stringify(rootPkg.engines)}`
);

assert(
  "test_root_package_repository_url",
  rootPkg.repository && rootPkg.repository.url === "https://github.com/dug-21/unimatrix",
  `got ${JSON.stringify(rootPkg.repository)}`
);

assert(
  "test_root_package_has_homepage",
  typeof rootPkg.homepage === "string" && rootPkg.homepage.length > 0,
  `got ${JSON.stringify(rootPkg.homepage)}`
);

assert(
  "test_root_package_readme_exists",
  fs.existsSync(path.join(PACKAGES_DIR, "unimatrix", "README.md")),
  `path: ${path.join(PACKAGES_DIR, "unimatrix", "README.md")}`
);

// --- Platform package tests ---
console.log("\nPlatform package (@dug-21/unimatrix-linux-x64):");

assert(
  "test_platform_package_os_field",
  deepEqual(platformPkg.os, ["linux"]),
  `got ${JSON.stringify(platformPkg.os)}`
);

assert(
  "test_platform_package_cpu_field",
  deepEqual(platformPkg.cpu, ["x64"]),
  `got ${JSON.stringify(platformPkg.cpu)}`
);

assert(
  "test_platform_package_version_matches_root",
  platformPkg.version === rootPkg.version,
  `platform=${platformPkg.version}, root=${rootPkg.version}`
);

assert(
  "test_platform_package_has_no_dependencies",
  !platformPkg.dependencies && !platformPkg.devDependencies,
  `deps=${JSON.stringify(platformPkg.dependencies)}, devDeps=${JSON.stringify(platformPkg.devDependencies)}`
);

assert(
  "test_platform_package_publish_config_restricted",
  platformPkg.publishConfig &&
    platformPkg.publishConfig.access === "public",
  `got ${JSON.stringify(platformPkg.publishConfig)}`
);

assert(
  "test_platform_package_files_array",
  Array.isArray(platformPkg.files) &&
    platformPkg.files.includes("bin/"),
  `got ${JSON.stringify(platformPkg.files)}`
);

// AC-12: optionalDependencies uses exact version (not range)
assert(
  "test_optional_deps_exact_version_not_range_x64",
  rootPkg.optionalDependencies &&
    /^\d+\.\d+\.\d+$/.test(rootPkg.optionalDependencies["@dug-21/unimatrix-linux-x64"]),
  `got "${rootPkg.optionalDependencies && rootPkg.optionalDependencies["@dug-21/unimatrix-linux-x64"]}"`
);

assert(
  "test_optional_deps_exact_version_not_range_arm64",
  rootPkg.optionalDependencies &&
    /^\d+\.\d+\.\d+$/.test(rootPkg.optionalDependencies["@dug-21/unimatrix-linux-arm64"]),
  `got "${rootPkg.optionalDependencies && rootPkg.optionalDependencies["@dug-21/unimatrix-linux-arm64"]}"`
);

// --- Skills directory tests ---
console.log("\nSkills directory:");

const EXPECTED_SKILLS = [
  "knowledge-lookup",
  "knowledge-search",
  "query-patterns",
  "record-outcome",
  "retro",
  "review-pr",
  "store-adr",
  "store-lesson",
  "store-pattern",
  "store-procedure",
  "uni-git",
  "unimatrix-init",
  "unimatrix-seed",
];

const skillEntries = fs
  .readdirSync(SKILLS_DIR)
  .filter((e) => fs.statSync(path.join(SKILLS_DIR, e)).isDirectory());

assert(
  "test_skills_directory_has_13_entries",
  skillEntries.length === 13,
  `got ${skillEntries.length}: ${skillEntries.join(", ")}`
);

let allSkillsHaveMd = true;
const missingMd = [];
for (const skill of EXPECTED_SKILLS) {
  const mdPath = path.join(SKILLS_DIR, skill, "SKILL.md");
  if (!fs.existsSync(mdPath)) {
    allSkillsHaveMd = false;
    missingMd.push(skill);
  }
}
assert(
  "test_each_skill_has_skill_md",
  allSkillsHaveMd,
  missingMd.length > 0 ? `missing SKILL.md in: ${missingMd.join(", ")}` : ""
);

// --- ARM64 platform package tests ---
console.log("\nPlatform package (@dug-21/unimatrix-linux-arm64):");

assert(
  "test_arm64_platform_package_os_field",
  deepEqual(platformArm64Pkg.os, ["linux"]),
  `got ${JSON.stringify(platformArm64Pkg.os)}`
);

assert(
  "test_arm64_platform_package_cpu_field",
  deepEqual(platformArm64Pkg.cpu, ["arm64"]),
  `got ${JSON.stringify(platformArm64Pkg.cpu)}`
);

assert(
  "test_arm64_platform_package_version_matches_root",
  platformArm64Pkg.version === rootPkg.version,
  `platform=${platformArm64Pkg.version}, root=${rootPkg.version}`
);

assert(
  "test_arm64_platform_package_has_no_dependencies",
  !platformArm64Pkg.dependencies && !platformArm64Pkg.devDependencies,
  `deps=${JSON.stringify(platformArm64Pkg.dependencies)}, devDeps=${JSON.stringify(platformArm64Pkg.devDependencies)}`
);

assert(
  "test_arm64_platform_package_publish_config_restricted",
  platformArm64Pkg.publishConfig &&
    platformArm64Pkg.publishConfig.access === "public",
  `got ${JSON.stringify(platformArm64Pkg.publishConfig)}`
);

assert(
  "test_arm64_platform_package_files_array",
  Array.isArray(platformArm64Pkg.files) &&
    platformArm64Pkg.files.includes("bin/"),
  `got ${JSON.stringify(platformArm64Pkg.files)}`
);

// --- Platform bin directories ---
console.log("\nPlatform binary directories:");

assert(
  "test_platform_x64_bin_dir_exists",
  fs.existsSync(PLATFORM_X64_BIN_DIR) &&
    fs.statSync(PLATFORM_X64_BIN_DIR).isDirectory(),
  `path: ${PLATFORM_X64_BIN_DIR}`
);

assert(
  "test_platform_arm64_bin_dir_exists",
  fs.existsSync(PLATFORM_ARM64_BIN_DIR) &&
    fs.statSync(PLATFORM_ARM64_BIN_DIR).isDirectory(),
  `path: ${PLATFORM_ARM64_BIN_DIR}`
);

// --- Summary ---
console.log(`\n--- Results: ${passed} passed, ${failed} failed ---`);
process.exit(failed > 0 ? 1 : 0);
