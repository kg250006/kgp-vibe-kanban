Placeholder mounted at `/seed/claude` when the host has no Claude Code profile.

Docker creates an empty **directory** at a bind-mount source that does not
exist, which would silently turn `~/.claude.json` into a folder on the host.
Pointing the seed mounts here instead keeps that from happening. The entrypoint
finds no `.credentials.json` and prints login instructions.
