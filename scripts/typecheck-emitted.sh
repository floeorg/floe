#!/usr/bin/env bash
#
# Type-check the TypeScript that `floe build` emits, against a ratchet.
#
# `floe build` writes `// @ts-nocheck` at the top of every file, so tsc
# checks none of the output. This script emits the example apps again with
# `--no-ts-nocheck` and runs tsc over the result. It finds the class of bug
# where Floe emits a name that nothing declares. See issue #1470.
#
# The output goes to `<example>/.floe/typecheck/`, which `.gitignore`
# already covers, so the committed `.ts` files stay untouched.
#
# The emitted code carries known diagnostics today, so a plain pass or fail
# would be red on every pull request. This script counts diagnostics per
# TypeScript error code and compares each count against the baseline file:
#
#   more than the baseline for any code -> fail, and name the codes
#   fewer for any code                  -> pass, and ask for a lower baseline
#   equal for every code                -> pass
#
# Usage:
#   scripts/typecheck-emitted.sh [path-to-floe-binary]
#   scripts/typecheck-emitted.sh --update-baseline [path-to-floe-binary]
#
# Environment:
#   FLOE                 path to the floe binary, if no argument is given
#   TYPECHECK_BASELINE   path to the baseline file, for testing this script

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

update_baseline="no"
if [ "${1:-}" = "--update-baseline" ]; then
  update_baseline="yes"
  shift
fi

floe_bin="${1:-${FLOE:-$repo_root/target/debug/floe}}"

# The build below runs from inside each example directory, so a relative
# binary path would stop resolving there. CI passes `./floe`.
if [ ! -x "$floe_bin" ]; then
  echo "no floe binary at: $floe_bin" >&2
  exit 2
fi
floe_bin="$(cd "$(dirname "$floe_bin")" && pwd)/$(basename "$floe_bin")"
tsc_bin="$repo_root/node_modules/.bin/tsc"
baseline_file="${TYPECHECK_BASELINE:-$repo_root/scripts/typecheck-emitted-baseline.txt}"

if [ ! -x "$floe_bin" ]; then
  echo "error: no floe binary at $floe_bin" >&2
  exit 1
fi
if [ ! -x "$tsc_bin" ]; then
  echo "error: no tsc at $tsc_bin. Run pnpm install --frozen-lockfile first." >&2
  exit 1
fi

work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir:?work_dir is unset}"' EXIT

diagnostics="$work_dir/diagnostics.txt"
: > "$diagnostics"

examples="store todo-app hono-api"

for example in $examples; do
  example_dir="$repo_root/examples/$example"
  out_dir="$example_dir/.floe/typecheck"

  rm -rf "${out_dir:?out_dir is unset}"
  mkdir -p "$out_dir"

  # `floe build` names the output after the source path relative to the
  # working directory, so run it from the example root.
  #
  # It exits non-zero when a file reported an error, and it still writes
  # the TypeScript. This job counts tsc diagnostics, so let it run on
  # what did get written rather than ending the whole ratchet here. A
  # Floe error is the `Floe check examples` job's to report.
  if ! ( cd "$example_dir" && "$floe_bin" build src/ --out-dir .floe/typecheck --no-ts-nocheck >/dev/null ); then
    echo "note: floe build reported errors in $example, counting the output it wrote" >&2
  fi

  # Reuse the example's own compiler options. Only the file set changes.
  cat > "$out_dir/tsconfig.json" <<'JSON'
{
  "extends": "../../tsconfig.json",
  "include": ["src/**/*.ts", "src/**/*.tsx"]
}
JSON

  # tsc exits non-zero when it reports anything, which is the normal state
  # here. The ratchet below decides pass or fail, so do not let `set -e` end
  # the run.
  "$tsc_bin" --noEmit -p "$out_dir/tsconfig.json" >> "$diagnostics" 2>&1 || true
done

# One diagnostic per line, in the form `path(line,col): error TSnnnn: text`.
# Count the codes only. A line number moves whenever an example changes; a
# code does not.
counts="$work_dir/counts.txt"
grep -oE '\): error TS[0-9]+:' "$diagnostics" \
  | grep -oE 'TS[0-9]+' \
  | sort \
  | uniq -c \
  | awk '{ print $2, $1 }' \
  | sort > "$counts"

