#!/bin/sh
set -eu

# --- git ---------------------------------------------------------------------
# Bind-mounted host paths arrive owned by a uid that is not appuser, so git
# refuses to touch them ("detected dubious ownership") without this.
git config --global --add safe.directory '*'
git config --global user.name "${GIT_USER_NAME:-Vibe Kanban}"
git config --global user.email "${GIT_USER_EMAIL:-vibe-kanban@localhost}"
git config --global init.defaultBranch main
# Repos authored on Windows commonly carry CRLF in the working tree; leave the
# bytes untouched so agent edits do not produce whole-file diffs.
git config --global core.autocrlf false

# --- coding agent credentials ------------------------------------------------
# /seed/* are read-only mounts of the host's Claude Code profile. Copy once into
# the container's own home so the agent can write session state without ever
# mutating the host installation. Nothing is re-copied on later starts.
seeded_creds=0

if [ -f /seed/claude/.credentials.json ] && [ ! -f "$HOME/.claude/.credentials.json" ]; then
  mkdir -p "$HOME/.claude"
  cp /seed/claude/.credentials.json "$HOME/.claude/.credentials.json"
  chmod 600 "$HOME/.claude/.credentials.json"
  seeded_creds=1
  echo "kgp-entrypoint: seeded Claude Code credentials from host profile"
fi

if [ -f /seed/claude/settings.json ] && [ ! -f "$HOME/.claude/settings.json" ]; then
  cp /seed/claude/settings.json "$HOME/.claude/settings.json"
fi

if [ -f /seed/claude.json ] && [ ! -f "$HOME/.claude.json" ]; then
  cp /seed/claude.json "$HOME/.claude.json"
fi

# Tell the user how to authenticate when there is nothing usable. This is the
# normal case on macOS hosts: Claude Code keeps its OAuth tokens in the login
# Keychain there, not in ~/.claude/.credentials.json, so there is no file to
# seed even when the profile directory is mounted.
if [ "$seeded_creds" -eq 0 ] \
   && [ ! -f "$HOME/.claude/.credentials.json" ] \
   && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
  echo "kgp-entrypoint: no Claude Code credentials in the container." >&2
  echo "kgp-entrypoint:   log in once:  docker compose exec vibe-kanban npx -y @anthropic-ai/claude-code /login" >&2
  echo "kgp-entrypoint:   or set ANTHROPIC_API_KEY in docker/kgp-local/.env" >&2
  echo "kgp-entrypoint:   (note: an API key disables Claude Remote Control, which" >&2
  echo "kgp-entrypoint:    requires a claude.ai subscription login)" >&2
fi

# --- sanity ------------------------------------------------------------------
if [ -n "${VK_SHARED_API_BASE:-}" ] || [ -n "${VK_SHARED_RELAY_API_BASE:-}" ]; then
  echo "kgp-entrypoint: WARNING - VK_SHARED_API_BASE/VK_SHARED_RELAY_API_BASE are set;" \
       "this instance is no longer self-contained." >&2
fi

echo "kgp-entrypoint: $(uname -m), node $(node --version), $(git --version)"
echo "kgp-entrypoint: starting server on ${HOST:-0.0.0.0}:${PORT:-3000} (preview proxy ${PREVIEW_PROXY_PORT:-3001})"

exec /usr/local/bin/server "$@"
