@echo off
call "C:\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul
set "PATH=C:\Users\alexj\Tools\node-v24.15.0-win-x64;C:\Users\alexj\.cargo\bin;%PATH%"
cd /d C:\Users\alexj\Documents\OpenDrop
npm run tauri dev
