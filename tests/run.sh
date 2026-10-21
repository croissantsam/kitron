#!/usr/bin/env bash
# Kitron test suite (spec §41 — the MVP vertical slice must be provable).
#
#   bash tests/run.sh            # fast checks: typecheck, rust tests, node host protocol
#   KITRON_E2E_MACOS=1 bash tests/run.sh   # + macOS end-to-end (pod install, xcodebuild, launch)
#
# Every step is independent: a failure stops the run and prints the reason.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FAILED=0

step() {
  printf '\n\033[36m▶ %s\033[0m\n' "$1"
}

fail() {
  printf '\033[31m✗ %s\033[0m\n' "$1"
  FAILED=1
}

step "typecheck (TypeScript SDK + CLI)"
if npx tsc --noEmit; then
  echo "  typecheck: PASS"
else
  fail "typecheck failed"
fi

step "rust tests (kitron-protocol / kitron-core / node supervisor)"
if KITRON_NODE_PATH="${KITRON_NODE_PATH:-$(command -v node)}" \
  cargo test --manifest-path runtime/rust/Cargo.toml --quiet; then
  echo "  rust: PASS"
else
  fail "cargo test failed"
fi

step "node host protocol smoke (RN ↔ Node transport)"
if node runtime/node/smoke.mjs; then
  echo "  node host: PASS"
else
  fail "node host smoke failed"
fi

step "sdk smoke (typed errors + namespaces outside a native runtime)"
if node --experimental-strip-types --disable-warning=ExperimentalWarning tests/sdk-smoke.mjs; then
  echo "  sdk: PASS"
else
  fail "sdk smoke failed"
fi

step "CLI e2e: kitron create scaffolds a runnable app"
SCAFFOLD_ROOT="$(mktemp -d)"
SCAFFOLD_APP="$SCAFFOLD_ROOT/scaffolded-app"
if node cli/src/kitron.ts create "$SCAFFOLD_APP" \
    --name ScaffoldedApp \
    --identifier dev.kitron.app.scaffolded \
    --skip-install \
    --skip-macos-setup >/dev/null 2>&1; then
  if [ -f "$SCAFFOLD_APP/kitron.config.ts" ] \
    && [ -f "$SCAFFOLD_APP/App.tsx" ] \
    && [ -f "$SCAFFOLD_APP/backend/index.ts" ] \
    && [ -f "$SCAFFOLD_APP/node_modules/kitron/dist/index.js" ] \
    && [ -f "$SCAFFOLD_APP/.kitron/host.mjs" ] \
    && grep -q "ScaffoldedApp" "$SCAFFOLD_APP/package.json"; then
    echo "  create: PASS ($SCAFFOLD_APP)"
  else
    fail "kitron create produced an incomplete scaffold"
  fi
else
  fail "kitron create failed"
fi
rm -rf "$SCAFFOLD_ROOT"

if [ "${KITRON_E2E_MACOS:-0}" = "1" ]; then
  step "macOS e2e: pod install + xcodebuild + launch (slow)"
  if bash tests/e2e-macos.sh; then
    echo "  macOS e2e: PASS"
  else
    fail "macOS e2e failed"
  fi
else
  printf '\n\033[33m• macOS e2e skipped (set KITRON_E2E_MACOS=1 to run it)\033[0m\n'
fi

if [ "$FAILED" -ne 0 ]; then
  printf '\n\033[31mFAILED\033[0m\n'
  exit 1
fi
printf '\n\033[32mALL TESTS PASSED\033[0m\n'
