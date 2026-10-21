#!/usr/bin/env bash
# Materialises the example into a runnable Kitron app.
#
#   bash examples/hello-kitron/scaffold.sh [target-dir]
#
# The example ships sources only (no generated Xcode project, no Pods), so the
# script asks the CLI to scaffold a real app and then overlays the example's
# UI/backend/config on top of the template.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXAMPLE="$ROOT/examples/hello-kitron"
TARGET="${1:-$ROOT/examples/.hello-kitron-app}"

if [ -e "$TARGET/package.json" ]; then
  echo "target already scaffolded: $TARGET" >&2
  echo "remove it first (rm -rf \"$TARGET\")" >&2
  exit 1
fi

echo "→ scaffolding $TARGET"
node "$ROOT/cli/src/kitron.ts" create "$TARGET" \
  --name HelloKitronExample \
  --identifier dev.croissantlabs.hello-kitron-example

echo "→ overlaying the example sources"
cp "$EXAMPLE/App.tsx" "$TARGET/App.tsx"
cp "$EXAMPLE/kitron.config.ts" "$TARGET/kitron.config.ts"
rm -rf "$TARGET/backend"
cp -R "$EXAMPLE/backend" "$TARGET/backend"

cat <<EOF

done.

  cd "$TARGET"
  node "$ROOT/cli/src/kitron.ts" dev      # Metro + Debug build + launch
  node "$ROOT/cli/src/kitron.ts" build    # packaged .app
EOF
