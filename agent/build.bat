@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "SRC=%SCRIPT_DIR%\src\main\java"
set "OUT=%SCRIPT_DIR%\build\classes"
set "JAR=%SCRIPT_DIR%\build\weaverift-agent.jar"

where javac >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERROR] javac not found
    pause
    exit /b 1
)

if exist "%SCRIPT_DIR%\build" rmdir /S /Q "%SCRIPT_DIR%\build"
mkdir "%OUT%"

echo [1/2] Compiling...
javac -encoding UTF-8 -d "%OUT%" ^
    "%SRC%\com\fastnow\weaverift\WeaveRiftAgent.java" ^
    "%SRC%\com\fastnow\weaverift\ClassLoaderUtil.java" ^
    "%SRC%\com\fastnow\weaverift\RiftBridge.java" ^
    "%SRC%\com\fastnow\weaverift\NativeBridge.java"

if %errorlevel% neq 0 (
    echo [ERROR] javac failed
    pause
    exit /b 1
)

echo [2/2] Packaging...
jar cfm "%JAR%" "%SCRIPT_DIR%\MANIFEST.MF" -C "%OUT%" .

if %errorlevel% neq 0 (
    echo [ERROR] jar failed
    pause
    exit /b 1
)

echo ✅ Done: %JAR%
dir "%JAR%"