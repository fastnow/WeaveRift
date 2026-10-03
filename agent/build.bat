@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo  weaverift-agent 构建脚本
echo ========================================
echo.

set "SCRIPT_DIR=%~dp0"
set "SRC=%SCRIPT_DIR%\src\main\java"
set "RES=%SCRIPT_DIR%\src\main\resources"
set "OUT=%SCRIPT_DIR%\build\classes"
set "JAR=%SCRIPT_DIR%\build\weaverift-agent.jar"
set "MANIFEST=%SCRIPT_DIR%\MANIFEST.MF"

set "PROJECT_ROOT=%SCRIPT_DIR%\.."
set "TARGET_DIR=%PROJECT_ROOT%\release\WeaveRift"

where javac >nul 2>nul
if %errorlevel% neq 0 (
    echo [错误] 未找到 javac，请安装 JDK 17+ 并添加到 PATH。
    pause
    exit /b 1
)

javac -version
echo.

if exist "%SCRIPT_DIR%\build" rmdir /S /Q "%SCRIPT_DIR%\build"
mkdir "%OUT%"

if not exist "%MANIFEST%" (
    (
        echo Manifest-Version: 1.0
        echo Agent-Class: com.fastnow.weaverift.WeaveRiftAgent
        echo Can-Redefine-Classes: false
        echo Can-Retransform-Classes: false
    ) > "%MANIFEST%"
)

echo [1/3] 编译 Java 源文件...
javac -encoding UTF-8 -d "%OUT%" ^
    "%SRC%\com\fastnow\weaverift\WeaveRiftAgent.java" ^
    "%SRC%\com\fastnow\weaverift\ClassLoaderUtil.java" ^
    "%SRC%\com\fastnow\weaverift\RiftBridge.java" ^
    "%SRC%\com\fastnow\weaverift\NativeBridge.java" ^
    "%SRC%\com\fastnow\weaverift\VersionProbe.java" ^
    "%SRC%\com\fastnow\weaverift\VersionConfig.java" ^
    "%SRC%\com\fastnow\weaverift\NamespaceResolver.java" ^
    "%SRC%\com\fastnow\weaverift\ClassCache.java" ^
    "%SRC%\com\fastnow\weaverift\FieldCache.java" ^
    "%SRC%\com\fastnow\weaverift\MethodCache.java"

if %errorlevel% neq 0 (
    echo [错误] javac 失败！
    pause
    exit /b 1
)

echo [2/3] 打包 JAR（含 resources）...
jar cfm "%JAR%" "%MANIFEST%" -C "%OUT%" . -C "%RES%" .

if %errorlevel% neq 0 (
    echo [错误] jar 打包失败！
    pause
    exit /b 1
)

echo [3/3] 复制到 release 目录...
if not exist "%TARGET_DIR%" (
    echo 创建目录：%TARGET_DIR%
    mkdir "%TARGET_DIR%"
)

copy /Y "%JAR%" "%TARGET_DIR%\weaverift-agent.jar" >nul

if %errorlevel% equ 0 (
    echo 已部署到：%TARGET_DIR%\weaverift-agent.jar
) else (
    echo 复制失败，请检查目标目录权限。
    pause
    exit /b 1
)

echo.
echo ========================================
echo  构建成功！
echo  JAR: %JAR%
echo  部署: %TARGET_DIR%\weaverift-agent.jar
echo ========================================
dir "%JAR%"
echo.