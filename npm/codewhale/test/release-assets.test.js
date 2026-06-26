const assert = require("node:assert/strict");
const test = require("node:test");

const pkg = require("../package.json");
const {
  assertPackageVersionMatchesBinaryVersion,
  assertReleaseAssetsFresh,
  parseChecksumManifest,
} = require("../scripts/verify-release-assets");

test("parseChecksumManifest accepts GNU and BSD filename forms", () => {
  const manifest = parseChecksumManifest(
    [
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  codewhale-linux-x64",
      "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *codewhale-windows-x64.exe",
    ].join("\n"),
  );

  assert.equal(manifest.get("codewhale-linux-x64"), "a".repeat(64));
  assert.equal(manifest.get("codewhale-windows-x64.exe"), "b".repeat(64));
});

test("parseChecksumManifest rejects malformed checksum rows", () => {
  assert.throws(
    () => parseChecksumManifest("not-a-sha  codewhale-linux-x64"),
    /Invalid checksum manifest line/,
  );
});

test("assertReleaseAssetsFresh rejects missing npm-facing assets", () => {
  assert.throws(
    () =>
      assertReleaseAssetsFresh(
        { assets: [{ name: "codewhale-linux-x64", state: "uploaded", updated_at: "2026-06-26T00:10:00Z" }] },
        ["codewhale-linux-x64", "codewhale-artifacts-sha256.txt"],
        { database_id: 123, created_at: "2026-06-26T00:00:00Z" },
      ),
    /missing required npm-facing asset/,
  );
});

test("assertReleaseAssetsFresh rejects assets older than the release workflow run", () => {
  assert.throws(
    () =>
      assertReleaseAssetsFresh(
        { assets: [{ name: "codewhale-linux-x64", state: "uploaded", updated_at: "2026-06-25T23:59:59Z" }] },
        ["codewhale-linux-x64"],
        { database_id: 123, created_at: "2026-06-26T00:00:00Z" },
      ),
    /asset set is stale/,
  );
});

test("assertReleaseAssetsFresh accepts assets updated by the release workflow run", () => {
  assert.doesNotThrow(() =>
    assertReleaseAssetsFresh(
      { assets: [{ name: "codewhale-linux-x64", state: "uploaded", updated_at: "2026-06-26T00:10:00Z" }] },
      ["codewhale-linux-x64"],
      { database_id: 123, created_at: "2026-06-26T00:00:00Z" },
    ),
  );
});

test("assertPackageVersionMatchesBinaryVersion allows packaging-only releases only with an explicit override", () => {
  assert.doesNotThrow(() => assertPackageVersionMatchesBinaryVersion(pkg.version));
  assert.throws(
    () => assertPackageVersionMatchesBinaryVersion("0.0.0-packaging-test"),
    /does not match codewhaleBinaryVersion/,
  );

  const previous = process.env.CODEWHALE_ALLOW_NPM_BINARY_MISMATCH;
  process.env.CODEWHALE_ALLOW_NPM_BINARY_MISMATCH = "1";
  try {
    assert.doesNotThrow(() => assertPackageVersionMatchesBinaryVersion("0.0.0-packaging-test"));
  } finally {
    if (previous === undefined) {
      delete process.env.CODEWHALE_ALLOW_NPM_BINARY_MISMATCH;
    } else {
      process.env.CODEWHALE_ALLOW_NPM_BINARY_MISMATCH = previous;
    }
  }
});
