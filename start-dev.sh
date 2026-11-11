#!/bin/bash
echo "Starting Sprite Assistant Development Environment..."
echo

echo "[1/4] Building shared schemas..."
pnpm -C packages/shared build
if [ $? -ne 0 ]; then
    echo "Failed to build shared schemas"
    exit 1
fi

echo "[2/4] Building Python voice service..."
pnpm run build:voice
if [ $? -ne 0 ]; then
    echo "Failed to build voice service"
    exit 1
fi

# Cleanup worker on exit
cleanup() {
  if [ -n "$WORKER_PID" ] && kill -0 "$WORKER_PID" 2>/dev/null; then
    kill "$WORKER_PID"
  fi
}
trap cleanup EXIT

echo "[3/4] Starting worker in background..."
pnpm -C apps/worker dev &
WORKER_PID=$!
sleep 3

echo "[4/4] Starting Tauri desktop app..."
pnpm -C apps/tauri tauri:dev
