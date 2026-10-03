@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo  WeaveRift One-Click Build
echo ========================================
echo.

set "ROOT_DIR=%~dp0"
set "ROOT_DIR=%ROOT_DIR:~0,-1%"
set "RELEASE_DIR=%ROOT_DIR%\release"
set "WR_SUB=%RELEASE_DIR%\WeaveRift"

:: ©¤©¤©¤ Check toolchain ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found. Please install Rust and add it to PATH.
    pause
    exit /b 1
)
where javac >nul 2>nul
if errorlevel 1 (
    echo [ERROR] javac not found. Please install JDK 17+ and add it to PATH.
    pause
    exit /b 1
)

echo Toolchain Info:
cargo --version
javac -version 2>&1
echo.

:: ©¤©¤©¤ Clean old release ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [1/7] Cleaning old release directory...
if exist "%RELEASE_DIR%" (
    rmdir /S /Q "%RELEASE_DIR%"
    if exist "%RELEASE_DIR%" (
        echo [ERROR] Cannot delete old release dir.
        echo         Close any program using it ^(Explorer, WeaveRift.exe, etc.^) and retry.
        pause
        exit /b 1
    )
)
mkdir "%WR_SUB%"
echo [OK] Cleaned
echo.

:: ©¤©¤©¤ Build WeaveRift.exe (main injector) ©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [2/7] Building WeaveRift.exe...
cd /d "%ROOT_DIR%"
cargo build --release
if errorlevel 1 (
    echo [ERROR] WeaveRift build failed
    pause
    exit /b 1
)

set "EXE_SRC=%ROOT_DIR%\target\release\WeaveRift.exe"
if not exist "%EXE_SRC%" (
    echo [ERROR] Cannot find %EXE_SRC%
    pause
    exit /b 1
)
copy /Y "%EXE_SRC%" "%RELEASE_DIR%\WeaveRift.exe" >nul
echo [OK] WeaveRift.exe deployed
echo.

:: ©¤©¤©¤ client.dll ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [3/7] Building client.dll...
cd /d "%ROOT_DIR%\client"
cargo build --release
if errorlevel 1 (
    echo [ERROR] client build failed
    pause
    exit /b 1
)

set "CLIENT_SRC=%ROOT_DIR%\client\target\release\client.dll"
if not exist "%CLIENT_SRC%" (
    echo [ERROR] Cannot find %CLIENT_SRC%
    pause
    exit /b 1
)
copy /Y "%CLIENT_SRC%" "%WR_SUB%\client.dll" >nul
echo [OK] client.dll deployed
echo.

:: ©¤©¤©¤ Build Agent ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [4/7] Building weaverift-agent.jar...
cd /d "%ROOT_DIR%\agent"
call build.bat
if errorlevel 1 (
    echo [ERROR] Agent build failed
    pause
    exit /b 1
)

set "AGENT_JAR=%ROOT_DIR%\agent\build\weaverift-agent.jar"
if not exist "%AGENT_JAR%" (
    echo [ERROR] Cannot find %AGENT_JAR%
    pause
    exit /b 1
)
copy /Y "%AGENT_JAR%" "%WR_SUB%\weaverift-agent.jar" >nul
echo [OK] Agent deployed
echo.

:: ©¤©¤©¤ Build jar_loader ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [5/7] Building jar_loader.dll...
cd /d "%ROOT_DIR%\jar_loader"
cargo build --release
if errorlevel 1 (
    echo [ERROR] jar_loader build failed
    pause
    exit /b 1
)

set "DLL_SRC=%ROOT_DIR%\jar_loader\target\release\jar_loader.dll"
if not exist "%DLL_SRC%" (
    echo [ERROR] Cannot find %DLL_SRC%
    pause
    exit /b 1
)
copy /Y "%DLL_SRC%" "%WR_SUB%\jar_loader.dll" >nul
echo [OK] DLL deployed
echo.

:: ©¤©¤©¤ Copy console.html ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [6/7] Copying console.html...
if exist "%ROOT_DIR%\console.html" (
    copy /Y "%ROOT_DIR%\console.html" "%WR_SUB%\console.html" >nul
    echo [OK] console.html deployed
) else if exist "%ROOT_DIR%\jar_loader\console.html" (
    copy /Y "%ROOT_DIR%\jar_loader\console.html" "%WR_SUB%\console.html" >nul
    echo [OK] console.html deployed ^(from jar_loader/^)
) else (
    echo [WARN] console.html not found. Debug console will use built-in version.
)
echo.

:: ©¤©¤©¤ Check runtime dependencies ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [7/7] Checking runtime dependencies...
if exist "%ROOT_DIR%\obf2srg.srg" (
    copy /Y "%ROOT_DIR%\obf2srg.srg" "%WR_SUB%\obf2srg.srg" >nul
    echo [OK] obf2srg.srg deployed
) else if exist "%WR_SUB%\obf2srg.srg" (
    echo [OK] obf2srg.srg already exists
) else (
    echo [WARN] obf2srg.srg not found!
    echo    Please download the corresponding version of obf2srg.srg from:
    echo    https://github.com/kettingpowered/MinecraftMappings
    echo    and place it at:
    echo    %WR_SUB%\obf2srg.srg
    echo    After placing it, re-run this script or copy it manually.
)
echo.

:: ©¤©¤©¤ Done ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo ========================================
echo  All done!
echo ========================================
echo.
echo Output directory: %RELEASE_DIR%
echo.
echo [release/]
dir /B "%RELEASE_DIR%"
echo.
echo [release/WeaveRift/]
dir /B "%WR_SUB%"
echo.
pause