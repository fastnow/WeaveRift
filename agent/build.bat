@echo off
setlocal

set SRC=src\main\java
set OUT=build\classes
set JAR=build\weaverift-agent-1.0.0.jar
set LIBS=libs

if exist build rmdir /S /Q build
mkdir build\classes
mkdir build\libs_extract

echo [1/4] Compiling...
javac -encoding UTF-8 -cp "%LIBS%\asm-9.7.jar;%LIBS%\asm-commons-9.7.jar;%LIBS%\lwjgl-2.9.3.jar" -d %OUT% ^
    %SRC%\com\fastnow\weaverift\WeaveRiftAgent.java ^
    %SRC%\com\fastnow\weaverift\NativeBridge.java ^
    %SRC%\com\fastnow\weaverift\render\RiftRender.java

if errorlevel 1 (
    echo [ERROR] javac failed
    pause
    exit /b 1
)

echo [2/4] Extracting ASM...
cd /d build\libs_extract
jar xf "..\..\%LIBS%\asm-9.7.jar"
jar xf "..\..\%LIBS%\asm-commons-9.7.jar"
cd /d "..\.."

echo [3/4] Packaging JAR...
jar cfm %JAR% manifest.txt -C %OUT% . -C build\libs_extract .

if errorlevel 1 (
    echo [ERROR] jar failed
    pause
    exit /b 1
)

echo [4/4] Done: %JAR%
dir %JAR%
pause