total="$(awk '{ sum += $2 } END { print sum + 0 }' "$counts")"

if [ "$update_baseline" = "yes" ]; then
  {
    echo "# Ratchet baseline for scripts/typecheck-emitted.sh."
    echo "#"
    echo "# Each row is a TypeScript error code and how many times tsc reports it"
    echo "# over the emitted output of the example apps, with the \`// @ts-nocheck\`"
    echo "# header removed. The CI job fails when any count rises above its row."
    echo "#"
    echo "# These numbers must come down. Epic #1490 groups every diagnostic by"
    echo "# cause, and #1492 to #1499 break out the serious ones. Lower the row"
    echo "# in the same pull request that fixes the cause. The job prints the new"
    echo "# number for you when it sees a drop."
    echo "#"
    echo "# Regenerate with:"
    echo "#   pnpm install --frozen-lockfile"
    echo "#   cargo build -p floe"
    echo "#   scripts/typecheck-emitted.sh --update-baseline target/debug/floe"
    echo "#"
    echo "# Total at the time of writing: $total"
    cat "$counts"
  } > "$baseline_file"
  echo "wrote $baseline_file ($total diagnostics)"
  exit 0
fi

if [ ! -f "$baseline_file" ]; then
  echo "error: no baseline at $baseline_file" >&2
  echo "Create one with: scripts/typecheck-emitted.sh --update-baseline" >&2
  exit 1
fi

baseline="$work_dir/baseline.txt"
grep -vE '^\s*(#|$)' "$baseline_file" | sort > "$baseline"

# Join the two sets on the code, so a code that only one side names still
# gets a row, with zero on the other side.
report="$work_dir/report.txt"
join -a 1 -a 2 -e 0 -o '0,1.2,2.2' "$baseline" "$counts" | sort > "$report"

grew="$(awk '$3 > $2 { print $1 }' "$report" | tr '\n' ' ' | sed 's/ $//')"
shrank="$(awk '$3 < $2 { print $1 }' "$report" | tr '\n' ' ' | sed 's/ $//')"
baseline_total="$(awk '{ sum += $2 } END { print sum + 0 }' "$report")"

emit_table() {
  printf '| code | baseline | now | change |\n'
  printf '| --- | ---: | ---: | :---: |\n'
  awk '{
    change = "same"
    if ($3 > $2) change = "up"
    if ($3 < $2) change = "down"
    printf "| %s | %s | %s | %s |\n", $1, $2, $3, change
  }' "$report"
  printf '| **total** | **%s** | **%s** | |\n' "$baseline_total" "$total"
}

echo
echo "Diagnostics in the emitted TypeScript, by error code:"
echo
emit_table
echo

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "## Type-check of emitted TypeScript"
    echo
    echo "Baseline: \`scripts/typecheck-emitted-baseline.txt\`. Epic #1490 tracks the fixes."
    echo
    emit_table
  } >> "$GITHUB_STEP_SUMMARY"
fi

if [ -n "$grew" ]; then
  echo "FAIL: these error codes rose above the baseline: $grew"
  echo
  echo "Something in this change makes Floe emit TypeScript that does not"
  echo "type-check. The diagnostics for those codes are:"
  echo
  for code in $grew; do
    grep -E "\): error $code:" "$diagnostics" || true
  done
  echo
  echo "Fix the cause, or, if the rise is deliberate, raise the row in"
  echo "scripts/typecheck-emitted-baseline.txt and say why."
  exit 1
fi

if [ -n "$shrank" ]; then
  echo "PASS, and the count went down for: $shrank"
  echo
  echo "Lower those rows in scripts/typecheck-emitted-baseline.txt so the"
  echo "ratchet holds the gain. Run:"
  echo "  scripts/typecheck-emitted.sh --update-baseline <path-to-floe>"
  exit 0
fi

echo "PASS: $total diagnostics, the same as the baseline."
