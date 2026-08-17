<p align="center">
  <a href="https://vibekanban.com">
    <picture>
      <source srcset="packages/public/vibe-kanban-logo-dark.svg" media="(prefers-color-scheme: dark)">
      <source srcset="packages/public/vibe-kanban-logo.svg" media="(prefers-color-scheme: light)">
      <img src="packages/public/vibe-kanban-logo.svg" alt="Vibe Kanban Logo">
    </picture>
  </a>
</p>

<p align="center">Get 10X more out of Claude Code, Gemini CLI, Codex, Amp and other coding agents...</p>
<p align="center">
  <a href="https://www.npmjs.com/package/vibe-kanban"><img alt="npm" src="https://img.shields.io/npm/v/vibe-kanban?style=flat-square" /></a>
  <a href="https://github.com/BloopAI/vibe-kanban/blob/main/.github/workflows/publish.yml"><img alt="Build status" src="https://img.shields.io/github/actions/workflow/status/BloopAI/vibe-kanban/.github%2Fworkflows%2Fpublish.yml" /></a>
  <a href="https://deepwiki.com/BloopAI/vibe-kanban"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki"></a>
</p>

<h1 align="center">
  <strong>Vibe Kanban is sunsetting.</strong>
  <a href="https://www.vibekanban.com/blog/shutdown">Read the announcement.</a>
</h1>

![](packages/public/vibe-kanban-screenshot-overview.png)

## Overview

In a world where software engineers spend most of their time planning and reviewing coding agents, the most impactful way to ship more is to get faster at planning and review.

Vibe Kanban is built for this. Use kanban issues to plan work, either privately or with your team. When you're ready to begin, create workspaces where coding agents can execute.

- **Plan with kanban issues** — create, prioritise, and assign issues on a kanban board
- **Run coding agents in workspaces** — each workspace gives an agent a branch, a terminal, and a dev server
- **Review diffs and leave inline comments** — send feedback directly to the agent without leaving the UI
- **Preview your app** — built-in browser with devtools, inspect mode, and device emulation
- **Switch between 10+ coding agents** — Claude Code, Codex, Gemini CLI, GitHub Copilot, Amp, Cursor, OpenCode, Droid, CCR, and Qwen Code
- **Create pull requests and merge** — open PRs with AI-generated descriptions, review on GitHub, and merge

![](packages/public/vibe-kanban-screenshot-workspace.png)

One command. Describe the work, review the diff, ship it.

```bash
npx vibe-kanban
```


## Installation

