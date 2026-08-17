@echo off
rem ============================================================================
rem  Vibe Kanban - one-click local launcher (Windows)
rem
rem  Double-click to start the fully-local app and open it in your browser.
rem  Safe to run repeatedly: if the app is already up, it just opens the tab.
rem
rem  Make a desktop shortcut: right-click this file -> Send to -> Desktop.
rem ============================================================================
setlocal
set "PORT=5262"
set "REPO=%~dp0.."
set "URL=http://localhost:%PORT%"
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
set "PATH=%USERPROFILE%\.cargo\bin;C:\Program Files\LLVM\bin;%PATH%"

cd /d "%REPO%"

rem --- already running? just open the browser --------------------------------
curl -s -o NUL --max-time 2 "%URL%/api/health"
if %errorlevel%==0 (
    echo Vibe Kanban is already running - opening %URL%
    start "" "%URL%"
    goto :done
)

rem --- self-heal: frontend bundle --------------------------------------------
if not exist "packages\local-web\dist\index.html" (
    echo [first run] Building the web app - this can take a couple of minutes...
    set "NODE_OPTIONS=--max-old-space-size=4096"
    call pnpm -C packages\local-web build || goto :fail
)

rem --- self-heal: server binary ----------------------------------------------
if not exist "target\release\server.exe" (
    echo [first run] Building the server - 10-20 minutes on a fresh machine...
    echo             ^(needs the toolchain from scripts\setup-windows-dev.ps1^)
    cargo build --release --bin server || goto :fail
)

rem --- start ------------------------------------------------------------------
echo Starting Vibe Kanban on %URL% ...
rem Release builds open the browser themselves once ready.
start "Vibe Kanban" /min cmd /c "set PORT=%PORT%&& target\release\server.exe"

rem Fallback: if the browser hasn't opened within ~20s, open it ourselves.
for /l %%i in (1,1,10) do (
    rem ping is a stdin-immune 2s delay (timeout.exe refuses redirected stdin)
    ping -n 3 127.0.0.1 >NUL
    curl -s -o NUL --max-time 2 "%URL%/api/health" && goto :done
)
echo Server did not come up in time - check the "Vibe Kanban" console window.
pause
goto :done

:fail
echo.
echo Build failed. Run scripts\setup-windows-dev.ps1 first, then try again.
pause

:done
endlocal
