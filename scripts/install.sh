#!/usr/bin/env bash

set -euo pipefail

readonly LATE_BIN_NAME="late"
readonly LATE_DEFAULT_BASE_URL="https://cli.late.sh"
VERBOSE=0

log() {
  printf 'late installer: %s\n' "$*"
}

log_verbose() {
  if [[ "$VERBOSE" -eq 1 ]]; then
    printf 'late installer: %s\n' "$*"
  fi
}

fail() {
  printf 'late installer: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

is_wsl() {
  [[ -r /proc/sys/kernel/osrelease ]] && grep -qi microsoft /proc/sys/kernel/osrelease
}

detect_target() {
  local os arch

  os="$(uname -s)"
  arch="$(uname -m)"

  case "$arch" in
    x86_64|amd64)
      arch="x86_64"
      ;;
    arm64|aarch64)
      arch="aarch64"
      ;;
    *)
      fail "unsupported architecture: $arch"
      ;;
  esac

  case "$os" in
    Linux)
      printf '%s\n' "${arch}-unknown-linux-gnu"
      ;;
    Darwin)
      printf '%s\n' "${arch}-apple-darwin"
      ;;
    *)
      fail "unsupported operating system: $os"
      ;;
  esac
}

checksum_cmd() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s\n' "sha256sum"
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    printf '%s\n' "shasum -a 256"
    return
  fi

  printf '%s\n' ""
}

verify_checksum() {
  local checksum_file="$1"
  local target="$2"
  local downloaded_file="$3"
  local expected actual cmd

  expected="$(awk -v path="${target}/${LATE_BIN_NAME}" '$2 == path { print $1 }' "$checksum_file")"
  [[ -n "$expected" ]] || fail "missing checksum for ${target}/${LATE_BIN_NAME}"

  cmd="$(checksum_cmd)"
  if [[ -z "$cmd" ]]; then
    log "warning: no SHA-256 tool found; skipping checksum verification"
    return
  fi

  actual="$($cmd "$downloaded_file" | awk '{ print $1 }')"
  [[ "$actual" == "$expected" ]] || fail "checksum mismatch for ${LATE_BIN_NAME}"
}

install_binary() {
  local src="$1"
  local dest_dir="$2"
  local dest_path="${dest_dir}/${LATE_BIN_NAME}"

  mkdir -p "$dest_dir"

  if command -v install >/dev/null 2>&1; then
    install -m 755 "$src" "$dest_path"
  else
    cp "$src" "$dest_path"
    chmod 755 "$dest_path"
  fi

  printf '%s\n' "$dest_path"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --verbose|-v)
        VERBOSE=1
        ;;
      --help|-h)
        cat <<'EOF'
late installer

Options:
  -v, --verbose   Print resolved target, URLs, and install paths
  -h, --help      Show this help

Environment:
  LATE_INSTALL_BASE_URL   Override distribution host
  LATE_INSTALL_VERSION    Use a specific version instead of latest
EOF
        exit 0
        ;;
      *)
        fail "unknown argument: $1"
        ;;
    esac
    shift
  done
}

main() {
  local base_url version prefix target tmp_dir binary_url checksum_url target_dir dest_path

  parse_args "$@"
  need_cmd curl
  need_cmd uname
  need_cmd mktemp

  base_url="${LATE_INSTALL_BASE_URL:-$LATE_DEFAULT_BASE_URL}"
  version="${LATE_INSTALL_VERSION:-latest}"
  target="$(detect_target)"

  if is_wsl; then
    log "detected WSL; installing the Linux build"
  fi

  case "$version" in
    latest)
      prefix="latest"
      ;;
    *)
      prefix="releases/${version}"
      ;;
  esac

  tmp_dir="$(mktemp -d)"
  trap "rm -rf '$tmp_dir'" EXIT

  binary_url="${base_url%/}/${prefix}/${target}/${LATE_BIN_NAME}"
  checksum_url="${base_url%/}/${prefix}/sha256sums.txt"

  log_verbose "base_url=${base_url}"
  log_verbose "version=${version}"
  log_verbose "target=${target}"
  log_verbose "binary_url=${binary_url}"
  log_verbose "checksum_url=${checksum_url}"
  log "downloading ${target} from ${binary_url}"
  curl -fsSL "$binary_url" -o "${tmp_dir}/${LATE_BIN_NAME}"
  chmod 755 "${tmp_dir}/${LATE_BIN_NAME}"

  if curl -fsSL "$checksum_url" -o "${tmp_dir}/sha256sums.txt"; then
    verify_checksum "${tmp_dir}/sha256sums.txt" "$target" "${tmp_dir}/${LATE_BIN_NAME}"
  else
    log "warning: checksum file unavailable at ${checksum_url}; continuing without verification"
  fi

  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    target_dir="/usr/local/bin"
  else
    target_dir="${HOME}/.local/bin"
  fi

  log_verbose "target_dir=${target_dir}"
  dest_path="$(install_binary "${tmp_dir}/${LATE_BIN_NAME}" "$target_dir")"
  log "installed ${LATE_BIN_NAME} to ${dest_path}"

  case ":${PATH}:" in
    *":${target_dir}:"*)
      ;;
    *)
      log "warning: ${target_dir} is not currently on PATH"
      ;;
  esac

  if ! command -v ssh >/dev/null 2>&1; then
    log "warning: 'ssh' is required at runtime but was not found"
  fi

  if ! command -v script >/dev/null 2>&1; then
    log "warning: 'script' is required at runtime but was not found"
  fi

  log "run 'late --help' to verify the install"
}

main "$@"
