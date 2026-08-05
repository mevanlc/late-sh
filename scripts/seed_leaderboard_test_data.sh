#!/usr/bin/env bash
#
# Populate the local Docker Compose database with synthetic leaderboard data.
#
# Usage:
#   scripts/seed_leaderboard_test_data.sh
#
# Optional env:
#   LATE_DB_USER=postgres
#   LATE_DB_NAME=postgres

set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
SCRIPT_DIR="${ROOT_DIR}/scripts"
COMPOSE=("${SCRIPT_DIR}/dev_compose.sh")

echo "-> ensuring local postgres is running"
"${COMPOSE[@]}" up -d --wait postgres >/dev/null

echo "-> replacing synthetic leaderboard activity"
"${COMPOSE[@]}" exec -T postgres psql \
  -U "${LATE_DB_USER:-postgres}" \
  -d "${LATE_DB_NAME:-postgres}" \
  -v ON_ERROR_STOP=1 \
  <"${SCRIPT_DIR}/seed_leaderboard_test_data.sql"
