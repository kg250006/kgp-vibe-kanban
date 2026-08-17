# Self-contained Vibe Kanban (Docker, local only)

One container. SQLite. No login, no Postgres, no ElectricSQL, no relay, no cloud
API. Coding agents run inside the container.

This is the **local** app (`crates/server` + `packages/local-web`), not the cloud
stack in `crates/remote`.

Supported hosts: **Windows** (Docker Desktop + WSL2) and **macOS** (Docker
Desktop, Intel or Apple Silicon). Linux works too and is treated as macOS by the
setup script. The image itself is plain linux/amd64 or linux/arm64 — identical
on every host.

## Quick start

<table>
<tr><th>Windows</th><th>macOS / Linux</th></tr>
<tr><td>

```powershell
cd docker\kgp-local
.\setup.ps1
docker compose up -d --build
```

</td><td>

```bash
cd docker/kgp-local
./setup.sh
docker compose up -d --build
```

</td></tr>
</table>

Or let the setup script do it all: `.\setup.ps1 -Up` / `./setup.sh --up`.

> If `./setup.sh` reports "permission denied" after cloning on macOS, the exec
> bit didn't survive the checkout — use `bash setup.sh`, or stage it once with
> `git add --chmod=+x docker/kgp-local/setup.sh`.

The setup script detects your repos folder, your Claude Code profile, and your
git identity, writes `.env`, and preflights Docker. Re-run with `-Force` /
`--force` to regenerate.

First build is 10–20 minutes (Rust release build of the whole backend).
Subsequent builds reuse the cargo/pnpm cache mounts and are much faster.

Then open <http://localhost:3000>.

| Command | |
|---|---|
| `docker compose logs -f` | follow logs |
| `docker compose restart` | restart |
| `docker compose down` | stop (keeps data) |
| `docker compose down -v` | stop and **delete** the DB + agent state |
| `docker compose exec vibe-kanban bash` | shell inside the container |

## What makes it self-contained

`crates/local-deployment/src/lib.rs` enables cloud features only when
`VK_SHARED_API_BASE` is set, at runtime *or* baked in at build time via
`option_env!`. This setup sets it in neither place:

- The compose file passes no `VK_SHARED_API_BASE`, `VK_SHARED_RELAY_API_BASE`,
  or `VK_TUNNEL`, so the server logs `VK_SHARED_API_BASE not set; remote
  features disabled` on boot.
- The Dockerfile passes no `VITE_VK_SHARED_API_BASE`, so the frontend's
  `BUILD_TIME_API_BASE` compiles to `''`.
- A repo-root `.env` cannot leak into the build either — `.dockerignore`
  excludes it, so `crates/server/build.rs` sees nothing to bake in.

The entrypoint prints a warning if either variable ever shows up.

Note this is about *this app's* backend. The coding agent still reaches its own
model provider; `npx` also fetches the agent package from the npm registry on
first use (then it is cached on the `vk-home` volume).

## Layout

| Path | |
|---|---|
| `Dockerfile` | frontend build → Rust release build → Debian runtime **with Node.js** |
| `docker-compose.yml` | the single service, ports, volumes — identical on both OSes |
| `entrypoint.sh` | git config, credential seeding, then `exec server` |
| `setup.ps1` / `setup.sh` | host detection → generates `.env` |
| `.env.example` | template if you'd rather write `.env` by hand |
| `noseed/`, `noseed.json` | placeholders for hosts with no Claude Code profile |

Ports: `3000` web UI/API, `3001` preview proxy (dev servers agents start inside
workspaces are reachable through it).

Volumes:

| Host | Container | |
|---|---|---|
| `VK_REPOS_DIR` | `/repos` | your repositories, bind-mounted |
| `vk-home` | `/home/appuser` | SQLite DB (`~/.local/share/vibe-kanban`), npx cache, agent state |
| `VK_CLAUDE_DIR` | `/seed/claude` | read-only |
| `VK_CLAUDE_JSON` | `/seed/claude.json` | read-only |

## Coding agent credentials

