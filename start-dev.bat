@echo off
setlocal EnableDelayedExpansion
echo Starting Sprite Assistant Development Environment...
echo.

echo [1/4] Building shared schemas...
call pnpm -C packages/shared build
if %errorlevel% neq 0 (
    echo Failed to build shared schemas
    exit /b 1
)

echo [2/4] Building Python voice service...
call pnpm run build:voice
if %errorlevel% neq 0 (
    echo Failed to build voice service
    exit /b 1
)

echo [3/4] Starting worker in background...
start "Worker" cmd /c "pnpm -C apps/worker dev"
timeout /t 3 /nobreak >nul

echo [4/4] Starting Tauri desktop app...
call pnpm -C apps/tauri tauri:dev
