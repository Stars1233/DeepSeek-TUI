#!/usr/bin/env bash
# Bump every version-bearing file for a release in one shot.
#
# Usage: ./scripts/release/prepare-release.sh <new-version>
#
# Touches: Cargo.toml (workspace version), crates/*/Cargo.toml (internal
# codewhale-* dependency pins), npm/codewhale/package.json (version +
# codewhaleBinaryVersion), README*.md install-tag examples when present,
# Cargo.lock, crates/tui/CHANGELOG.md (via sync-changelog.sh), and
# web/lib/facts.generated.ts (via derive-facts.mjs).
#
# It does NOT write the CHANGELOG entry — add the `## [X.Y.Z] - YYYY-MM-DD`
# section first (see docs/RELEASE_CHECKLIST.md), then run this script, then
# let check-versions.sh (run at the end here) confirm everything agrees.
set -euo pipefail

new="${1:?usage: $0 <new-version>}"
if ! [[ "${new}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: '${new}' is not a plain X.Y.Z version" >&2
  exit 1
fi

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo}"

old="$(grep -E '^version = "' Cargo.toml | head -n1 | sed -E 's/^version = "([^"]+)".*/\1/')"
if ! grep -q "^## \[${new}\]" CHANGELOG.md; then
  echo "warning: CHANGELOG.md has no '## [${new}]' entry yet — add it before tagging" >&2
fi

if [[ "${old}" != "${new}" ]]; then
  echo "Bumping ${old} -> ${new}"

  OLD_VERSION="${old}" NEW_VERSION="${new}" python3 - <<'PY'
import os, pathlib, re, sys

old, new = os.environ["OLD_VERSION"], os.environ["NEW_VERSION"]
old_re = re.escape(old)
readmes = [
    "README.md",
    "README.zh-CN.md",
    "README.ja-JP.md",
    "README.vi.md",
    "README.ko-KR.md",
]

def bump(path, pattern, repl, minimum):
    p = pathlib.Path(path)
    text = p.read_text()
    out, n = re.subn(pattern, repl, text, flags=re.MULTILINE)
    if n < minimum:
        sys.exit(f"error: expected >= {minimum} replacement(s) in {path}, made {n}")
    p.write_text(out)
    print(f"  {path}: {n} replacement(s)")

# Validate every versioned README install tag before writing any file. A README
# with no pinned tag is valid; if a tag exists, it must match the workspace so
# the release helper cannot silently preserve stale public install instructions.
release_tag_pattern = re.compile(r"--tag v([0-9]+\.[0-9]+\.[0-9]+)\b")
for readme in readmes:
    versions = sorted(set(release_tag_pattern.findall(pathlib.Path(readme).read_text())))
    stale = [version for version in versions if version != old]
    if stale:
        found = ", ".join(stale)
        sys.exit(
            f"error: {readme} has release tag version(s) {found}; expected {old}"
        )

# 1) Workspace version.
bump("Cargo.toml", rf'^version = "{old_re}"$', f'version = "{new}"', 1)

# 2) Internal codewhale-* dependency pins in every crate manifest.
total = 0
for manifest in sorted(pathlib.Path("crates").glob("*/Cargo.toml")):
    text = manifest.read_text()
    out, n = re.subn(
        rf'(codewhale-[a-z0-9-]+\s*=\s*\{{[^}}]*version = "){old_re}(")',
        rf"\g<1>{new}\g<2>",
        text,
    )
    if n:
        manifest.write_text(out)
        print(f"  {manifest}: {n} pin(s)")
        total += n
if total == 0:
    sys.exit("error: no internal dependency pins were bumped — wrong old version?")

# 3) npm wrapper.
bump(
    "npm/codewhale/package.json",
    rf'("(?:version|codewhaleBinaryVersion)": "){old_re}(")',
    rf"\g<1>{new}\g<2>",
    2,
)

# 4) README install-tag examples (all translations, when present).
for readme in readmes:
    p = pathlib.Path(readme)
    text = p.read_text()
    out, n = re.subn(rf"--tag v{old_re}\b", f"--tag v{new}", text)
    if n:
        p.write_text(out)
        print(f"  {readme}: {n} install-tag replacement(s)")
    else:
        print(f"  {readme}: no versioned install-tag example; skipped")

# 5) Public install/version snippets in README*.md and docs/INSTALL.md.
#    These are the user-facing "verify your install" lines and the npm wrapper
#    publish pointer. They drifted on a prior lane while check-versions still
#    passed (#3767, #3552), so bump and (in check-versions.sh) guard them.
version_doc_files = [
    "README.md",
    "README.zh-CN.md",
    "README.ja-JP.md",
    "README.vi.md",
    "README.ko-KR.md",
    "docs/INSTALL.md",
]
version_comment_hits = 0
for doc in version_doc_files:
    p = pathlib.Path(doc)
    text = p.read_text()
    out, n = re.subn(
        rf"(codewhale --version\s+#\s*){old_re}\b", rf"\g<1>{new}", text
    )
    if n:
        p.write_text(out)
        print(f"  {doc}: {n} version-comment replacement(s)")
        version_comment_hits += n
if version_comment_hits == 0:
    sys.exit("error: no 'codewhale --version # X' snippets were bumped — wrong old version?")

# docs/INSTALL.md npm-wrapper publish pointer ("published at vX.Y.Z").
bump(
    "docs/INSTALL.md",
    rf"(wrapper is published at\s+)v{old_re}\b",
    rf"\g<1>v{new}",
    1,
)
PY

  echo "Refreshing Cargo.lock..."
  cargo update --workspace --offline >/dev/null
else
  echo "Workspace is already at ${new}; refreshing generated release state and rerunning gates."
fi

echo "Regenerating crates/tui/CHANGELOG.md slice..."
./scripts/sync-changelog.sh

echo "Regenerating web/lib/facts.generated.ts..."
node web/scripts/derive-facts.mjs

echo "Validating..."
./scripts/release/check-versions.sh
./scripts/release/check-ohos-deps.sh
echo "Done. Review 'git diff', commit, and follow docs/RELEASE_CHECKLIST.md."
