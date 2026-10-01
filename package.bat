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
if %errorlevel% neq 0 (
    echo [ERROR] cargo not found. Please install Rust and add it to PATH.
    pause
    exit /b 1
)
where javac >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERROR] javac not found. Please install JDK 17+ and add it to PATH.
    pause
    exit /b 1
)

echo Toolchain Info:
cargo --version
javac -version
echo.

:: ©¤©¤©¤ Clean old release ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [1/6] Cleaning old release directory...
if exist "%RELEASE_DIR%" rmdir /S /Q "%RELEASE_DIR%"
mkdir "%WR_SUB%"
echo [OK] Cleaned
echo.

:: ©¤©¤©¤ Build WeaveRift.exe (main injector) ©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [2/6] Building WeaveRift.exe...
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

:: ©¤©¤©¤ Build Agent ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [3/6] Building weaverift-agent.jar...
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
echo [4/6] Building jar_loader.dll...
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
echo [5/6] Copying console.html...
if exist "%ROOT_DIR%\console.html" (
    copy /Y "%ROOT_DIR%\console.html" "%WR_SUB%\console.html" >nul
    echo [OK] console.html deployed
) else if exist "%ROOT_DIR%\jar_loader\console.html" (
    copy /Y "%ROOT_DIR%\jar_loader\console.html" "%WR_SUB%\console.html" >nul
    echo [OK] console.html deployed (from jar_loader/)
) else (
    echo [WARN] console.html not found. Debug console will use built-in version.
)
echo.

:: ©¤©¤©¤ Check runtime dependencies ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo [6/6] Checking runtime dependencies...
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

:: ©¤©¤©¤ Usage Tips ©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤©¤
echo ========================================
echo  Usage Tips
echo ========================================
echo.
echo 1. Add the following JVM argument in PCL/HMCL:
echo      -Djdk.attach.allowAttachSelf=true
echo.
echo 2. Launch the game, then use release\WeaveRift.exe
echo    to inject jar_loader.dll
echo.
echo 3. Inject weaverift-agent.jar via Attach API,
echo    with argument: srg=%WR_SUB%\obf2srg.srg
echo.
echo 4. Debug console URL is written to:
echo      %%TEMP%%\WeaveRift\debug-url.txt
echo.
pause