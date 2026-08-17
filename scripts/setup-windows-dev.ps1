<#
.SYNOPSIS
  Bootstrap a Windows machine for native kgp-vibe-kanban development.

  Installs everything the build needs, in dependency order, skipping whatever
  is already present. Derived from a real from-zero bootstrap on Windows 11
  (2026-08-17); every pin and workaround below was hit in practice.

.EXAMPLE
  .\scripts\setup-windows-dev.ps1          # install missing pieces
  .\scripts\setup-windows-dev.ps1 -Check   # report only, install nothing

.NOTES
  Written for Windows PowerShell 5.1 — no ternary, no ??, no && chaining.
  VS Build Tools and LLVM are multi-GB downloads; first run takes a while.
#>
[CmdletBinding()]
param(
  [switch] $Check
)

$ErrorActionPreference = 'Stop'

function Write-Step { param($m) Write-Host "==> $m" -ForegroundColor Cyan }
function Write-Ok   { param($m) Write-Host "    $m" -ForegroundColor Green }
function Write-Warn { param($m) Write-Host "    $m" -ForegroundColor Yellow }

function Refresh-Path {
  $env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\LLVM\bin;" +
    [System.Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
    [System.Environment]::GetEnvironmentVariable('Path', 'User')
}

Write-Step 'kgp-vibe-kanban - Windows native dev bootstrap'
Refresh-Path
$missing = @()

# --- 1. Node.js (>= 20) ------------------------------------------------------
if (Get-Command node -ErrorAction SilentlyContinue) {
  Write-Ok "node      $(node --version)"
} else {
  $missing += 'node'
  if (-not $Check) {
    Write-Step 'Installing Node.js LTS'
    winget install --id OpenJS.NodeJS.LTS --accept-source-agreements --accept-package-agreements --silent
    Refresh-Path
  }
}

# --- 2. pnpm (10.x, matches packageManager in package.json) ------------------
if (Get-Command pnpm -ErrorAction SilentlyContinue) {
  Write-Ok "pnpm      $(pnpm --version)"
} else {
  $missing += 'pnpm'
  if (-not $Check) {
    Write-Step 'Installing pnpm 10.13.1'
    npm install -g pnpm@10.13.1
    Refresh-Path
  }
}

# --- 3. rustup (auto-installs the pinned nightly from rust-toolchain.toml) ---
if (Get-Command rustup -ErrorAction SilentlyContinue) {
  Write-Ok "rustup    $((rustup --version 2>&1 | Select-Object -First 1))"
} else {
  $missing += 'rustup'
  if (-not $Check) {
    Write-Step 'Installing rustup (the repo pin in rust-toolchain.toml downloads on first cargo use)'
    winget install --id Rustlang.Rustup --accept-source-agreements --accept-package-agreements --disable-interactivity --silent
    Refresh-Path
  }
}

# --- 4. MSVC linker + Windows SDK (Rust's msvc target links nothing without it)
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vcOk = $false
if (Test-Path $vswhere) {
  $p = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  if ($p) { $vcOk = $true; Write-Ok "MSVC      $p" }
}
if (-not $vcOk) {
  $missing += 'VS Build Tools (C++ workload)'
  if (-not $Check) {
    Write-Step 'Installing VS 2022 Build Tools + C++ workload + Windows SDK (several GB, be patient)'
    winget install --id Microsoft.VisualStudio.2022.BuildTools --accept-source-agreements --accept-package-agreements `
      --override '--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --includeRecommended'
  }
}

# --- 5. LLVM / libclang (libsqlite3-sys runs bindgen; same reason the
#         Dockerfile installs libclang-dev) ----------------------------------
if (Test-Path 'C:\Program Files\LLVM\bin\libclang.dll') {
  Write-Ok 'libclang  C:\Program Files\LLVM\bin'
} else {
  $missing += 'LLVM (libclang)'
  if (-not $Check) {
    Write-Step 'Installing LLVM'
    winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --disable-interactivity --silent
    Refresh-Path
  }
}

# --- 6. sqlx-cli 0.8.6 -------------------------------------------------------
# Version matters twice over:
#   - must be 0.8.x to match the workspace's sqlx 0.8.6 (query-cache format);
#   - sqlx-cli 0.9 requires rustc 1.94+, newer than the repo's pinned nightly,
#     so build it with the STABLE toolchain (+stable).
if (Get-Command cargo-sqlx -ErrorAction SilentlyContinue) {
  Write-Ok 'sqlx-cli  installed'
} else {
  $missing += 'sqlx-cli 0.8.6'
  if (-not $Check) {
    Write-Step 'Installing sqlx-cli 0.8.6 (built with stable)'
    rustup toolchain install stable
    cargo +stable install sqlx-cli --version 0.8.6 --no-default-features --features sqlite --locked
  }
}

# --- 7. Workspace dependencies ----------------------------------------------
$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (Test-Path (Join-Path $repoRoot 'node_modules')) {
  Write-Ok 'deps      node_modules present'
} else {
  $missing += 'pnpm install'
  if (-not $Check) {
    Write-Step 'Installing workspace dependencies'
    Push-Location $repoRoot
    try { pnpm install --frozen-lockfile } finally { Pop-Location }
  }
}

# --- Report ------------------------------------------------------------------
Write-Host ''
if ($Check) {
  if ($missing.Count) { Write-Warn ("Missing: " + ($missing -join ', ')); exit 1 }
  Write-Ok 'Everything present.'
  exit 0
}

Write-Step 'Done. Session environment for builds:'
Write-Host '  $env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"' -ForegroundColor White
Write-Host ''
Write-Host '  Build + verify:   pnpm -C packages/local-web build ; cargo check --workspace' -ForegroundColor White
Write-Host '  Run natively:     $env:PORT=3000; cargo run --bin server' -ForegroundColor White
Write-Host ''
Write-Warn 'Windows quirks (see README "Windows quirks"):'
Write-Warn '  - pnpm run check / lint start with ./scripts/*.sh - run those steps from Git Bash'
Write-Warn '  - scripts/check-unused-i18n-keys.mjs must run from Git Bash (needs POSIX find)'
Write-Warn '  - open a NEW terminal after first install so PATH updates apply'
