#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

manifest="${tmp_dir}/codewhale-artifacts-sha256.txt"
formula="${tmp_dir}/deepseek-tui.rb"

assets=(
  codewhale-macos-arm64
  codew-macos-arm64
  codewhale-tui-macos-arm64
  codewhale-macos-x64
  codew-macos-x64
  codewhale-tui-macos-x64
  codewhale-linux-arm64
  codew-linux-arm64
  codewhale-tui-linux-arm64
  codewhale-linux-x64
  codew-linux-x64
  codewhale-tui-linux-x64
)

for asset in "${assets[@]}"; do
  printf '%064d  %s\n' 0 "${asset}" >> "${manifest}"
done

TAG=v1.2.3 \
MANIFEST="${manifest}" \
TAP_REPO=Hmbown/homebrew-deepseek-tui \
FORMULA_OUTPUT="${formula}" \
  bash "${repo_root}/.github/scripts/update-homebrew-tap.sh"

ruby -c "${formula}" >/dev/null
grep -Fq 'desc "Agentic terminal for open-source and open-weight coding models"' "${formula}"
test "$(grep -Fc 'resource "codew" do' "${formula}")" -eq 4
grep -Fq 'bin.install Dir["*"].first => "codew"' "${formula}"
grep -Fq 'system "#{bin}/codew", "--version"' "${formula}"
grep -Fq 'system "#{bin}/codewhale-tui", "--version"' "${formula}"

echo "update-homebrew-tap tests passed"
