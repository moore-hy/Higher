@echo off
chcp 65001 >nul
title Higher 测试环境

echo Higher 测试环境启动中...
echo 项目目录：C:\Users\37653\Desktop\Higher
echo.

cd /d "C:\Users\37653\Desktop\Higher"

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

npm run tauri dev

echo.
echo Higher 已停止或启动失败。
pause
