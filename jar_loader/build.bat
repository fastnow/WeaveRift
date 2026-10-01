@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo  jar_loader 构建并部署脚本
echo ========================================
echo.

set "SCRIPT_DIR=%~dp0"
set "SCRIPT_DIR=%SCRIPT_DIR:~0,-1%"
set "PROJECT_ROOT=%SCRIPT_DIR%\.."
set "TARGET_DIR=%PROJECT_ROOT%\release\WeaveRift"

:: ─── 检查 cargo ────────────────────────────────
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [错误] 未找到 cargo，请安装 Rust 并添加到 PATH。
    pause
    exit /b 1
)

echo Rust 工具链信息：
cargo --version
echo.

:: ─── 检查源码文件 ──────────────────────────────
set "MISSING=0"
if not exist "%SCRIPT_DIR%\src\lib.rs" (
    echo [错误] 找不到 src\lib.rs
    set "MISSING=1"
)
if not exist "%SCRIPT_DIR%\src\module_hide.rs" (
    echo [警告] 找不到 src\module_hide.rs，PEB 断链将不可用
)

if "%MISSING%"=="1" (
    pause
    exit /b 1
)

:: ─── 构建 ─────────────────────────────────────
cd /d "%SCRIPT_DIR%"
echo 当前目录：%cd%
echo.
echo 开始构建 release 版本...
cargo build --release

if %errorlevel% neq 0 (
    echo.
    echo [错误] 构建失败！请检查上方错误信息。
    pause
    exit /b %errorlevel%
)

:: ─── 定位产物 ──────────────────────────────────
cd /d "%PROJECT_ROOT%"

set "DLL_SRC=%SCRIPT_DIR%\target\release\jar_loader.dll"
set "DLL_DST=%TARGET_DIR%\jar_loader.dll"

if not exist "%DLL_SRC%" (
    echo.
    echo [警告] 找不到生成的 DLL：%DLL_SRC%
    pause
    exit /b 1
)

:: ─── 确保目标目录存在 ──────────────────────────
if not exist "%TARGET_DIR%" (
    echo 创建目录：%TARGET_DIR%
    mkdir "%TARGET_DIR%"
)

:: ─── 复制 DLL ─────────────────────────────────
echo.
echo ========================================
echo  构建成功！
echo  源文件：%DLL_SRC%
echo  目标：%DLL_DST%
echo ========================================
echo.
echo 正在复制 DLL...
copy /Y "%DLL_SRC%" "%DLL_DST%" >nul

if %errorlevel% equ 0 (
    echo ✅ DLL 已部署
) else (
    echo ❌ DLL 复制失败
    pause
    exit /b 1
)

:: ─── 复制 console.html ─────────────────────────
set "HTML_SRC=%SCRIPT_DIR%\console.html"
set "HTML_DST=%TARGET_DIR%\console.html"
if exist "%HTML_SRC%" (
    copy /Y "%HTML_SRC%" "%HTML_DST%" >nul
    echo ✅ console.html 已部署
)

:: ─── 复制 Agent JAR ───────────────────────────
set "AGENT_SRC=%PROJECT_ROOT%\agent\build\weaverift-agent.jar"
if exist "%AGENT_SRC%" (
    copy /Y "%AGENT_SRC%" "%TARGET_DIR%\weaverift-agent.jar" >nul
    echo ✅ Agent JAR 已部署
) else (
    echo ⚠️  未找到 Agent JAR，请先运行 agent\build.bat
)

:: ─── 完成 ─────────────────────────────────────
echo.
echo ========================================
echo  全部完成！
echo ========================================
echo.
echo 部署目录：%TARGET_DIR%
dir /B "%TARGET_DIR%"
echo.