The runtime image ships Node.js, because every executor shells out to `npx`
(e.g. `npx -y @anthropic-ai/claude-code@2.1.119`, see
`crates/executors/src/executors/claude.rs`). The stock root `Dockerfile` has no
Node and therefore cannot run any agent at all.

Your Claude Code profile is mounted **read-only**. On first start the entrypoint
copies `.credentials.json` (and `settings.json`, `.claude.json`) into the
container's own home. The container never writes back to your host profile, and
re-copies nothing on later starts.

**On macOS this seeding does not apply.** Claude Code stores its OAuth token in
the login Keychain there, not in `~/.claude/.credentials.json`, so there is no
file to copy. The entrypoint detects this and prints instructions.

Log in inside the container — this works on every host and is the only path that
supports Claude Remote Control:

```bash
docker compose exec vibe-kanban npx -y @anthropic-ai/claude-code /login
```

The login persists on the `vk-home` volume, so it survives restarts.

> **`ANTHROPIC_API_KEY` is a fallback only.** Claude Remote Control refuses
> API-key authentication — it requires a claude.ai subscription login. Setting a
> key instead of logging in makes that feature unusable in the container. If you
> are certain you will never use it:
> `echo 'ANTHROPIC_API_KEY=sk-ant-...' >> .env && docker compose up -d`

Windows and Linux hosts do use `.credentials.json`, so seeding covers normal
agent runs there without any of the above.

To add other agents (Codex, Gemini, …) no image change is needed — they are all
`npx`-invoked. Only their credentials need mounting or seeding the same way.

## Adding a project

In the UI, point the project at a path **under `/repos`** — e.g. `/repos/myapp`,
not `C:\PROJECTS\myapp` or `~/Projects/myapp`. The UI runs inside the container
and sees the container's filesystem.

## Host notes

### Windows

- **Requires WSL2.** Docker Desktop 29.x runs Linux containers on WSL2 only;
  there is no Hyper-V fallback. If `docker info` fails, run `wsl --install` from
  an **Administrator** PowerShell and reboot. `setup.ps1` checks this and says so.
- **Bind-mount speed.** Docker Desktop's Windows filesystem bridge is slow for
  git-heavy work. If a repo feels sluggish, move it onto the WSL2 filesystem and
  point `VK_REPOS_DIR` there.
- **Line endings.** The entrypoint sets `core.autocrlf false` so agent edits
  don't rewrite whole files. If a repo genuinely needs CRLF, set it per repo.

### macOS

- **Apple Silicon builds natively.** `rust:1.93-slim-bookworm` and
  `node:22-bookworm-slim` both publish arm64, so there is no Rosetta emulation
  and no `--platform` flag to set. The entrypoint logs the arch it booted on.
- **Enable VirtioFS** (Docker Desktop → Settings → General) for markedly better
  bind-mount performance than the legacy gRPC-FUSE sharing.
- **`VK_REPOS_DIR` must be under a shared path.** Docker Desktop shares `/Users`
  by default; if your repos live elsewhere, add that path under Settings →
  Resources → File sharing.
- **Keychain credentials** — see above.

### Both

- **The sign-in step looks broken, and that is expected.** In a self-contained
  install `GET /api/auth/methods` returns 400 — the handler calls
  `deployment.remote_client()?`, which has no client to return, and
  `crates/server/src/error.rs:100-104` maps that to a Bad Request. The onboarding
  page therefore shows *"Failed to load available sign-in methods"* with no
  buttons. Click **more options**, then
  **"I understand, continue without signing in."** No account is needed.
- **Worktree paths are container-absolute.** A worktree created in the container
  records `/repos/...` in `.git/worktrees/*/gitdir`, which host-side git cannot
  resolve. Don't run `git worktree` operations on the same repo from both the
  host and the container. Normal editing and committing from the host is fine.
- **"Failed to open browser automatically"** in the logs is expected — release
  builds try to launch a browser and there is none in the container.

## Rebuilding after code changes

```
docker compose up -d --build
```

The `vk-home` volume is seeded from the image only when it is first created, so
a rebuild does not refresh anything under `/home/appuser`. That only matters if
you change home-directory contents in the Dockerfile.
