#!/usr/bin/env bash
# Update the Homebrew tap at Hmbown/homebrew-deepseek-tui after a release.
#
# Expected environment:
#   TAG       – git tag, e.g. "v0.8.31"
#   MANIFEST  – path to codewhale-artifacts-sha256.txt
#   TAP_REPO  – owner/repo of the Homebrew tap
#   TOKEN     – PAT with contents:write on TAP_REPO (optional; skips if unset)
#   FORMULA_OUTPUT – optional local render path used by contract tests

set -euo pipefail

: "${TAG:?}"
: "${MANIFEST:?}"
: "${TAP_REPO:?}"

if [ -z "${TOKEN:-}" ] && [ -z "${FORMULA_OUTPUT:-}" ]; then
  echo "No Homebrew tap token configured; skipping."
  exit 0
fi

VERSION="${TAG#v}"

die() { echo "::error::${1}" >&2; exit 1; }

sha() {
  local file="${1:?}"
  local val
  val="$(awk -v f="${file}" '$2 == f {print $1; exit}' "${MANIFEST}")"
  if [ -z "${val}" ]; then
    die "Missing binary in checksum manifest: ${file}"
  fi
  echo "${val}"
}

# --- read checksums ---------------------------------------------------

# Canonical dispatcher and TUI
SHA_COD_MACOS_ARM="$(sha codewhale-macos-arm64)"
SHA_CODEW_MACOS_ARM="$(sha codew-macos-arm64)"
SHA_TUI_MACOS_ARM="$(sha codewhale-tui-macos-arm64)"
SHA_COD_MACOS_X64="$(sha codewhale-macos-x64)"
SHA_CODEW_MACOS_X64="$(sha codew-macos-x64)"
SHA_TUI_MACOS_X64="$(sha codewhale-tui-macos-x64)"
SHA_COD_LINUX_ARM="$(sha codewhale-linux-arm64)"
SHA_CODEW_LINUX_ARM="$(sha codew-linux-arm64)"
SHA_TUI_LINUX_ARM="$(sha codewhale-tui-linux-arm64)"
SHA_COD_LINUX_X64="$(sha codewhale-linux-x64)"
SHA_CODEW_LINUX_X64="$(sha codew-linux-x64)"
SHA_TUI_LINUX_X64="$(sha codewhale-tui-linux-x64)"
readonly SHA_COD_MACOS_ARM SHA_CODEW_MACOS_ARM SHA_TUI_MACOS_ARM
readonly SHA_COD_MACOS_X64 SHA_CODEW_MACOS_X64 SHA_TUI_MACOS_X64
readonly SHA_COD_LINUX_ARM SHA_CODEW_LINUX_ARM SHA_TUI_LINUX_ARM
readonly SHA_COD_LINUX_X64 SHA_CODEW_LINUX_X64 SHA_TUI_LINUX_X64

# --- temp dirs --------------------------------------------------------

FORMULA_FILE="$(mktemp)"
TAP_DIR="$(mktemp -d)"
trap 'rm -rf "${TAP_DIR}" "${FORMULA_FILE}"' EXIT

# --- generate formula --------------------------------------------------

readonly BASE_URL="https://github.com/Hmbown/CodeWhale/releases/download/${TAG}"

cat > "${FORMULA_FILE}" << EOF
class DeepseekTui < Formula
  desc "Agentic terminal for open-source and open-weight coding models"
  homepage "https://github.com/Hmbown/CodeWhale"
  version "${VERSION}"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "${BASE_URL}/codewhale-macos-arm64", using: :nounzip
      sha256 "${SHA_COD_MACOS_ARM}"
      resource "codew" do
        url "${BASE_URL}/codew-macos-arm64", using: :nounzip
        sha256 "${SHA_CODEW_MACOS_ARM}"
      end
      resource "tui" do
        url "${BASE_URL}/codewhale-tui-macos-arm64", using: :nounzip
        sha256 "${SHA_TUI_MACOS_ARM}"
      end
    else
      url "${BASE_URL}/codewhale-macos-x64", using: :nounzip
      sha256 "${SHA_COD_MACOS_X64}"
      resource "codew" do
        url "${BASE_URL}/codew-macos-x64", using: :nounzip
        sha256 "${SHA_CODEW_MACOS_X64}"
      end
      resource "tui" do
        url "${BASE_URL}/codewhale-tui-macos-x64", using: :nounzip
        sha256 "${SHA_TUI_MACOS_X64}"
      end
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "${BASE_URL}/codewhale-linux-arm64", using: :nounzip
      sha256 "${SHA_COD_LINUX_ARM}"
      resource "codew" do
        url "${BASE_URL}/codew-linux-arm64", using: :nounzip
        sha256 "${SHA_CODEW_LINUX_ARM}"
      end
      resource "tui" do
        url "${BASE_URL}/codewhale-tui-linux-arm64", using: :nounzip
        sha256 "${SHA_TUI_LINUX_ARM}"
      end
    else
      url "${BASE_URL}/codewhale-linux-x64", using: :nounzip
      sha256 "${SHA_COD_LINUX_X64}"
      resource "codew" do
        url "${BASE_URL}/codew-linux-x64", using: :nounzip
        sha256 "${SHA_CODEW_LINUX_X64}"
      end
      resource "tui" do
        url "${BASE_URL}/codewhale-tui-linux-x64", using: :nounzip
        sha256 "${SHA_TUI_LINUX_X64}"
      end
    end
  end

  def install
    bin.install Dir["*"].first => "codewhale"
    resource("codew").stage { bin.install Dir["*"].first => "codew" }
    resource("tui").stage { bin.install Dir["*"].first => "codewhale-tui" }
  end

  test do
    system "#{bin}/codewhale", "--version"
    system "#{bin}/codew", "--version"
    system "#{bin}/codewhale-tui", "--version"
  end
end
EOF

if [ -n "${FORMULA_OUTPUT:-}" ]; then
  cp "${FORMULA_FILE}" "${FORMULA_OUTPUT}"
  echo "Rendered Homebrew formula to ${FORMULA_OUTPUT}"
  exit 0
fi

# --- push to tap repo --------------------------------------------------

ENCODED_TOKEN="$(printf '%s' "${TOKEN}" | python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.stdin.read(),safe=""))')"
TAP_URL="https://x-access-token:${ENCODED_TOKEN}@github.com/${TAP_REPO}.git"

git clone --depth 1 "${TAP_URL}" "${TAP_DIR}"

mkdir -p "${TAP_DIR}/Formula"
cp "${FORMULA_FILE}" "${TAP_DIR}/Formula/deepseek-tui.rb"

cd "${TAP_DIR}"
git config user.name  "github-actions[bot]"
git config user.email "github-actions[bot]@users.noreply.github.com"

git add Formula/deepseek-tui.rb

if git diff --cached --quiet; then
  echo "Formula unchanged (already at ${VERSION}); nothing to push."
  exit 0
fi

git commit -m "chore: bump formula to ${VERSION}

Automated update from the release workflow."

git push origin HEAD:main
echo "Pushed formula update to ${TAP_REPO} (v${VERSION})"
