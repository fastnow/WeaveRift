@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo  client build and deploy script
echo ========================================
echo.

set "SCRIPT_DIR=%~dp0"
set "SCRIPT_DIR=%SCRIPT_DIR:~0,-1%"
set "PROJECT_ROOT=%SCRIPT_DIR%\.."
set "TARGET_DIR=%PROJECT_ROOT%\release\WeaveRift"

where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERROR] cargo not found.
    pause
    exit /b 1
)

echo cargo:
cargo --version
echo.

cd /d "%SCRIPT_DIR%"
echo Current directory: %cd%
echo Building release...
cargo build --release

if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Build failed.
    pause
    exit /b %errorlevel%
)

set "DLL_SRC=%SCRIPT_DIR%\target\release\client.dll"

if not exist "%DLL_SRC%" (
    echo [ERROR] Cannot find %DLL_SRC%
    pause
    exit /b 1
)

if not exist "%TARGET_DIR%" mkdir "%TARGET_DIR%"

copy /Y "%DLL_SRC%" "%TARGET_DIR%\client.dll" >nul
if %errorlevel% equ 0 (
    echo ✅ client.dll deployed to %TARGET_DIR%
) else (
    echo ❌ Copy failed
    pause
    exit /b 1
)

echo.
dir /B "%TARGET_DIR%"
echo.