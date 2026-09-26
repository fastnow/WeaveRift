@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ========================================
echo  jar_loader 构建并部署脚本
echo ========================================
echo.

set "ROOT_DIR=%~dp0"
set "ROOT_DIR=%ROOT_DIR:~0,-1%"
set "TARGET_DIR=%ROOT_DIR%\..\release\WeaveRift"

where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [错误] 未找到 cargo，请安装 Rust 并添加到 PATH。
    pause
    exit /b 1
)

echo Rust 工具链信息：
cargo --version
echo.

cd /d "%ROOT_DIR%"
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

cd /d "%ROOT_DIR%\.."

set "DLL_SRC=%ROOT_DIR%\target\release\jar_loader.dll"
set "DLL_DST=%TARGET_DIR%\jar_loader.dll"

if not exist "%DLL_SRC%" (
    echo.
    echo [警告] 找不到生成的 DLL：%DLL_SRC%
    pause
    exit /b 1
)

echo.
echo ========================================
echo  构建成功！
echo  源文件：%DLL_SRC%
echo ========================================

if not exist "%TARGET_DIR%" (
    echo 创建目录：%TARGET_DIR%
    mkdir "%TARGET_DIR%"
)

echo 正在复制 DLL 到：%DLL_DST%
copy /Y "%DLL_SRC%" "%DLL_DST%"

if %errorlevel% equ 0 (
    echo 已成功部署到：%DLL_DST%
) else (
    echo [错误] 复制失败，请检查目标目录权限。
)

pause