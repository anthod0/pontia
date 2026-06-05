#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export PILOTFY_EXTERNAL_API_TOKEN="${PILOTFY_EXTERNAL_API_TOKEN:-dev-token}"

backend_pid=""
frontend_pid=""

terminate_tree() {
  local pid="$1"
  local child

  while read -r child; do
    [[ -n "$child" ]] && terminate_tree "$child"
  done < <(pgrep -P "$pid" 2>/dev/null || true)

  kill "$pid" 2>/dev/null || true
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM

  echo
  echo "Stopping pilotfy dev processes..."

  if [[ -n "$frontend_pid" ]] && kill -0 "$frontend_pid" 2>/dev/null; then
    terminate_tree "$frontend_pid"
  fi
  if [[ -n "$backend_pid" ]] && kill -0 "$backend_pid" 2>/dev/null; then
    terminate_tree "$backend_pid"
  fi

  if [[ -n "$frontend_pid" ]]; then
    wait "$frontend_pid" 2>/dev/null || true
  fi
  if [[ -n "$backend_pid" ]]; then
    wait "$backend_pid" 2>/dev/null || true
  fi

  exit "$status"
}

trap cleanup EXIT INT TERM

echo "Starting pilotfy backend with cargo run..."
echo "Using PILOTFY_EXTERNAL_API_TOKEN=$PILOTFY_EXTERNAL_API_TOKEN"
cargo run &
backend_pid=$!

echo "Starting dashboard Vite dev server..."
pnpm --dir apps/dashboard run dev -- --host 127.0.0.1 &
frontend_pid=$!

cat <<'EOF'

Development servers are starting:
  Backend API:        http://127.0.0.1:8080
  Dashboard dev UI:  http://127.0.0.1:5173/dashboard/

Open the Dashboard dev UI URL above for Vite HMR updates.
Press Ctrl-C to stop both processes.
EOF

set +e
wait -n "$backend_pid" "$frontend_pid"
status=$?
set -e

echo
if [[ $status -eq 0 ]]; then
  echo "A dev process exited. Shutting down the remaining process..."
else
  echo "A dev process failed with status $status. Shutting down the remaining process..."
fi
exit "$status"
