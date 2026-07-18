#!/usr/bin/env bash
set -euo pipefail

# Atomically install the exact binaries built by this checkout and leave a
# durable identity receipt. Replacing the directory entry (rather than copying
# over a running vnode) keeps live sessions on their old image while new shells
# get the new build safely.

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
src_dir="${1:-${repo_root}/target/release}"

if [[ ! -x "${src_dir}/codewhale" || ! -x "${src_dir}/codew" || ! -x "${src_dir}/codewhale-tui" ]]; then
  echo "ERROR: expected executable codewhale, codew, and codewhale-tui in ${src_dir}" >&2
  echo "Build first: cargo build --release -p codewhale-cli -p codewhale-tui --locked" >&2
  exit 1
fi

source_sha="$(git -C "${repo_root}" rev-parse HEAD)"
source_dirty="$(git -C "${repo_root}" status --porcelain --untracked-files=no)"
if [[ -n "${source_dirty}" ]]; then
  if [[ "${CODEWHALE_ALLOW_DIRTY_DOGFOOD:-0}" != "1" ]]; then
    echo "ERROR: refusing to install from a dirty source tree" >&2
    echo "Commit/stash the source, or set CODEWHALE_ALLOW_DIRTY_DOGFOOD=1 explicitly." >&2
    exit 1
  fi
  source_identity="${source_sha}-dirty"
else
  source_identity="${source_sha}"
fi

cli_version="$("${src_dir}/codewhale" --version)"
shim_version="$("${src_dir}/codew" --version)"
tui_version="$("${src_dir}/codewhale-tui" --version)"
short_sha="${source_sha:0:12}"
if [[ "${cli_version}" != *"${short_sha}"* || "${shim_version}" != *"${short_sha}"* || "${tui_version}" != *"${short_sha}"* ]]; then
  echo "ERROR: release binaries do not embed current HEAD ${short_sha}" >&2
  echo "  codewhale: ${cli_version}" >&2
  echo "  codew: ${shim_version}" >&2
  echo "  codewhale-tui: ${tui_version}" >&2
  echo "Rebuild this checkout before installing." >&2
  exit 1
fi
cli_sha="$(shasum -a 256 "${src_dir}/codewhale" | awk '{print $1}')"
shim_sha="$(shasum -a 256 "${src_dir}/codew" | awk '{print $1}')"
tui_sha="$(shasum -a 256 "${src_dir}/codewhale-tui" | awk '{print $1}')"

default_install_dirs="${HOME}/.cargo/bin:${HOME}/.local/bin"
for command_name in codewhale codewhale-tui codew; do
  if command_path="$(command -v "${command_name}" 2>/dev/null)" \
    && [[ "${command_path}" == "${HOME}/"* ]]; then
    command_dir="$(dirname "${command_path}")"
    if [[ ":${default_install_dirs}:" != *":${command_dir}:"* ]]; then
      default_install_dirs="${default_install_dirs}:${command_dir}"
    fi
  fi
done
IFS=':' read -r -a dest_dirs <<< "${CODEWHALE_INSTALL_DIRS:-${default_install_dirs}}"

install_binary() {
  local src="$1"
  local dst="$2"
  local tmp="${dst}.tmp.$$"
  trap 'rm -f -- "${tmp}"' RETURN
  cp "${src}" "${tmp}"
  chmod 0755 "${tmp}"
  mv -f "${tmp}" "${dst}"
  cmp -s "${src}" "${dst}" || {
    echo "ERROR: installed binary differs from source: ${dst}" >&2
    return 1
  }
  trap - RETURN
}

installed=()
for dest in "${dest_dirs[@]}"; do
  mkdir -p "${dest}"
  install_binary "${src_dir}/codewhale" "${dest}/codewhale"
  install_binary "${src_dir}/codew" "${dest}/codew"
  install_binary "${src_dir}/codewhale-tui" "${dest}/codewhale-tui"
  installed+=("${dest}/codewhale" "${dest}/codewhale-tui" "${dest}/codew")
done

verify_fresh_shell_binary() {
  local command_name="$1"
  local expected_sha="$2"
  local command_path
  local command_sha
  local command_version

  command_path="$(zsh -lc "command -v ${command_name}" 2>/dev/null || true)"
  if [[ -z "${command_path}" || ! -x "${command_path}" ]]; then
    echo "ERROR: fresh login shell cannot resolve ${command_name}" >&2
    return 1
  fi
  command_sha="$(shasum -a 256 "${command_path}" | awk '{print $1}')"
  if [[ "${command_sha}" != "${expected_sha}" ]]; then
    echo "ERROR: fresh-shell ${command_name} is not the installed build: ${command_path}" >&2
    return 1
  fi
  command_version="$(zsh -lc "${command_name} --version" 2>/dev/null || true)"
  if [[ "${command_version}" != *"${short_sha}"* ]]; then
    echo "ERROR: fresh-shell ${command_name} does not report current HEAD ${short_sha}" >&2
    return 1
  fi
  printf '%s\n' "${command_path}"
}

path_cli="$(verify_fresh_shell_binary codewhale "${cli_sha}")"
path_shim="$(verify_fresh_shell_binary codew "${shim_sha}")"
path_tui="$(verify_fresh_shell_binary codewhale-tui "${tui_sha}")"

default_receipt_root="${HOME}/.codewhale/dogfood-receipts"
if [[ -d "/Volumes/VIXinSSD/CW/backups" ]]; then
  default_receipt_root="/Volumes/VIXinSSD/CW/backups/dogfood-installs"
fi
receipt_root="${CODEWHALE_DOGFOOD_RECEIPT_DIR:-${default_receipt_root}}"
mkdir -p "${receipt_root}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
receipt="${receipt_root}/${timestamp}-${source_sha:0:12}.txt"
{
  echo "installed_at_utc=${timestamp}"
  echo "source_repo=${repo_root}"
  echo "source_commit=${source_identity}"
  echo "source_dir=${src_dir}"
  echo "codewhale_version=${cli_version}"
  echo "codewhale_sha256=${cli_sha}"
  echo "codew_version=${shim_version}"
  echo "codew_sha256=${shim_sha}"
  echo "codewhale_tui_version=${tui_version}"
  echo "codewhale_tui_sha256=${tui_sha}"
  echo "fresh_shell_codewhale=${path_cli}"
  echo "fresh_shell_codew=${path_shim}"
  echo "fresh_shell_codewhale_tui=${path_tui}"
  printf 'installed_path=%s\n' "${installed[@]}"
} >"${receipt}"

echo "Installed ${source_identity}:"
printf '  %s\n' "${installed[@]}"
echo "Receipt: ${receipt}"
echo "Fresh-shell check: zsh -lc 'type -a codew codewhale codewhale-tui; codew --version; codewhale-tui --version'"
