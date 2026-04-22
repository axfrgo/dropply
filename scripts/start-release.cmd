@echo off
setlocal
cd /d "%~dp0\.."
start "" "src-tauri\target\release\Dropply.exe"
