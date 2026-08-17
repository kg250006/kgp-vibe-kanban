#!/usr/bin/env bash
# =============================================================================
#  Vibe Kanban - one-click local launcher (macOS)
#
#  Double-click in Finder to start the fully-local app and open it in your
#  browser. Safe to run repeatedly: if the app is already up, it just opens
#  the tab.
#
#  Make it double-clickable after a fresh clone (once):
#    chmod +x scripts/start-vibe-kanban.command
#  Drag it to the Dock or make an alias on the Desktop for one-click access.
# =============================================================================
set -euo pipefail

PORT=5262
URL="http://localhost:${PORT}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

# --- already running? just open the browser ----------------------------------
if curl -s -o /dev/null --max-time 2 "${URL}/api/health"; then
  echo "Vibe Kanban is already running - opening ${URL}"
  open "$URL"
  exit 0
fi

# --- self-heal: frontend bundle -----------------------------------------------
if [ ! -f "packages/local-web/dist/index.html" ]; then
  echo "[first run] Building the web app - this can take a couple of minutes..."
  NODE_OPTIONS=--max-old-space-size=4096 pnpm -C packages/local-web build
fi

# --- self-heal: server binary -------------------------------------------------
if [ ! -x "target/release/server" ]; then
  echo "[first run] Building the server - 10-20 minutes on a fresh machine..."
  echo "            (needs Rust: https://rustup.rs, plus pnpm and Xcode CLT)"
  cargo build --release --bin server
fi

# --- start --------------------------------------------------------------------
echo "Starting Vibe Kanban on ${URL} ..."
# Release builds open the browser themselves once ready; keep logs in ~/.
PORT="$PORT" nohup target/release/server >> "$HOME/vibe-kanban.log" 2>&1 &

# Fallback: if the browser hasn't opened within ~20s, open it ourselves.
for _ in $(seq 1 10); do
  sleep 2
  if curl -s -o /dev/null --max-time 2 "${URL}/api/health"; then
    exit 0
  fi
done
echo "Server did not come up in time - see ~/vibe-kanban.log"
exit 1
