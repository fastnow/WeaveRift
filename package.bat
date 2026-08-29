@echo off
setlocal enabledelayedexpansion
title WeaveRift Build
echo ================================================
echo    WeaveRift - Build and Package Tool
echo ================================================
echo.
set SCRIPT_DIR=%~dp0
cd /d "%SCRIPT_DIR%"

where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found
    pause
    exit /b 1
)
echo [OK] Rust installed

echo.
echo [1/4] Cleaning old build...
if exist "%SCRIPT_DIR%release" rmdir /S /Q "%SCRIPT_DIR%release"
echo [OK]

echo.
echo [2/4] Building main project (Release)...
cargo build --release
if errorlevel 1 (
    echo [ERROR] Build failed
    pause
    exit /b 1
)
echo [OK]

echo.
echo [3/4] Packaging main executable...
mkdir "%SCRIPT_DIR%release\WeaveRift" 2>nul
copy "%SCRIPT_DIR%target\release\WeaveRift.exe" "%SCRIPT_DIR%release\" >nul
if exist "%SCRIPT_DIR%icon.ico" copy "%SCRIPT_DIR%icon.ico" "%SCRIPT_DIR%release\" >nul
echo [OK]

echo.
echo [4/4] Building overlay core + jar_loader...
echo      (core.dll - WeaveRift overlay animation)
cd /d "%SCRIPT_DIR%\WeaveRift"
cargo build --release
if errorlevel 1 (
    echo [ERROR] Core build failed
    pause
    exit /b 1
)
copy "%SCRIPT_DIR%\WeaveRift\target\release\core.dll" "%SCRIPT_DIR%\release\WeaveRift\" >nul
echo [OK]

echo      (jar_loader.dll - JNI jar loader, stealth)
cd /d "%SCRIPT_DIR%\jar_loader"
cargo build --release
if errorlevel 1 (
    echo [ERROR] jar_loader build failed
    pause
    exit /b 1
)
copy "%SCRIPT_DIR%\jar_loader\target\release\jar_loader.dll" "%SCRIPT_DIR%\release\WeaveRift\" >nul
cd /d "%SCRIPT_DIR%"
echo [OK]

echo.
echo ================================================
echo    Build Complete!
echo ================================================
echo.
echo Output folder: %SCRIPT_DIR%release
dir /B "%SCRIPT_DIR%release"
echo.
pause
exit /b 0