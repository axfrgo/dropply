@echo off
setlocal
call "C:\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul
set "PATH=C:\Users\alexj\Tools\node-v24.15.0-win-x64;C:\Users\alexj\.cargo\bin;%PATH%"
cd /d "%~dp0\.."

if exist "src-tauri\target\release\bundle\msi\Dropply_*.msi" del /q "src-tauri\target\release\bundle\msi\Dropply_*.msi"
if exist "src-tauri\target\release\bundle\nsis\Dropply_*.exe" del /q "src-tauri\target\release\bundle\nsis\Dropply_*.exe"
if exist "src-tauri\target\release\bundle\cli" rmdir /s /q "src-tauri\target\release\bundle\cli"
if exist "src-tauri\target\release\bundle\github-release" rmdir /s /q "src-tauri\target\release\bundle\github-release"
if exist "src-tauri\target\release\bundle\Dropply_BrowserShare_*.zip" del /q "src-tauri\target\release\bundle\Dropply_BrowserShare_*.zip"

call "C:\Users\alexj\Tools\node-v24.15.0-win-x64\npm.cmd" run tauri:build
if errorlevel 1 exit /b %errorlevel%
call cargo build --manifest-path src-tauri\Cargo.toml --release --bin dropply-cli
if errorlevel 1 exit /b %errorlevel%

for /f "tokens=2 delims=:, " %%a in ('findstr /c:"\"version\"" package.json') do set "APP_VERSION=%%~a"
set "APP_VERSION=%APP_VERSION:"=%"

if exist "src-tauri\target\release\bundle\msi\Dropply_%APP_VERSION%_x64_en-US.msi" (
  ren "src-tauri\target\release\bundle\msi\Dropply_%APP_VERSION%_x64_en-US.msi" "Dropply_%APP_VERSION%_x64_EN-FR.msi"
)

if exist "src-tauri\target\release\bundle\nsis\Dropply_%APP_VERSION%_x64-setup.exe" (
  ren "src-tauri\target\release\bundle\nsis\Dropply_%APP_VERSION%_x64-setup.exe" "Dropply_%APP_VERSION%_x64_EN-FR-setup.exe"
)

mkdir "src-tauri\target\release\bundle\cli"
copy /y "src-tauri\target\release\dropply-cli.exe" "src-tauri\target\release\bundle\cli\dropply-cli.exe" >nul
copy /y "docs\DROPPLY_CLI_QUICKSTART.txt" "src-tauri\target\release\bundle\cli\README.txt" >nul
copy /y "scripts\install-dropply-cli.ps1" "src-tauri\target\release\bundle\cli\install-dropply-cli.ps1" >nul
copy /y "scripts\install-dropply-cli.cmd" "src-tauri\target\release\bundle\cli\install-dropply-cli.cmd" >nul
copy /y "scripts\uninstall-dropply-cli.ps1" "src-tauri\target\release\bundle\cli\uninstall-dropply-cli.ps1" >nul
copy /y "scripts\uninstall-dropply-cli.cmd" "src-tauri\target\release\bundle\cli\uninstall-dropply-cli.cmd" >nul

powershell -NoProfile -Command "Compress-Archive -Path 'src-tauri\target\release\bundle\cli\dropply-cli.exe','src-tauri\target\release\bundle\cli\README.txt','src-tauri\target\release\bundle\cli\install-dropply-cli.ps1','src-tauri\target\release\bundle\cli\install-dropply-cli.cmd','src-tauri\target\release\bundle\cli\uninstall-dropply-cli.ps1','src-tauri\target\release\bundle\cli\uninstall-dropply-cli.cmd' -DestinationPath 'src-tauri\target\release\bundle\cli\Dropply_CLI_%APP_VERSION%_Windows_x64_EN-FR.zip' -Force"
powershell -NoProfile -Command "Compress-Archive -Path 'browser-extension\dropply-share' -DestinationPath 'src-tauri\target\release\bundle\Dropply_BrowserShare_%APP_VERSION%_Extension.zip' -Force"
powershell -NoProfile -Command "$files = @('src-tauri\target\release\bundle\msi\Dropply_%APP_VERSION%_x64_EN-FR.msi','src-tauri\target\release\bundle\nsis\Dropply_%APP_VERSION%_x64_EN-FR-setup.exe','src-tauri\target\release\bundle\cli\Dropply_CLI_%APP_VERSION%_Windows_x64_EN-FR.zip','src-tauri\target\release\bundle\Dropply_BrowserShare_%APP_VERSION%_Extension.zip'); $lines = foreach ($file in $files) { if (Test-Path $file) { $name = [System.IO.Path]::GetFileName($file); $hash = (Get-FileHash -Algorithm SHA256 -Path $file).Hash.ToUpperInvariant(); \"$name`r`nSHA-256: $hash`r`n\" } }; Set-Content -Path 'src-tauri\target\release\bundle\Dropply_%APP_VERSION%_x64_EN-FR_SHA256.txt' -Value $lines -Encoding ASCII"

mkdir "src-tauri\target\release\bundle\github-release"
copy /y "src-tauri\target\release\bundle\msi\Dropply_%APP_VERSION%_x64_EN-FR.msi" "src-tauri\target\release\bundle\github-release\" >nul
if errorlevel 1 exit /b %errorlevel%
copy /y "src-tauri\target\release\bundle\nsis\Dropply_%APP_VERSION%_x64_EN-FR-setup.exe" "src-tauri\target\release\bundle\github-release\" >nul
if errorlevel 1 exit /b %errorlevel%
copy /y "src-tauri\target\release\bundle\cli\Dropply_CLI_%APP_VERSION%_Windows_x64_EN-FR.zip" "src-tauri\target\release\bundle\github-release\" >nul
if errorlevel 1 exit /b %errorlevel%
copy /y "src-tauri\target\release\bundle\Dropply_BrowserShare_%APP_VERSION%_Extension.zip" "src-tauri\target\release\bundle\github-release\" >nul
if errorlevel 1 exit /b %errorlevel%
copy /y "src-tauri\target\release\bundle\Dropply_%APP_VERSION%_x64_EN-FR_SHA256.txt" "src-tauri\target\release\bundle\github-release\" >nul
if errorlevel 1 exit /b %errorlevel%
