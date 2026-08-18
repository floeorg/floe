#!/usr/bin/env bash
#
# Type-check the TypeScript that `floe build` emits.
#
# `floe build` writes `// @ts-nocheck` at the top of every file, so tsc
# checks none of the output. This script emits the example apps again with
# `--no-ts-nocheck` and runs tsc over the result. It finds the class of bug
# where Floe emits a name that nothing declares. See issue #1470.
#
# The output goes to `<example>/.floe/typecheck/`, which `.gitignore`
# already covers, so the committed `.ts` files stay untouched.
#
# Usage: scripts/typecheck-emitted.sh [path-to-floe-binary]

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
floe_bin="${1:-${FLOE:-$repo_root/target/debug/floe}}"
tsc_bin="$repo_root/node_modules/.bin/tsc"

if [ ! -x "$floe_bin" ]; then
  echo "error: no floe binary at $floe_bin" >&2
  exit 1
fi
if [ ! -x "$tsc_bin" ]; then
  echo "error: no tsc at $tsc_bin. Run pnpm install --frozen-lockfile first." >&2
  exit 1
fi

examples="store todo-app hono-api"
failed=""

for example in $examples; do
  example_dir="$repo_root/examples/$example"
  out_dir="$example_dir/.floe/typecheck"

  rm -rf "${out_dir:?out_dir is unset}"
  mkdir -p "$out_dir"

  # `floe build` names the output after the source path relative to the
  # working directory, so run it from the example root.
  ( cd "$example_dir" && "$floe_bin" build src/ --out-dir .floe/typecheck --no-ts-nocheck >/dev/null )

  # Reuse the example's own compiler options. Only the file set changes.
  cat > "$out_dir/tsconfig.json" <<'JSON'
{
  "extends": "../../tsconfig.json",
  "include": ["src/**/*.ts", "src/**/*.tsx"]
}
JSON

  echo "── $example ─────────────────────────────────────────"
  if "$tsc_bin" --noEmit -p "$out_dir/tsconfig.json"; then
    echo "  no diagnostics"
  else
    failed="$failed $example"
  fi
done

if [ -n "$failed" ]; then
  echo
  echo "tsc reported diagnostics for:$failed"
  exit 1
fi

echo
echo "tsc reported no diagnostics for any example"
