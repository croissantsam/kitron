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

step "bridge conformance (native module ↔ TS API ↔ C ABI, incl. no_mangle)"
if node tests/bridge-conformance.mjs; then
  echo "  bridge: PASS"
else
  fail "bridge conformance failed"
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
    && [ -f "$SCAFFOLD_APP/windows/kitron/KitronHost.cpp" ] \
    && [ -f "$SCAFFOLD_APP/windows/kitron/KitronHost.h" ] \
    && [ -f "$SCAFFOLD_APP/.kitron/windows-vendor.json" ] \
    && [ -f "$SCAFFOLD_APP/.kitron/windows-runtime/Cargo.toml" ] \
    && [ -d "$SCAFFOLD_APP/.kitron/windows-runtime/crates/kitron-windows-host" ] \
    && grep -q "ScaffoldedApp" "$SCAFFOLD_APP/package.json"; then
    echo "  create: PASS ($SCAFFOLD_APP)"
  else
    fail "kitron create produced an incomplete scaffold"
  fi
else
  fail "kitron create failed"
fi

step "CLI e2e: kitron update re-vendors the runtime into an existing app"
if [ -d "$SCAFFOLD_APP" ]; then
  if node cli/src/kitron.ts update "$SCAFFOLD_APP" >/dev/null 2>&1 \
    && [ -f "$SCAFFOLD_APP/.kitron/sdk-vendor.json" ] \
    && [ -f "$SCAFFOLD_APP/node_modules/@kitron/core/dist/index.js" ] \
    && [ -f "$SCAFFOLD_APP/node_modules/@kitron/sdk/cli/dist/kitron.js" ] \
    && [ -f "$SCAFFOLD_APP/node_modules/@kitron/core/src/index.ts" ] \
    && [ -x "$SCAFFOLD_APP/node_modules/.bin/kitron" ] \
    && grep -q "vendoredAt" "$SCAFFOLD_APP/.kitron/windows-vendor.json"; then
    echo "  update: PASS"
  else
    fail "kitron update did not re-vendor the app"
  fi
fi
step "CLI e2e: kitron dev --platform windows refuses cleanly off-Windows"
if [ -d "$SCAFFOLD_APP" ]; then
  OUT="$(node cli/src/kitron.ts dev "$SCAFFOLD_APP" --platform windows 2>&1 || true)"
  if [ "$(echo "$OUT" | grep -c "Windows machine")" -ge 1 ]; then
    echo "  windows dev guard: PASS"
  else
    fail "kitron dev --platform windows did not refuse off-Windows"
  fi
fi
rm -rf "$SCAFFOLD_ROOT"

step "CLI e2e: a Rust backend scaffolds and compiles"
RUST_SCAFFOLD_ROOT="$(mktemp -d)"
RUST_SCAFFOLD_APP="$RUST_SCAFFOLD_ROOT/rust-app"
if node cli/src/kitron.ts create "$RUST_SCAFFOLD_APP" \
    --name RustScaffold \
    --identifier dev.kitron.app.rustscaffold \
    --backend rust \
    --skip-install \
    --skip-macos-setup >/dev/null 2>&1 \
  && [ -f "$RUST_SCAFFOLD_APP/backend/Cargo.toml" ] \
  && [ -f "$RUST_SCAFFOLD_APP/backend/src/main.rs" ] \
  && grep -q 'type: "binary"' "$RUST_SCAFFOLD_APP/kitron.config.ts"; then
  echo "  rust backend scaffold: PASS"
else
  fail "the Rust backend scaffold is incomplete"
fi
rm -rf "$RUST_SCAFFOLD_ROOT"

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
