@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo  WeaveRift Full Package Build
echo ========================================
echo.

set "ROOT_DIR=%~dp0"
set "ROOT_DIR=%ROOT_DIR:~0,-1%"
set "RELEASE_DIR=%ROOT_DIR%\release"
set "WR_SUB=%RELEASE_DIR%\WeaveRift"

echo [1/6] Cleaning old release...
if exist "%RELEASE_DIR%" rmdir /S /Q "%RELEASE_DIR%"
mkdir "%WR_SUB%"

echo [2/6] Building WeaveRift.exe...
cd /d "%ROOT_DIR%"
cargo build --release
if errorlevel 1 goto :fail
copy /Y "%ROOT_DIR%\target\release\WeaveRift.exe" "%RELEASE_DIR%\" >nul

echo [3/6] Building jar_loader.dll...
cd /d "%ROOT_DIR%\jar_loader"
cargo build --release
if errorlevel 1 goto :fail
copy /Y "%ROOT_DIR%\jar_loader\target\release\jar_loader.dll" "%WR_SUB%\" >nul

echo [4/6] Building weaverift-agent.jar...
cd /d "%ROOT_DIR%\agent"
call build.bat
if errorlevel 1 goto :fail
copy /Y "%ROOT_DIR%\agent\build\weaverift-agent-1.0.0.jar" "%WR_SUB%\" >nul

echo [5/6] Checking runtime dependency (obf2srg.srg)...
if exist "%ROOT_DIR%\obf2srg.srg" (
    copy /Y "%ROOT_DIR%\obf2srg.srg" "%WR_SUB%\" >nul
    echo [OK] obf2srg.srg copied from project root
) else if exist "%WR_SUB%\obf2srg.srg" (
    echo [OK] obf2srg.srg already present in release
) else (
    echo [WARN] obf2srg.srg not found!
    echo [WARN] Please download it from:
    echo [WARN]   https://github.com/kettingpowered/MinecraftMappings
    echo [WARN] and place it at: %WR_SUB%\obf2srg.srg
)

echo [6/6] Done!
cd /d "%ROOT_DIR%"
echo.
echo ========================================
echo   Build Complete
echo   Output: %RELEASE_DIR%
echo ========================================
echo.
echo [release/]
dir /B "%RELEASE_DIR%"
echo.
echo [release/WeaveRift/]
dir /B "%WR_SUB%"
pause
exit /b 0

:fail
echo.
echo [ERROR] Build failed.
cd /d "%ROOT_DIR%"
pause
exit /b 1