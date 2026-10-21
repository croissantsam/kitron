#!/usr/bin/env bash
# macOS end-to-end gate (spec §41 items 1-10).
#
#   bash tests/e2e-macos.sh
#
# 1. `kitron create` a fresh app (installs deps, generates the Xcode project,
#    runs `pod install` — slow, several minutes).
# 2. `kitron build` → Release .app with the JS bundle, the backend and the Node
#    runtime embedded.
# 3. Launch the packaged app with a *clean environment* (no KITRON_* vars, no
#    Metro) and assert the runtime chain in its log:
#       ui mounted → app.info → app.ready → backend.invoke result {sum:42}
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
APP_DIR="$WORK/E2EApp"
LOG="$WORK/app.log"
KITRON="$ROOT/cli/src/kitron.ts"

cleanup() {
  if [ -n "${APP_PID:-}" ] && kill -0 "$APP_PID" 2>/dev/null; then
    kill "$APP_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "→ kitron create $APP_DIR"
if ! node "$KITRON" create "$APP_DIR" \
    --name E2EApp \
    --identifier dev.kitron.app.e2e; then
  echo "✗ kitron create failed" >&2
  exit 1
fi

echo "→ kitron build (Release + embedded assets)"
if ! node "$KITRON" build "$APP_DIR"; then
  echo "✗ kitron build failed" >&2
  exit 1
fi

APP_PATH="$(find "$APP_DIR/macos/build/derived/Build/Products/Release" -maxdepth 1 -name '*.app' | head -1)"
if [ -z "$APP_PATH" ]; then
  echo "✗ no .app produced" >&2
  exit 1
fi
echo "  app: $APP_PATH"

if [ ! -f "$APP_PATH/Contents/Resources/main.jsbundle" ] \
  || [ ! -d "$APP_PATH/Contents/Resources/kitron/backend" ] \
  || [ ! -x "$APP_PATH/Contents/Resources/kitron/node/bin/node" ]; then
  echo "✗ the packaged app is missing embedded runtime assets" >&2
  exit 1
fi

echo "→ launching the packaged app (clean env, no Metro)"
(
  cd /tmp
  env -i \
    HOME="$HOME" \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    "$APP_PATH/Contents/MacOS/E2EApp" > "$LOG" 2>&1
) &
APP_PID=$!

DEADLINE=$((SECONDS + 90))
while [ "$SECONDS" -lt "$DEADLINE" ]; do
  if grep -q "backend.invoke result" "$LOG" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    break
  fi
  sleep 1
done

kill "$APP_PID" 2>/dev/null || true
sleep 1

echo "--- app log ---"
grep "\[kitron" "$LOG" | sed 's/^/  /' | head -40
echo "----------------"

status=0
assert_log() {
  if grep -q "$1" "$LOG"; then
    echo "  ✓ $1"
  else
    echo "  ✗ missing: $1" >&2
    status=1
  fi
}

assert_log "ui mounted"
assert_log "app.info"
assert_log "app.ready received"
assert_log "backend.invoke result"

if [ "$status" -ne 0 ]; then
  echo "✗ the packaged app did not complete the runtime chain" >&2
  exit 1
fi

echo "✓ macOS e2e passed"
