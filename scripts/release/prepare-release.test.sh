#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

make_fixture() {
  local root="$1"
  mkdir -p \
    "${root}/bin" \
    "${root}/crates/example" \
    "${root}/docs" \
    "${root}/npm/codewhale" \
    "${root}/scripts/release" \
    "${root}/web/scripts"

  cp "${repo_root}/scripts/release/prepare-release.sh" \
    "${root}/scripts/release/prepare-release.sh"

  cat >"${root}/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/example"]

[workspace.package]
version = "0.8.68"
EOF

  cat >"${root}/crates/example/Cargo.toml" <<'EOF'
[package]
name = "codewhale-example"
version.workspace = true

[dependencies]
codewhale-core = { path = "../core", version = "0.8.68" }
EOF

  cat >"${root}/npm/codewhale/package.json" <<'EOF'
{
  "name": "codewhale",
  "version": "0.8.68",
  "codewhaleBinaryVersion": "0.8.68"
}
EOF

  cat >"${root}/CHANGELOG.md" <<'EOF'
## [Unreleased]

## [0.9.0] - 2026-07-15

### Changed

- Test release.
EOF

  cat >"${root}/docs/INSTALL.md" <<'EOF'
The npm wrapper is published at v0.8.68.

codewhale --version   # 0.8.68
EOF

  for readme in README.md README.zh-CN.md README.ja-JP.md README.vi.md README.ko-KR.md; do
    printf 'Install Codewhale from the package manager.\n' >"${root}/${readme}"
  done

  cat >"${root}/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: >"${PREPARE_RELEASE_TEST_MARKERS}/cargo"
EOF

  cat >"${root}/scripts/sync-changelog.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: >"${PREPARE_RELEASE_TEST_MARKERS}/sync-changelog"
EOF

  cat >"${root}/scripts/release/check-versions.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: >"${PREPARE_RELEASE_TEST_MARKERS}/check-versions"
EOF

  cat >"${root}/scripts/release/check-ohos-deps.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: >"${PREPARE_RELEASE_TEST_MARKERS}/check-ohos-deps"
EOF

  cat >"${root}/web/scripts/derive-facts.mjs" <<'EOF'
import { writeFileSync } from "node:fs";

writeFileSync(`${process.env.PREPARE_RELEASE_TEST_MARKERS}/derive-facts`, "");
EOF

  chmod +x \
    "${root}/bin/cargo" \
    "${root}/scripts/release/prepare-release.sh" \
    "${root}/scripts/release/check-ohos-deps.sh" \
    "${root}/scripts/release/check-versions.sh" \
    "${root}/scripts/sync-changelog.sh"
}

success_root="${tmp_dir}/success"
success_markers="${tmp_dir}/success-markers"
make_fixture "${success_root}"
mkdir -p "${success_markers}"
printf 'Install from a release tag: --tag v0.8.68\n' >"${success_root}/README.md"

PREPARE_RELEASE_TEST_MARKERS="${success_markers}" \
  PATH="${success_root}/bin:${PATH}" \
  "${success_root}/scripts/release/prepare-release.sh" 0.9.0 >/dev/null

grep -Fq 'version = "0.9.0"' "${success_root}/Cargo.toml"
grep -Fq 'version = "0.9.0"' "${success_root}/crates/example/Cargo.toml"
grep -Fq '"version": "0.9.0"' "${success_root}/npm/codewhale/package.json"
grep -Fq '"codewhaleBinaryVersion": "0.9.0"' \
  "${success_root}/npm/codewhale/package.json"
grep -Fq -- '--tag v0.9.0' "${success_root}/README.md"
if grep -R -E -- '--tag v[0-9]+\.[0-9]+\.[0-9]+' \
  "${success_root}/README.zh-CN.md" \
  "${success_root}/README.ja-JP.md" \
  "${success_root}/README.vi.md" \
  "${success_root}/README.ko-KR.md"; then
  echo "tag-free localized README unexpectedly gained a release tag" >&2
  exit 1
fi
for marker in cargo sync-changelog derive-facts check-versions check-ohos-deps; do
  [[ -f "${success_markers}/${marker}" ]] || {
    echo "prepare-release did not reach ${marker}" >&2
    exit 1
  }
done

same_root="${tmp_dir}/same"
same_markers="${tmp_dir}/same-markers"
same_log="${tmp_dir}/same.log"
make_fixture "${same_root}"
mkdir -p "${same_markers}"
cat >"${same_root}/CHANGELOG.md" <<'EOF'
## [Unreleased]

## [0.8.68] - 2026-07-18

### Changed

- Test already-prepared release.
EOF

PREPARE_RELEASE_TEST_MARKERS="${same_markers}" \
  PATH="${same_root}/bin:${PATH}" \
  "${same_root}/scripts/release/prepare-release.sh" 0.8.68 \
  >"${same_log}"

grep -Fq \
  'Workspace is already at 0.8.68; refreshing generated release state and rerunning gates.' \
  "${same_log}"
for marker in sync-changelog derive-facts check-versions check-ohos-deps; do
  [[ -f "${same_markers}/${marker}" ]] || {
    echo "same-version prepare-release did not reach ${marker}" >&2
    exit 1
  }
done
if [[ -f "${same_markers}/cargo" ]]; then
  echo "same-version prepare-release unexpectedly mutated Cargo.lock" >&2
  exit 1
fi
grep -Fq 'version = "0.8.68"' "${same_root}/Cargo.toml"
grep -Fq 'version = "0.8.68"' "${same_root}/crates/example/Cargo.toml"
grep -Fq '"version": "0.8.68"' "${same_root}/npm/codewhale/package.json"

stale_root="${tmp_dir}/stale"
stale_markers="${tmp_dir}/stale-markers"
stale_log="${tmp_dir}/stale.log"
make_fixture "${stale_root}"
mkdir -p "${stale_markers}"
printf 'Stale install example: --tag v0.8.67\n' >"${stale_root}/README.ja-JP.md"

if PREPARE_RELEASE_TEST_MARKERS="${stale_markers}" \
  PATH="${stale_root}/bin:${PATH}" \
  "${stale_root}/scripts/release/prepare-release.sh" 0.9.0 \
  >"${stale_log}" 2>&1; then
  echo "stale README release tag unexpectedly passed" >&2
  exit 1
fi

grep -Fq \
  'README.ja-JP.md has release tag version(s) 0.8.67; expected 0.8.68' \
  "${stale_log}"
grep -Fq 'version = "0.8.68"' "${stale_root}/Cargo.toml"
grep -Fq 'version = "0.8.68"' "${stale_root}/crates/example/Cargo.toml"
grep -Fq '"version": "0.8.68"' "${stale_root}/npm/codewhale/package.json"
if find "${stale_markers}" -type f -print -quit | grep -q .; then
  echo "stale README validation mutated downstream release state" >&2
  exit 1
fi

echo "prepare-release tests passed"
