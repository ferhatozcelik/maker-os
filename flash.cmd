@echo off
rem MakerOS microSD yazici (Windows): cift tiklayin. Ayni klasordeki flash.ps1'i, yoksa GitHub'daki surumu calistirir.
rem MakerOS microSD flasher (Windows): double-click. Runs flash.ps1 next to this file, or the one on GitHub.
net session >nul 2>&1
if errorlevel 1 (
  powershell.exe -NoProfile -Command "Start-Process -Verb RunAs -FilePath '%~f0'"
  exit /b
)
if exist "%~dp0flash.ps1" (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "iex ([IO.File]::ReadAllText('%~dp0flash.ps1', [Text.Encoding]::UTF8))"
) else (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.ps1 | iex"
)
