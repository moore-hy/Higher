@echo off
chcp 65001 >nul
title Higher Dev

rem DEV-0065.3 §9：仓库根从脚本位置相对解析（scripts\ 的上一级），
rem 不再硬编码 C:\Users\37653\Desktop\Higher。
set "ROOT=%~dp0.."
for %%i in ("%ROOT%") do set "ROOT=%%~fi"

echo Higher dev environment starting...
echo Repo root: %ROOT%
echo.

cd /d "%ROOT%"

set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where npm >nul 2>nul
if errorlevel 1 (
    echo [错误] 未找到 npm，请检查 Node.js 环境。
    pause
    exit /b 1
)

where cargo >nul 2>nul
if errorlevel 1 (
    echo [错误] 未找到 cargo，请检查 Rust 环境。
    pause
    exit /b 1
)

call npm run tauri dev

echo.
echo Higher 已停止或启动失败。
pause
