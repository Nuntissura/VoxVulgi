@echo off
setlocal

set "REPO_ROOT=%~dp0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%REPO_ROOT%governance\scripts\vv_start.ps1"
exit /b %ERRORLEVEL%