Make sure you have authenticated with your favourite coding agent. A full list of supported coding agents can be found in the [docs](https://vibekanban.com/docs/supported-coding-agents). Then in your terminal run:

```bash
npx vibe-kanban
```

## Documentation

Head to the [website](https://vibekanban.com/docs) for the latest documentation and user guides.

## Self-Hosting

Want to host your own Vibe Kanban Cloud instance? See our [self-hosting guide](https://vibekanban.com/docs/self-hosting/deploy-docker).

## Support

We use [GitHub Discussions](https://github.com/BloopAI/vibe-kanban/discussions) for feature requests. Please open a discussion to create a feature request. For bugs please open an issue on this repo.

## Contributing

We would prefer that ideas and changes are first raised with the core team via [GitHub Discussions](https://github.com/BloopAI/vibe-kanban/discussions) or [Discord](https://discord.gg/AC4nwVtJM3), where we can discuss implementation details and alignment with the existing roadmap. Please do not open PRs without first discussing your proposal with the team.

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) (>=20)
- [pnpm](https://pnpm.io/) (>=8)

Additional development tools:
```bash
cargo install cargo-watch
cargo install sqlx-cli
```

Install dependencies:
```bash
pnpm i
```

#### One-click launch

After setup, start the whole local app with a double-click:

| OS | Launcher | Shortcut |
|---|---|---|
| Windows | `scripts\start-vibe-kanban.bat` | right-click → **Send to → Desktop** |
| macOS | `scripts/start-vibe-kanban.command` | `chmod +x` once, then drag to Dock |

The launcher is idempotent: if the app is already running it just opens
`http://localhost:5262` (KANB on a phone keypad — picked to never collide with
dev servers squatting on 3000); otherwise it starts the release server (building the
frontend bundle and/or server binary first if they're missing) and opens the
browser once healthy.

> Release builds store data in the per-user app directory
> (`%APPDATA%\bloop\vibe-kanban\data` on Windows, `~/Library/Application
> Support/ai.bloop.vibe-kanban` on macOS) — separate from the `dev_assets/` DB
> that debug builds use.

#### Windows: one-command native setup

Everything the build needs (rustup + pinned nightly, VS 2022 Build Tools C++
workload + Windows SDK, LLVM/libclang, pnpm, sqlx-cli, workspace deps) installs
idempotently via:

```powershell
.\scripts\setup-windows-dev.ps1          # installs whatever is missing
.\scripts\setup-windows-dev.ps1 -Check   # report only
```

Then build and run natively — no Docker or WSL2 required:

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
pnpm -C packages/local-web build
$env:PORT = 5262; cargo run --bin server    # http://localhost:5262
```

Hard-won pins the script encodes (do not "upgrade" these casually):

- **sqlx-cli must be 0.8.x** to match the workspace's sqlx 0.8.6 query-cache
  format, and **must be built with `cargo +stable`** — sqlx-cli 0.9 requires
  rustc 1.94+, newer than the repo's pinned nightly.
- **LLVM/libclang is mandatory**: `libsqlite3-sys` runs bindgen (the Dockerfile
  installs `libclang-dev` for the same reason). Set `LIBCLANG_PATH` for builds.
- **MSVC Build Tools + Windows SDK are mandatory**: Rust's default
  `x86_64-pc-windows-msvc` target cannot link a single binary without them.

##### Windows quirks

- `pnpm run check` and `pnpm run lint` begin with `./scripts/*.sh`, which
  cmd.exe cannot execute. Run those composite scripts from **Git Bash**, or run
  each sub-step (`web-core:check`, `ui:lint`, `backend:check`, …) individually.
- `scripts/check-unused-i18n-keys.mjs` shells out to POSIX `find` and silently
  swallows the failure on Windows, reporting **every** key as unused. Run it
  from Git Bash for a true result.
- Three upstream `executors::claude::tests` cases hardcode POSIX paths
  (`/tmp/test-worktree`) and fail on Windows path separators. Pre-existing;
  unrelated to local changes.

### Running the dev server

```bash
pnpm run dev
```

This will start the backend and web app. A blank DB will be copied from the `dev_assets_seed` folder.

### Building the web app

To build just the web app:

```bash
cd packages/local-web
pnpm run build
```

### Build from source (macOS)

1. Run `./local-build.sh`
2. Test with `cd npx-cli && node bin/cli.js`

## Two install modes

This fork builds two incompatible ways. Which one a given checkout is for is recorded in the git-ignored `.kgp-install-mode` file at the repo root, written by `docker/kgp-local/setup.ps1` / `setup.sh`.

| | Mode A — cloud-connected | Mode B — self-contained |
|---|---|---|
| Built by | `.github/workflows/kgp-local-cli.yml` | `docker/kgp-local/` |
| `VK_SHARED_API_BASE` | baked at build time | never set |
| Cloud features | on | `remote features disabled` |
| VK Remote Access (relay) | available | unavailable |
| Claude Remote Control | available | available |

Three things to know before changing modes:

- **Runtime beats build time, but only one way.** `crates/local-deployment/src/lib.rs:171-176` reads `std::env::var(...).ok().or_else(|| option_env!(...))`. A local-baked binary *can* be pointed at a cloud at runtime, but a cloud-baked binary **cannot be forced local by unsetting** the variable — it falls through to the baked value. Rebuild instead.
- **Empty string is a trap.** `VK_SHARED_API_BASE=` yields `Ok("")`, which passes the `Some(url)` branch at `crates/local-deployment/src/lib.rs:189` and attempts `RemoteClient::new("")`. There is no `is_empty()` guard, even though `crates/remote/AGENTS.md:174` warns about exactly this. Omit the variable; never set it empty.
- **The frontend is sticky to its build.** `packages/web-core/src/shared/lib/remoteApi.ts:30` is `_remoteApiBase = base || BUILD_TIME_API_BASE`, so a `null` from the server cannot clear a baked-in base. Changing modes requires a frontend rebuild, not just a backend env change.

### Environment Variables

The following environment variables can be configured at build time or runtime:

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `POSTHOG_API_KEY` | Build-time | Empty | PostHog analytics API key (disables analytics if empty) |
| `POSTHOG_API_ENDPOINT` | Build-time | Empty | PostHog analytics endpoint (disables analytics if empty) |
| `PORT` | Runtime | Auto-assign | **Production**: Server port. **Dev**: Frontend port (backend uses PORT+1) |
| `BACKEND_PORT` | Runtime | `0` (auto-assign) | Backend server port (dev mode only, overrides PORT+1) |
| `FRONTEND_PORT` | Runtime | `3000` | Frontend dev server port (dev mode only, overrides PORT) |
| `HOST` | Runtime | `127.0.0.1` | Backend server host |
| `MCP_HOST` | Runtime | Value of `HOST` | MCP server connection host (use `127.0.0.1` when `HOST=0.0.0.0` on Windows) |
| `MCP_PORT` | Runtime | Value of `BACKEND_PORT` | MCP server connection port |
| `DISABLE_WORKTREE_CLEANUP` | Runtime | Not set | Disable all git worktree cleanup including orphan and expired workspace cleanup (for debugging) |
| `VK_ALLOWED_ORIGINS` | Runtime | Not set | Comma-separated list of origins that are allowed to make backend API requests (e.g., `https://my-vibekanban-frontend.com`) |
| `VK_SHARED_API_BASE` | Build-time + Runtime | Not set | Base URL for the remote/cloud API used by the local desktop app. Read at runtime, falling back to a value baked in at build time via `option_env!` — see [Two install modes](#two-install-modes) |
| `VK_SHARED_RELAY_API_BASE` | Build-time + Runtime | Not set | Base URL for the relay API used by tunnel-mode connections. Same build-time/runtime resolution as above |
| `VK_TUNNEL` | Runtime | Not set | Enable relay tunnel mode when set (requires relay API base URL) |

**Build-time variables** must be set when running `pnpm run build`. **Runtime variables** are read when the application starts.

#### Self-Hosting with a Reverse Proxy or Custom Domain

When running Vibe Kanban behind a reverse proxy (e.g., nginx, Caddy, Traefik) or on a custom domain, you must set the `VK_ALLOWED_ORIGINS` environment variable. Without this, the browser's Origin header won't match the backend's expected host, and API requests will be rejected with a 403 Forbidden error.

Set it to the full origin URL(s) where your frontend is accessible:

```bash
# Single origin
VK_ALLOWED_ORIGINS=https://vk.example.com

# Multiple origins (comma-separated)
VK_ALLOWED_ORIGINS=https://vk.example.com,https://vk-staging.example.com
```

### Remote Deployment

When running Vibe Kanban on a remote server (e.g., via systemctl, Docker, or cloud hosting), you can configure your editor to open projects via SSH:

1. **Access via tunnel**: Use Cloudflare Tunnel, ngrok, or similar to expose the web UI
2. **Configure remote SSH** in Settings → Editor Integration:
   - Set **Remote SSH Host** to your server hostname or IP
   - Set **Remote SSH User** to your SSH username (optional)
3. **Prerequisites**:
   - SSH access from your local machine to the remote server
   - SSH keys configured (passwordless authentication)
   - VSCode Remote-SSH extension

When configured, the "Open in VSCode" buttons will generate URLs like `vscode://vscode-remote/ssh-remote+user@host/path` that open your local editor and connect to the remote server.

See the [documentation](https://vibekanban.com/docs/settings/general) for detailed setup instructions.
