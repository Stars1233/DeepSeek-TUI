#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

cat >"${tmp_dir}/CHANGELOG.md" <<'EOF'
## [Unreleased]

## [1.2.3] - 2026-07-14

### Fixed

- A release fix.

### Contributors

- [@example](https://github.com/example) — report and implementation.

## [1.2.2] - 2026-07-01
EOF

body="$("${repo_root}/scripts/release/generate-release-body.sh" v1.2.3 "${tmp_dir}/CHANGELOG.md")"

grep -Fq -- "- A release fix." <<<"${body}"
grep -Fq -- "## Contributors" <<<"${body}"
grep -Fq -- "[@example](https://github.com/example)" <<<"${body}"
grep -Fq -- 'codewhale-home:/home/codewhale/.codewhale' <<<"${body}"
grep -Fq -- 'codewhale-android-arm64.tar.gz' <<<"${body}"
grep -Fq -- 'codewhale-windows-arm64.zip' <<<"${body}"
grep -Fq -- 'The image ships the `codewhale` dispatcher, `codew` shim, and `codewhale-tui` runtime.' <<<"${body}"
if grep -Fq -- "### Contributors" <<<"${body}"; then
  echo "nested contributor heading leaked into generated release body" >&2
  exit 1
fi

echo "generate-release-body tests passed"
