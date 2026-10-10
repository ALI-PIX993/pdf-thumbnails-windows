@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0thumbnails.ps1" -Action Disable
pause
