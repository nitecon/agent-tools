@echo off
setlocal

cargo build --release
if errorlevel 1 exit /b 1

if "%~1"=="" goto :done

if not exist "%~1" mkdir "%~1"
copy /y "target\release\claude-tools.exe" "%~1\" >nul
copy /y "target\release\claude-tools-mcp.exe" "%~1\" >nul
echo Copied binaries to %~1

:done
