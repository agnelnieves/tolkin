#!/usr/bin/env bash
# drift-check.sh: tokenizer drift watchdog for the Claude approximation.
#
# Compares tolkin's local cl100k-based Anthropic proxy count against the
# ground truth from Anthropic's count_tokens API (via `tolkin count --verify`)
# for every fixture in fixtures/drift/. Fails if any fixture drifts more than
# the threshold.
#
# Usage:
#   scripts/drift-check.sh [path-to-tolkin-binary]
#     Binary defaults to target/release/tolkin (relative to apps/tolkin-cli).
#
# Environment:
#   ANTHROPIC_API_KEY    Required for the real run. The CLI redacts text
#                        before sending it to api.anthropic.com.
#   DRIFT_THRESHOLD_PCT  Max allowed drift percent per fixture. Default 15.
#   DRIFT_SELFTEST=1     Plumbing self-test: compares the anthropic proxy
#                        against itself (drift is always 0.0), so the script
#                        can be validated without a network connection or an
#                        API key. Exists purely so CI-less environments can
#                        exercise the parsing, math, and threshold logic.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${1:-$ROOT/target/release/tolkin}"
FIXTURES_DIR="$ROOT/fixtures/drift"
THRESHOLD="${DRIFT_THRESHOLD_PCT:-15}"
SELFTEST="${DRIFT_SELFTEST:-0}"

if [[ ! -x "$BIN" ]]; then
  echo "error: tolkin binary not found or not executable at: $BIN" >&2
  echo "build it first: cargo build --release (in apps/tolkin-cli)" >&2
  exit 1
fi

if [[ "$SELFTEST" != "1" && -z "${ANTHROPIC_API_KEY:-}" ]]; then
  echo "error: ANTHROPIC_API_KEY is not set." >&2
  echo "The drift watchdog needs an Anthropic API key to fetch ground-truth" >&2
  echo "counts from the count_tokens endpoint. In CI, configure the" >&2
  echo "ANTHROPIC_API_KEY repository secret. Locally, export the variable." >&2
  echo "(For a key-less plumbing check, run with DRIFT_SELFTEST=1.)" >&2
  exit 1
fi

json_field() {
  # json_field <python-expression-over-d> ; JSON arrives on stdin.
  python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"
}

if [[ "$SELFTEST" == "1" ]]; then
  echo "mode: SELFTEST (proxy vs proxy, expected drift 0.0 everywhere)"
else
  echo "mode: real (proxy vs Anthropic count_tokens)"
fi
echo "threshold: ${THRESHOLD}% per fixture"
echo
printf '%-22s %10s %10s %8s\n' "fixture" "proxy" "verified" "drift%"

failures=()
for fixture in "$FIXTURES_DIR"/*.txt; do
  name="$(basename "$fixture")"

  proxy="$("$BIN" count --model anthropic --json "$fixture" | json_field 'd["tokens"]')"

  if [[ "$SELFTEST" == "1" ]]; then
    verified="$("$BIN" count --model anthropic --json "$fixture" | json_field 'd["tokens"]')"
  else
    verified="$("$BIN" count --verify --json "$fixture" | json_field 'd["anthropic_verified"]["tokens"]')"
  fi

  drift="$(python3 -c "print(f'{abs($verified - $proxy) / $verified * 100:.1f}')")"
  printf '%-22s %10s %10s %8s\n' "$name" "$proxy" "$verified" "$drift"

  exceeds="$(python3 -c "print(1 if $drift > $THRESHOLD else 0)")"
  if [[ "$exceeds" == "1" ]]; then
    failures+=("$name: proxy $proxy vs verified $verified (${drift}% > ${THRESHOLD}%)")
  fi
done

echo
if (( ${#failures[@]} > 0 )); then
  echo "DRIFT THRESHOLD EXCEEDED (${THRESHOLD}%):" >&2
  for f in "${failures[@]}"; do
    echo "  $f" >&2
  done
  echo >&2
  echo "The UI advertises a +/-10 percent confidence band for Claude counts." >&2
  echo "A breach here means the badge copy and the band need revisiting." >&2
  exit 1
fi

echo "OK: all fixtures within ${THRESHOLD}% of ground truth."
