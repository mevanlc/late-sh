#!/usr/bin/env bash
# Run the local Late.sh Compose stack within this checkout's ownership boundary.

set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
ENV_FILE="${ROOT_DIR}/.env"

dotenv_value() {
  local key="$1"
  [[ -f "${ENV_FILE}" ]] || return 0
  awk -F= -v key="${key}" '
    $1 == key {
      value = substr($0, length(key) + 2)
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      print value
      exit
    }
  ' "${ENV_FILE}"
}

environment_instance="${INSTANCE:-}"
environment_project="${LATE_COMPOSE_PROJECT:-}"
dotenv_instance="$(dotenv_value INSTANCE)"
dotenv_project="$(dotenv_value LATE_COMPOSE_PROJECT)"
instance="${environment_instance:-${dotenv_instance:-late}}"

if [[ -n "${environment_project}" ]]; then
  project="${environment_project}"
elif [[ -n "${environment_instance}" ]]; then
  worktree_scope="$(printf '%s\n' "${ROOT_DIR}" | cksum | awk '{print $1}')"
  project="late-sh-${instance}-${worktree_scope}"
elif [[ -n "${dotenv_project}" ]]; then
  project="${dotenv_project}"
else
  worktree_scope="$(printf '%s\n' "${ROOT_DIR}" | cksum | awk '{print $1}')"
  project="late-sh-${instance}-${worktree_scope}"
fi

if [[ ! "${project}" =~ ^late-sh-[a-z0-9][a-z0-9_-]*$ ]]; then
  echo "invalid Late.sh Compose project name: ${project}" >&2
  echo "the name must start with late-sh- and use lowercase letters, digits, dashes, and underscores" >&2
  exit 2
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required" >&2
  exit 1
fi

compose=(docker compose)
if ! "${compose[@]}" version >/dev/null 2>&1; then
  if command -v docker-compose >/dev/null 2>&1; then
    compose=(docker-compose)
  else
    echo "docker compose is required" >&2
    exit 1
  fi
fi

exec "${compose[@]}" \
  --project-name "${project}" \
  --project-directory "${ROOT_DIR}" \
  --file "${ROOT_DIR}/docker-compose.yml" \
  "$@"
