#!/usr/bin/env node

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const repoRoot = path.resolve(__dirname, "..", "..");
const {
  allAssetNames,
  allReleaseAssetNames,
  BUNDLE_ASSET_NAMES,
} = require(path.join(repoRoot, "npm", "codewhale", "scripts", "artifacts"));

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function valuesForKey(source, key) {
  const expression = new RegExp(`^\\s+${key}:\\s+([^#\\s]+)\\s*$`, "gm");
  return [...source.matchAll(expression)].map((match) => match[1]);
}

const ci = read(".github/workflows/ci.yml");
const candidate = read(".github/workflows/release-candidate.yml");
const artifacts = read(".github/workflows/release-artifacts.yml");
const release = read(".github/workflows/release.yml");
const bundles = read("scripts/release/create-release-bundles.sh");
const runbook = read("docs/RELEASE_RUNBOOK.md");

assert.match(ci, /^  workflow_dispatch:\n    inputs:\n      expected_sha:/m);
const manualForceBlock = ci.match(
  /if \[\[ "\$\{EVENT_NAME\}" == "workflow_dispatch" \]\]; then([\s\S]*?)\n\s+if \[\[ "\$\{EVENT_NAME\}" == "schedule" \]\]; then/,
);
assert.ok(manualForceBlock, "CI must have a dedicated manual-dispatch force-full branch");
for (const output of ["heavy", "workflow", "mobile", "actions"]) {
  assert.match(manualForceBlock[1], new RegExp(`echo "${output}=true"`));
}
assert.match(manualForceBlock[1], /#EXPECTED_SHA.*-ne 40/s);
assert.match(manualForceBlock[1], /actual.*EXPECTED_SHA/s);

assert.match(candidate, /^  workflow_dispatch:\n    inputs:\n      expected_sha:/m);
assert.doesNotMatch(candidate, /^  (push|pull_request|schedule):/m);
assert.match(candidate, /uses: \.\/\.github\/workflows\/release-artifacts\.yml/);
assert.match(candidate, /source_sha: \$\{\{ needs\.resolve\.outputs\.sha \}\}/);

for (const [label, workflow] of [
  ["release candidate", candidate],
  ["shared artifact", artifacts],
]) {
  for (const forbidden of [
    /contents:\s*write/,
    /packages:\s*write/,
    /softprops\/action-gh-release/,
    /docker\/login-action/,
    /docker\/build-push-action/,
    /\bgh release\b/,
    /\bnpm publish\b/,
    /\bcargo publish\b/,
    /\bgit push\b/,
  ]) {
    assert.doesNotMatch(workflow, forbidden, `${label} workflow contains publication capability`);
  }
}

assert.match(artifacts, /^  workflow_call:/m);
assert.match(artifacts, /^permissions:\n  contents: read$/m);
const expectedTargets = [
  "x86_64-unknown-linux-musl",
  "aarch64-unknown-linux-gnu",
  "aarch64-linux-android",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-pc-windows-msvc",
  "aarch64-pc-windows-msvc",
].sort();
assert.deepEqual([...new Set(valuesForKey(artifacts, "target"))].sort(), expectedTargets);

const builtAssetNames = [
  ...valuesForKey(artifacts, "cli_artifact"),
  ...valuesForKey(artifacts, "shim_artifact"),
  ...valuesForKey(artifacts, "tui_artifact"),
];
assert.equal(builtAssetNames.length, 21);
assert.deepEqual(
  [...new Set(builtAssetNames)].sort(),
  allAssetNames().filter((name) => name !== "codewhale.bat").sort(),
);
const bundleInvocations = [...bundles.matchAll(
  /^bundle (\S+) \\\n\s+\S+ \S+ \S+ (tar\.gz|zip) (""|portable)$/gm,
)].map((match) => {
  const variant = match[3] === "portable" ? "-portable" : "";
  return `codewhale-${match[1]}${variant}.${match[2]}`;
});
assert.deepEqual(bundleInvocations.sort(), [...BUNDLE_ASSET_NAMES].sort());
assert.match(artifacts, /aarch64-pc-windows-msvc/);
assert.match(artifacts, /aarch64-linux-android/);
assert.match(artifacts, /codew-windows-arm64\.exe/);
assert.match(artifacts, /CodeWhaleSetup\.exe/);
assert.match(artifacts, /assemble-release-assets\.js --verify release-assets/);
assert.match(artifacts, /CODEWHALE_SMOKE_ASSETS_DIR/);

assert.equal(allReleaseAssetNames().length, 34);
assert.match(release, /^  artifacts:\n/m);
assert.match(release, /uses: \.\/\.github\/workflows\/release-artifacts\.yml/);
assert.doesNotMatch(release, /^  (build|bundle|windows-installer):/m);
assert.match(release, /name: codewhale-release-assets\n\s+path: artifacts/);
assert.match(release, /files: artifacts\/\*/);
assert.equal(
  (release.match(/ensure-release-assets-absent\.js/g) || []).length,
  2,
  "public release must refuse existing assets before work and immediately before upload",
);
assert.match(release, /overwrite_files:\s*false/);
assert.match(release, /fail_on_unmatched_files:\s*true/);

assert.match(runbook, /release[- ]candidate/i);
assert.match(runbook, /expected_sha/);
assert.match(runbook, /34/);
assert.match(runbook, /does not create a tag/i);
assert.match(runbook, /explicit.*approval/i);

console.log("Release workflow contracts OK: exact-head full CI and 7-target/34-asset non-publishing candidate.");
